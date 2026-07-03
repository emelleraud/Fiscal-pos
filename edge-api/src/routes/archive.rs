//! # `routes::archive`
//!
//! Route de génération de l'archive fiscale annuelle (NF525 §7).
//!
//! ## POST /api/v1/archive/{year}
//!
//! Déclenche la génération de l'archive CSV signée pour une année civile.
//! - Requiert la variable d'environnement `FISCAL_SIGNING_KEY_HEX`
//!   (clé privée Ed25519, encodée en 64 caractères hexadécimaux).
//! - Idempotent : retourne 409 si l'archive de l'année existe déjà.
//! - Retourne 404 si aucune transaction n'existe pour l'année demandée.
//!
//! ## Sécurité
//! Cette route est réservée au manager. En production, elle devrait être
//! protégée par un middleware d'authentification (PIN ou badge). Pour le
//! MVP, elle est accessible sur le LAN sans authentification.
//!
//! ## Fichier généré
//! `{DATA_DIR}/archives/{year}.csv` — CSV UTF-8 BOM, séparateur `;`.

use std::fs;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use fiscal_engine::{
    archive_engine::{archive_exists, signing_key_from_bytes},
    errors::{ArchiveError, FiscalError},
    export_archive, hex_decode, hex_encode, persist_archive_metadata, sign_archive,
};
use serde::Serialize;
use tracing::info;

use crate::{app::AppState, error::ApiErr};

// ---------------------------------------------------------------------------
// DTO réponse
// ---------------------------------------------------------------------------

/// Réponse de la route POST /api/v1/archive/{year}.
#[derive(Debug, Serialize)]
pub struct ArchiveResponse {
    /// Année fiscale archivée.
    pub year: u32,
    /// Nombre d'entrées dans l'archive.
    pub entry_count: u64,
    /// Nombre de sessions couvertes.
    pub session_count: u64,
    /// Numéro de séquence de la première entrée.
    pub first_sequence: u64,
    /// Numéro de séquence de la dernière entrée.
    pub last_sequence: u64,
    /// Timestamp de génération (Unix ms).
    pub generated_at_ms: u64,
    /// Chemin du fichier CSV sur disque.
    pub csv_path: String,
    /// SHA-256 du CSV (hex 64 caractères).
    pub csv_hash_hex: String,
    /// Signature Ed25519 du `csv_hash` (hex 128 caractères).
    pub signature_hex: String,
    /// Clé publique Ed25519 (hex 64 caractères).
    pub public_key_hex: String,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `POST /api/v1/archive/{year}`
///
/// Génère l'archive fiscale annuelle CSV + signature Ed25519.
///
/// # Réponses
/// - `201 Created` — archive générée et persistée
/// - `404 Not Found` — aucune transaction pour l'année
/// - `409 Conflict` — archive déjà générée pour cette année
/// - `503 Service Unavailable` — `FISCAL_SIGNING_KEY_HEX` manquant
///
/// # Errors
/// Returns [`ApiErr`] on fiscal engine or database errors.
pub async fn generate_archive_handler(
    Path(year): Path<u32>,
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<ArchiveResponse>), ApiErr> {
    let pool = state.journal.store().pool.clone();

    // 1. Charger et valider la clé de signature
    let signing_key = load_signing_key().map_err(|e| {
        ApiErr(FiscalError::Archive(ArchiveError::InvalidSigningKey {
            reason: e,
        }))
    })?;

    // 2. Idempotence : refuser si l'archive existe déjà
    if archive_exists(&pool, year).await.map_err(ApiErr::from)? {
        return Err(ApiErr(FiscalError::Archive(
            ArchiveError::ArchiveAlreadyExists { year },
        )));
    }

    // 3. Générer le CSV (charge toutes les entrées de l'année)
    let export = export_archive(&pool, year).await.map_err(ApiErr::from)?;

    // 4. Signer avec Ed25519
    let site_id = std::env::var("SITE_ID").unwrap_or_else(|_| "UNKNOWN".to_string());
    let software_version = env!("CARGO_PKG_VERSION");
    let signed =
        sign_archive(export, &signing_key, software_version, &site_id).map_err(ApiErr::from)?;

    // 5. Écrire le CSV sur disque
    let archives_dir = format!("{}/archives", state.data_dir);
    fs::create_dir_all(&archives_dir).map_err(|e| {
        ApiErr(FiscalError::Archive(ArchiveError::CsvGenerationFailed {
            year,
            reason: format!("Impossible de créer le répertoire archives : {e}"),
        }))
    })?;

    let csv_path = format!("{archives_dir}/{year}.csv");
    fs::write(&csv_path, &signed.export.csv_content).map_err(|e| {
        ApiErr(FiscalError::Archive(ArchiveError::CsvGenerationFailed {
            year,
            reason: format!("Impossible d'écrire {csv_path} : {e}"),
        }))
    })?;

    // 6. Persister les métadonnées dans SQLite
    persist_archive_metadata(&pool, &signed, &csv_path)
        .await
        .map_err(ApiErr::from)?;

    info!(
        year = year,
        entry_count = signed.export.entry_count,
        csv_path = %csv_path,
        "Archive fiscale annuelle générée"
    );

    // 7. Construire la réponse
    let response = ArchiveResponse {
        year: signed.export.year,
        entry_count: signed.export.entry_count,
        session_count: signed.export.session_count,
        first_sequence: signed.export.first_sequence,
        last_sequence: signed.export.last_sequence,
        generated_at_ms: signed.export.generated_at_ms,
        csv_path,
        csv_hash_hex: hex_encode(&signed.export.csv_hash),
        signature_hex: hex_encode_64(&signed.signature),
        public_key_hex: hex_encode(&signed.public_key),
    };

    Ok((StatusCode::CREATED, Json(response)))
}

// ---------------------------------------------------------------------------
// Utilitaires privés
// ---------------------------------------------------------------------------

/// Charge la clé de signature depuis `FISCAL_SIGNING_KEY_HEX`.
///
/// La variable doit contenir 64 caractères hexadécimaux (32 octets Ed25519).
fn load_signing_key() -> Result<ed25519_dalek::SigningKey, String> {
    let hex = std::env::var("FISCAL_SIGNING_KEY_HEX")
        .map_err(|_| "Variable d'environnement FISCAL_SIGNING_KEY_HEX manquante".to_string())?;

    let bytes = hex_decode(&hex).map_err(|e| format!("FISCAL_SIGNING_KEY_HEX invalide : {e}"))?;

    signing_key_from_bytes(&bytes).map_err(|e| format!("Clé Ed25519 invalide : {e}"))
}

/// Encode 64 octets (signature Ed25519) en chaîne hexadécimale de 128 caractères.
fn hex_encode_64(bytes: &[u8; 64]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(128), |mut s, b| {
        write!(s, "{b:02x}").expect("writing to String is infallible");
        s
    })
}
