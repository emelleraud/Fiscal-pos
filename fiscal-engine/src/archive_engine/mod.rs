//! # `archive_engine`
//!
//! Génération et signature des archives annuelles fiscales (NF525 §7).
//!
//! ## Obligation légale
//! Chaque année civile, le logiciel de caisse doit produire un fichier CSV
//! signé numériquement contenant toutes les transactions de l'exercice.
//! Ce fichier est conservé 6 ans et peut être demandé en cas de contrôle fiscal.
//!
//! ## Pipeline de génération
//! ```text
//! export_archive(pool, year)
//!   │
//!   ├── 1. Charger les entrées de l'année depuis SQLite
//!   ├── 2. Vérifier qu'au moins une entrée existe
//!   ├── 3. Générer le CSV (UTF-8 BOM, séparateur ';')
//!   ├── 4. Calculer le SHA-256 du CSV
//!   └── → ArchiveExport { csv_content, csv_hash, ... }
//!
//! sign_archive(export, signing_key, public_key, version, site_id)
//!   │
//!   ├── 1. Signer csv_hash avec Ed25519
//!   └── → SignedArchive { export, signature, public_key, ... }
//!
//! persist_archive(pool, signed_archive, csv_path)
//!   └── INSERT archive_metadata (idempotence : erreur si déjà présent)
//! ```
//!
//! ## Format CSV (NF525 §7.3 — colonnes fixes, ordre immuable)
//! ```text
//! sequence;id;session_id;operation_type;amount_ttc_cents;ht_cents;tva_cents;
//! tva_rate;hash_hex;previous_hash_hex;created_at_ms;reason;order_reference
//! ```
//! - Encodage UTF-8 avec BOM (`\xEF\xBB\xBF`) pour compatibilité Excel française
//! - Séparateur `;` (virgule réservée aux décimaux en France)
//! - En-tête sur la première ligne
//! - Une ligne par `FiscalEntry`, ordre chronologique par `sequence_number`
//! - `reason` et `order_reference` vides si NULL

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};
use sqlx::sqlite::SqlitePool;

use crate::{
    errors::{ArchiveError, FiscalError},
    hash_engine::hex_encode,
    journal::store::JournalStore,
    types::{
        archive::{ArchiveExport, SignedArchive},
        entry::FiscalEntry,
    },
};

// ---------------------------------------------------------------------------
// UTF-8 BOM
// ---------------------------------------------------------------------------

/// BOM UTF-8 — préfixe obligatoire pour la compatibilité Excel française.
const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// En-tête CSV — colonnes fixes (ordre immuable NF525 §7.3).
const CSV_HEADER: &str = "sequence;id;session_id;operation_type;amount_ttc_cents;\
ht_cents;tva_cents;tva_rate;hash_hex;previous_hash_hex;created_at_ms;reason;order_reference";

// ---------------------------------------------------------------------------
// Export CSV
// ---------------------------------------------------------------------------

/// Génère l'export CSV d'une année fiscale.
///
/// Charge toutes les entrées de l'année depuis `SQLite`, génère le CSV,
/// calcule le SHA-256 du contenu et retourne un `ArchiveExport` non signé.
///
/// # Arguments
/// * `pool` - Pool `SQLite`.
/// * `year` - Année civile (ex: 2024).
///
/// # Errors
/// - `ArchiveError::NoDataForYear` si aucune entrée n'existe pour cette année
/// - `FiscalError::Database` sur erreur `SQLite`
///
/// # Examples
/// ```no_run
/// use fiscal_engine::archive_engine::export_archive;
///
/// # async fn example(pool: sqlx::sqlite::SqlitePool)
/// #     -> Result<(), fiscal_engine::errors::FiscalError> {
/// let export = export_archive(&pool, 2024).await?;
/// println!("{} entrées exportées", export.entry_count);
/// # Ok(())
/// # }
/// ```
pub async fn export_archive(pool: &SqlitePool, year: u32) -> Result<ArchiveExport, FiscalError> {
    // 1. Charger les entrées de l'année
    let store = JournalStore { pool: pool.clone() };
    let entries = store.load_entries_for_year(year).await?;

    if entries.is_empty() {
        return Err(ArchiveError::NoDataForYear { year }.into());
    }

    // 2. Compter les sessions distinctes couvertes
    let session_count = count_distinct_sessions(&entries);
    let first_sequence = entries.first().map_or(1, |e| e.sequence_number);
    let last_sequence = entries.last().map_or(1, |e| e.sequence_number);
    let entry_count = entries.len() as u64;

    // 3. Générer le CSV
    let csv_content = generate_csv(&entries).map_err(|reason| {
        FiscalError::Archive(ArchiveError::CsvGenerationFailed { year, reason })
    })?;

    // 4. SHA-256 du CSV (inclut le BOM)
    let csv_hash = sha256_bytes(&csv_content);

    Ok(ArchiveExport {
        year,
        entry_count,
        session_count,
        first_sequence,
        last_sequence,
        generated_at_ms: now_ms(),
        csv_content,
        csv_hash,
    })
}

// ---------------------------------------------------------------------------
// Signature Ed25519
// ---------------------------------------------------------------------------

/// Signe un `ArchiveExport` avec une clé Ed25519.
///
/// La signature couvre le `csv_hash` de l'export (SHA-256 du CSV).
/// La clé privée ne doit jamais être persistée — elle est chargée depuis
/// une variable d'environnement ou un HSM au moment de la signature.
///
/// # Arguments
/// * `export` - Export CSV non signé.
/// * `signing_key` - Clé Ed25519 privée (32 octets, depuis `FISCAL_SIGNING_KEY_B64`).
/// * `software_version` - Version du logiciel (doit correspondre à la certification LNE).
/// * `site_id` - SIRET ou identifiant réseau du site.
///
/// # Errors
/// - `ArchiveError::SigningFailed` si la signature échoue (ne devrait pas arriver avec ed25519-dalek)
///
/// # Examples
/// ```no_run
/// use fiscal_engine::archive_engine::{export_archive, sign_archive};
/// use ed25519_dalek::SigningKey;
///
/// # async fn example(pool: sqlx::sqlite::SqlitePool, key: SigningKey)
/// #     -> Result<(), fiscal_engine::errors::FiscalError> {
/// let export = export_archive(&pool, 2024).await?;
/// let signed = sign_archive(export, &key, "1.0.0", "12345678900000")?;
/// println!("Archive signée, clé publique: {:?}", signed.public_key);
/// # Ok(())
/// # }
/// ```
pub fn sign_archive(
    export: ArchiveExport,
    signing_key: &SigningKey,
    software_version: &str,
    site_id: &str,
) -> Result<SignedArchive, FiscalError> {
    // Signer le hash SHA-256 du CSV (pas le CSV entier — Ed25519 est sur 32 octets)
    let signature: Signature = signing_key.sign(&export.csv_hash);

    let verifying_key: VerifyingKey = signing_key.verifying_key();
    let public_key_bytes = verifying_key.to_bytes();
    let signature_bytes = signature.to_bytes();

    Ok(SignedArchive {
        export,
        signature: signature_bytes,
        public_key: public_key_bytes,
        software_version: software_version.to_string(),
        site_id: site_id.to_string(),
    })
}

/// Vérifie la signature Ed25519 d'une archive signée.
///
/// Utilisé par les auditeurs LNE pour valider une archive après réception.
/// La vérification recalcule le SHA-256 du CSV et vérifie la signature.
///
/// # Errors
/// - `ArchiveError::SigningFailed` si la signature est invalide
///
/// # Examples
/// ```no_run
/// use fiscal_engine::archive_engine::verify_archive_signature;
/// use fiscal_engine::types::archive::SignedArchive;
///
/// # fn example(archive: SignedArchive) -> Result<(), fiscal_engine::errors::FiscalError> {
/// verify_archive_signature(&archive)?;
/// println!("Signature valide");
/// # Ok(())
/// # }
/// ```
pub fn verify_archive_signature(archive: &SignedArchive) -> Result<(), FiscalError> {
    let year = archive.export.year;

    // 1. Recalculer le hash du CSV
    let computed_hash = sha256_bytes(&archive.export.csv_content);
    if computed_hash != archive.export.csv_hash {
        return Err(ArchiveError::SigningFailed {
            year,
            reason: "Le hash SHA-256 du CSV ne correspond pas au hash stocké".to_string(),
        }
        .into());
    }

    // 2. Reconstruire la clé publique
    let verifying_key = VerifyingKey::from_bytes(&archive.public_key).map_err(|e| {
        ArchiveError::InvalidSigningKey {
            reason: format!("Clé publique invalide: {e}"),
        }
    })?;

    // 3. Reconstruire la signature
    let signature = Signature::from_bytes(&archive.signature);

    // 4. Vérifier
    verifying_key
        .verify(&archive.export.csv_hash, &signature)
        .map_err(|e| {
            ArchiveError::SigningFailed {
                year,
                reason: format!("Signature Ed25519 invalide: {e}"),
            }
            .into()
        })
}

// ---------------------------------------------------------------------------
// Persistence de la métadonnée d'archive
// ---------------------------------------------------------------------------

/// Persiste les métadonnées d'une archive signée dans `archive_metadata`.
///
/// Le fichier CSV lui-même est écrit sur disque par l'appelant (edge-api).
/// Cette fonction ne stocke que les métadonnées pour vérification ultérieure.
///
/// # Arguments
/// * `pool` - Pool `SQLite`.
/// * `archive` - Archive signée dont on persiste les métadonnées.
/// * `csv_path` - Chemin du fichier CSV sur disque.
///
/// # Errors
/// - `ArchiveError::ArchiveAlreadyExists` si l'année est déjà en base
/// - `FiscalError::Database` sur erreur `SQLite`
pub async fn persist_archive_metadata(
    pool: &SqlitePool,
    archive: &SignedArchive,
    csv_path: &str,
) -> Result<(), FiscalError> {
    let year = archive.export.year;

    // Vérifier l'idempotence (une archive par an)
    let existing = sqlx::query("SELECT year FROM archive_metadata WHERE year = ?")
        .bind(i64::from(year))
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        return Err(ArchiveError::ArchiveAlreadyExists { year }.into());
    }

    sqlx::query(
        "INSERT INTO archive_metadata (
            year, entry_count, session_count, first_sequence, last_sequence,
            generated_at_ms, csv_hash_hex, signature_hex, public_key_hex,
            software_version, site_id, csv_path
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(i64::from(year))
    .bind(archive.export.entry_count.cast_signed())
    .bind(archive.export.session_count.cast_signed())
    .bind(archive.export.first_sequence.cast_signed())
    .bind(archive.export.last_sequence.cast_signed())
    .bind(archive.export.generated_at_ms.cast_signed())
    .bind(hex_encode(&archive.export.csv_hash))
    .bind(hex_encode_64(&archive.signature))
    .bind(hex_encode(&archive.public_key))
    .bind(&archive.software_version)
    .bind(&archive.site_id)
    .bind(csv_path)
    .execute(pool)
    .await?;

    Ok(())
}

/// Vérifie que l'archive d'une année a déjà été générée.
///
/// # Errors
/// `FiscalError::Database` sur erreur `SQLite`.
pub async fn archive_exists(pool: &SqlitePool, year: u32) -> Result<bool, FiscalError> {
    let row = sqlx::query("SELECT year FROM archive_metadata WHERE year = ?")
        .bind(i64::from(year))
        .fetch_optional(pool)
        .await?;
    Ok(row.is_some())
}

// ---------------------------------------------------------------------------
// Génération du CSV
// ---------------------------------------------------------------------------

/// Génère le contenu CSV complet avec BOM UTF-8.
fn generate_csv(entries: &[FiscalEntry]) -> Result<Vec<u8>, String> {
    let mut output = Vec::with_capacity(entries.len() * 200 + 100);

    // BOM UTF-8 (obligatoire pour Excel français)
    output.extend_from_slice(UTF8_BOM);

    // En-tête
    output.extend_from_slice(CSV_HEADER.as_bytes());
    output.push(b'\n');

    for entry in entries {
        let line = format_csv_line(entry)?;
        output.extend_from_slice(line.as_bytes());
        output.push(b'\n');
    }

    Ok(output)
}

/// Formate une entrée fiscale en ligne CSV.
///
/// Champs dans l'ordre NF525 §7.3 (ordre immuable).
/// Les champs texte optionnels sont vides si `None`.
fn format_csv_line(entry: &FiscalEntry) -> Result<String, String> {
    let tva_rate_str = match entry.tva_breakdown.rate {
        crate::types::tva::TvaRate::Reduit5_5 => "5.5",
        crate::types::tva::TvaRate::Intermediaire10 => "10",
        crate::types::tva::TvaRate::Normal20 => "20",
    };

    let reason = entry.reason.as_deref().unwrap_or("");
    let order_ref = entry.order_reference.as_deref().unwrap_or("");

    // Sanitisation : les champs texte ne doivent pas contenir de ';' ni de '\n'
    if reason.contains(';') || reason.contains('\n') {
        return Err(format!(
            "Motif invalide à la séquence {} : contient un séparateur CSV",
            entry.sequence_number
        ));
    }

    Ok(format!(
        "{seq};{id};{session_id};{op};{ttc};{ht};{tva};{rate};\
         {hash_hex};{prev_hex};{ts};{reason};{order_ref}",
        seq = entry.sequence_number,
        id = entry.id.0,
        session_id = entry.session_id.0,
        op = operation_type_label(entry.operation_type),
        ttc = entry.amount_ttc_cents.0,
        ht = entry.tva_breakdown.ht_cents.0,
        tva = entry.tva_breakdown.tva_cents.0,
        rate = tva_rate_str,
        hash_hex = hex_encode(&entry.hash),
        prev_hex = hex_encode(&entry.previous_hash),
        ts = entry.created_at_ms,
        reason = reason,
        order_ref = order_ref,
    ))
}

fn operation_type_label(op: crate::types::operation::OperationType) -> &'static str {
    use crate::types::operation::OperationType;
    match op {
        OperationType::Sale => "Sale",
        OperationType::Refund => "Refund",
        OperationType::Cancel => "Cancel",
        OperationType::Discount => "Discount",
        OperationType::ZClose => "ZClose",
    }
}

// ---------------------------------------------------------------------------
// Génération de clé (utilitaire pour le binaire `keygen`)
// ---------------------------------------------------------------------------

/// Génère une nouvelle paire de clés Ed25519 pour la signature des archives.
///
/// À utiliser une seule fois lors de la mise en production.
/// La clé privée doit être stockée de manière sécurisée (HSM ou coffre-fort).
///
/// # Returns
/// `(clé_privée_bytes, clé_publique_bytes)` — chacune en 32 octets.
///
/// # Examples
/// ```
/// use fiscal_engine::archive_engine::generate_signing_keypair;
/// let (private_bytes, public_bytes) = generate_signing_keypair();
/// assert_eq!(private_bytes.len(), 32);
/// assert_eq!(public_bytes.len(), 32);
/// ```
#[must_use]
pub fn generate_signing_keypair() -> ([u8; 32], [u8; 32]) {
    use rand::rngs::OsRng;
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    (signing_key.to_bytes(), verifying_key.to_bytes())
}

/// Charge une `SigningKey` depuis des octets bruts (32 octets).
///
/// Utilisé pour charger la clé depuis la variable d'environnement
/// `FISCAL_SIGNING_KEY_B64` (décodée en base64 avant appel).
///
/// # Errors
/// - `ArchiveError::InvalidSigningKey` si les octets sont invalides
///
/// # Examples
/// ```
/// use fiscal_engine::archive_engine::{generate_signing_keypair, signing_key_from_bytes};
/// let (private_bytes, _) = generate_signing_keypair();
/// let key = signing_key_from_bytes(&private_bytes).unwrap();
/// ```
pub fn signing_key_from_bytes(bytes: &[u8; 32]) -> Result<SigningKey, FiscalError> {
    Ok(SigningKey::from_bytes(bytes))
}

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// Encode 64 octets (signature Ed25519) en hex 128 caractères.
fn hex_encode_64(bytes: &[u8; 64]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(128), |mut acc, b| {
        write!(acc, "{b:02x}").expect("writing to String is infallible");
        acc
    })
}

fn count_distinct_sessions(entries: &[FiscalEntry]) -> u64 {
    use std::collections::HashSet;
    entries
        .iter()
        .map(|e| e.session_id.0)
        .collect::<HashSet<_>>()
        .len()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        journal::{store::JournalStore, Journal},
        types::{
            entry::FiscalEntryData,
            operation::OperationType,
            tva::{TvaBreakdown, TvaRate},
        },
        z_report_engine::generate_z_report,
    };
    use common::Cents;
    use rand::rngs::OsRng;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Pool SQLite");
        sqlx::query(include_str!("../../migrations/0001_initial_schema.sql"))
            .execute(&pool)
            .await
            .expect("Migration 0001");
        sqlx::query(include_str!("../../migrations/0002_z_reports_archives.sql"))
            .execute(&pool)
            .await
            .expect("Migration 0002");
        sqlx::query(include_str!("../../migrations/0003_sessions_sync.sql"))
            .execute(&pool)
            .await
            .expect("Migration 0003");
        sqlx::query(include_str!("../../migrations/0004_z_reports_sync.sql"))
            .execute(&pool)
            .await
            .expect("Migration 0004");
        sqlx::query(include_str!("../../migrations/0005_multi_tva.sql"))
            .execute(&pool)
            .await
            .expect("Migration 0005");
        pool
    }

    async fn journal_from_pool(pool: &SqlitePool) -> Journal {
        let store = JournalStore { pool: pool.clone() };
        Journal::from_store(store).await.expect("Journal")
    }

    fn fresh_signing_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    async fn populate_journal(pool: &SqlitePool, n_sales: u64) -> common::SessionId {
        let journal = journal_from_pool(pool).await;
        let session = journal.open_session().await.expect("Session");
        for i in 1..=n_sales {
            journal
                .record_transaction(FiscalEntryData {
                    session_id: session.id,
                    operation_type: OperationType::Sale,
                    amount_ttc_cents: Cents((i * 100) as i64),
                    tva_breakdown: TvaBreakdown::from_ttc(
                        Cents((i * 100) as i64),
                        TvaRate::Intermediaire10,
                    ),
                    tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
                    tva_10_breakdown: TvaBreakdown::from_ttc(
                        Cents((i * 100) as i64),
                        TvaRate::Intermediaire10,
                    ),
                    tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
                    reason: None,
                    order_reference: Some(format!("ORD-{i:04}")),
                })
                .await
                .expect("Vente");
        }
        journal.close_session().await.expect("Clôture");
        generate_z_report(pool, session.id)
            .await
            .expect("Rapport Z");
        session.id
    }

    // --- CSV ---

    #[test]
    fn csv_starts_with_bom() {
        use crate::hash_engine::build_entry_for_test;
        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            1_700_000_000_000,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV généré");
        assert_eq!(
            &csv[..3],
            UTF8_BOM,
            "Le CSV doit commencer par le BOM UTF-8"
        );
    }

    #[test]
    fn csv_header_is_correct() {
        use crate::hash_engine::build_entry_for_test;
        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV généré");
        let content = std::str::from_utf8(&csv[3..]).expect("UTF-8 valide");
        let first_line = content.lines().next().expect("Au moins une ligne");
        assert_eq!(first_line, CSV_HEADER);
    }

    #[test]
    fn csv_line_has_correct_column_count() {
        use crate::hash_engine::build_entry_for_test;
        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            common::GENESIS_HASH,
        );
        let line = format_csv_line(&entry).expect("Ligne CSV");
        let col_count = line.split(';').count();
        // En-tête a 13 colonnes → chaque ligne doit en avoir 13
        assert_eq!(
            col_count, 13,
            "Chaque ligne doit avoir 13 colonnes, obtenu {col_count}"
        );
    }

    #[test]
    fn csv_hash_is_sha256_of_content() {
        use crate::hash_engine::build_entry_for_test;
        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            500,
            TvaRate::Reduit5_5,
            0,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV");
        let computed = sha256_bytes(&csv);
        // On vérifie que la fonction est déterministe
        let computed2 = sha256_bytes(&csv);
        assert_eq!(computed, computed2);
        assert_ne!(computed, [0u8; 32]);
    }

    // --- Export ---

    #[tokio::test]
    async fn export_archive_no_data_fails() {
        let pool = setup_pool().await;
        let result = export_archive(&pool, 2024).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Archive(ArchiveError::NoDataForYear { year }) => {
                assert_eq!(year, 2024);
            }
            other => panic!("Attendu NoDataForYear, obtenu: {other}"),
        }
    }

    #[tokio::test]
    async fn export_archive_with_data_succeeds() {
        let pool = setup_pool().await;
        populate_journal(&pool, 5).await;

        // Utiliser l'année courante pour la correspondance avec now_ms()
        // En test, on utilise 1970 car les timestamps sont à 0 dans build_entry_for_test
        // mais ici on a des timestamps réels — on cherche l'année des entrées créées
        let year = current_year();
        let result = export_archive(&pool, year).await;
        assert!(result.is_ok(), "Export doit réussir : {:?}", result.err());

        let export = result.unwrap();
        // 5 ventes + 1 ZClose
        assert_eq!(export.entry_count, 6);
        assert_eq!(export.year, year);
        assert_ne!(export.csv_hash, [0u8; 32]);
        assert!(!export.csv_content.is_empty());
    }

    // --- Signature Ed25519 ---

    #[test]
    fn sign_and_verify_archive_roundtrip() {
        use crate::hash_engine::build_entry_for_test;

        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV");
        let csv_hash = sha256_bytes(&csv);

        let export = ArchiveExport {
            year: 2024,
            entry_count: 1,
            session_count: 1,
            first_sequence: 1,
            last_sequence: 1,
            generated_at_ms: 0,
            csv_content: csv,
            csv_hash,
        };

        let key = fresh_signing_key();
        let signed = sign_archive(export, &key, "1.0.0", "12345678900000").expect("Signature");

        // La vérification doit réussir
        assert!(verify_archive_signature(&signed).is_ok());
    }

    #[test]
    fn tampered_csv_fails_verification() {
        use crate::hash_engine::build_entry_for_test;

        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV");
        let csv_hash = sha256_bytes(&csv);

        let export = ArchiveExport {
            year: 2024,
            entry_count: 1,
            session_count: 1,
            first_sequence: 1,
            last_sequence: 1,
            generated_at_ms: 0,
            csv_content: csv,
            csv_hash,
        };

        let key = fresh_signing_key();
        let mut signed =
            sign_archive(export.clone(), &key, "1.0.0", "12345678900000").expect("Signature");

        // Altérer le contenu CSV après signature
        signed.export.csv_content.push(b'X');

        let result = verify_archive_signature(&signed);
        assert!(result.is_err(), "Un CSV altéré doit invalider la signature");
    }

    #[test]
    fn wrong_key_fails_verification() {
        use crate::hash_engine::build_entry_for_test;

        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV");
        let csv_hash = sha256_bytes(&csv);

        let export = ArchiveExport {
            year: 2024,
            entry_count: 1,
            session_count: 1,
            first_sequence: 1,
            last_sequence: 1,
            generated_at_ms: 0,
            csv_content: csv,
            csv_hash,
        };

        let key1 = fresh_signing_key();
        let key2 = fresh_signing_key();

        let mut signed = sign_archive(export, &key1, "1.0.0", "12345678900000").expect("Signature");

        // Remplacer la clé publique par celle d'une autre paire
        signed.public_key = key2.verifying_key().to_bytes();

        let result = verify_archive_signature(&signed);
        assert!(
            result.is_err(),
            "Une mauvaise clé publique doit invalider la vérification"
        );
    }

    // --- Génération de clé ---

    #[test]
    fn generate_keypair_produces_32_byte_keys() {
        let (priv_bytes, pub_bytes) = generate_signing_keypair();
        assert_eq!(priv_bytes.len(), 32);
        assert_eq!(pub_bytes.len(), 32);
    }

    #[test]
    fn signing_key_from_bytes_roundtrip() {
        let (priv_bytes, _) = generate_signing_keypair();
        let key = signing_key_from_bytes(&priv_bytes).expect("Clé valide");
        // Vérifier que la clé reconstruite donne le même résultat
        assert_eq!(key.to_bytes(), priv_bytes);
    }

    // --- Persistence ---

    #[tokio::test]
    async fn persist_archive_metadata_idempotence() {
        use crate::hash_engine::build_entry_for_test;

        let pool = setup_pool().await;
        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            common::GENESIS_HASH,
        );
        let csv = generate_csv(&[entry]).expect("CSV");
        let csv_hash = sha256_bytes(&csv);
        let export = ArchiveExport {
            year: 2023,
            entry_count: 1,
            session_count: 1,
            first_sequence: 1,
            last_sequence: 1,
            generated_at_ms: 0,
            csv_content: csv,
            csv_hash,
        };
        let key = fresh_signing_key();
        let signed = sign_archive(export, &key, "1.0.0", "SIRET001").expect("Signature");

        persist_archive_metadata(&pool, &signed, "/archives/2023.csv")
            .await
            .expect("Première persistence");

        // Deuxième tentative doit échouer
        let result = persist_archive_metadata(&pool, &signed, "/archives/2023.csv").await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Archive(ArchiveError::ArchiveAlreadyExists { year }) => {
                assert_eq!(year, 2023);
            }
            other => panic!("Attendu ArchiveAlreadyExists, obtenu: {other}"),
        }
    }

    fn current_year() -> u32 {
        // Calcul simple de l'année courante depuis Unix timestamp sans chrono
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        // Approximation : 365.25 jours/an en moyenne
        1970 + (secs / (365 * 24 * 3600 + 20952)) as u32
    }
}
