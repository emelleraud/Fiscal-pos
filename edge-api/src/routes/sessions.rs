//! # routes::sessions
//!
//! Routes de gestion des sessions de caisse.
//!
//! ## Routes
//! - `GET  /api/v1/sessions/current` — état de la session active
//! - `POST /api/v1/sessions/open`    — ouverture d'une nouvelle session
//! - `POST /api/v1/sessions/close`   — clôture + génération du rapport Z

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::{app::AppState, error::ApiErr};
use fiscal_engine::{generate_z_report, z_report_engine::format_z_report_text, ZReport};

// ---------------------------------------------------------------------------
// DTOs de réponse
// ---------------------------------------------------------------------------

/// Réponse représentant l'état d'une session.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionResponse {
    /// Identifiant de la session.
    pub session_id: String,
    /// Numéro de séquence de la session.
    pub session_sequence: u64,
    /// Statut : `"open"` ou `"closed"`.
    pub status: &'static str,
    /// Timestamp d'ouverture en millisecondes Unix.
    pub opened_at_ms: u64,
    /// Totaux courants (ventes, remboursements, etc.).
    pub totals: SessionTotals,
}

/// Totaux financiers courants de la session.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionTotals {
    /// Total ventes TTC en centimes.
    pub sales_cents: i64,
    /// Total remboursements en centimes (valeur absolue).
    pub refunds_cents: i64,
    /// Total annulations en centimes (valeur absolue).
    pub cancels_cents: i64,
    /// Total remises en centimes (valeur absolue).
    pub discounts_cents: i64,
    /// CA net en centimes.
    pub net_revenue_cents: i64,
    /// Nombre d'entrées fiscales dans la session.
    pub entry_count: u64,
}

/// Réponse de clôture de session (contient le rapport Z).
#[derive(Debug, Serialize, Deserialize)]
pub struct CloseSessionResponse {
    /// Message de confirmation.
    pub message: &'static str,
    /// Résumé du rapport Z généré.
    pub z_report: ZReportSummary,
    /// Texte formaté pour impression sur ticket 80mm.
    pub printable_text: String,
}

/// Résumé du rapport Z pour la réponse JSON.
#[derive(Debug, Serialize, Deserialize)]
pub struct ZReportSummary {
    /// Identifiant du rapport Z.
    pub id: String,
    /// Session clôturée.
    pub session_id: String,
    /// Numéro de séquence de la session.
    pub session_sequence: u64,
    /// Timestamp de génération.
    pub generated_at_ms: u64,
    /// Total ventes TTC en centimes.
    pub total_sales_cents: i64,
    /// Total remboursements en centimes.
    pub total_refunds_cents: i64,
    /// Total annulations en centimes.
    pub total_cancels_cents: i64,
    /// Total remises en centimes.
    pub total_discounts_cents: i64,
    /// CA net en centimes.
    pub net_revenue_cents: i64,
    /// Hash de clôture en hexadécimal (64 caractères).
    pub closing_hash_hex: String,
    /// Plage de séquences couvertes.
    pub first_sequence: u64,
    /// Dernière séquence.
    pub last_sequence: u64,
    /// Nombre d'entrées dans la session.
    pub entry_count: u64,
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `GET /api/v1/sessions/current`
///
/// Retourne l'état de la session active courante.
///
/// # Responses
/// - `200 OK` avec `SessionResponse` si une session est active
/// - `404 Not Found` si aucune session n'est ouverte
pub async fn get_current_session_handler(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<SessionResponse>), ApiErr> {
    let session = state
        .journal
        .active_session()
        .await
        .ok_or(fiscal_engine::errors::FiscalError::Session(
            fiscal_engine::errors::SessionError::NoActiveSession,
        ))?;

    let response = SessionResponse {
        session_id: session.id.to_string(),
        session_sequence: session.sequence_number,
        status: "open",
        opened_at_ms: session.opened_at_ms,
        totals: SessionTotals {
            sales_cents: session.total_sales_cents.0,
            refunds_cents: session.total_refunds_cents.0,
            cancels_cents: session.total_cancels_cents.0,
            discounts_cents: session.total_discounts_cents.0,
            net_revenue_cents: session.net_revenue().0,
            entry_count: session.entry_count,
        },
    };

    Ok((StatusCode::OK, Json(response)))
}

/// `POST /api/v1/sessions/open`
///
/// Ouvre une nouvelle session de caisse.
///
/// # Responses
/// - `201 Created` avec `SessionResponse`
/// - `409 Conflict` si une session est déjà ouverte (`SESSION_ALREADY_ACTIVE`)
pub async fn open_session_handler(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<SessionResponse>), ApiErr> {
    let session = state.journal.open_session().await?;

    let response = SessionResponse {
        session_id: session.id.to_string(),
        session_sequence: session.sequence_number,
        status: "open",
        opened_at_ms: session.opened_at_ms,
        totals: SessionTotals {
            sales_cents: 0,
            refunds_cents: 0,
            cancels_cents: 0,
            discounts_cents: 0,
            net_revenue_cents: 0,
            entry_count: 0,
        },
    };

    Ok((StatusCode::CREATED, Json(response)))
}

/// `POST /api/v1/sessions/close`
///
/// Clôture la session active et génère le rapport Z.
///
/// ## Comportement
/// 1. Enregistre l'entrée `ZClose` dans le journal
/// 2. Ferme la session en base
/// 3. Génère et persiste le rapport Z (NF525 §6)
/// 4. Retourne le rapport Z et son texte formaté pour impression
///
/// # Responses
/// - `200 OK` avec `CloseSessionResponse` (rapport Z inclus)
/// - `409 Conflict` si aucune session n'est active (`NO_ACTIVE_SESSION`)
pub async fn close_session_handler(
    State(state): State<AppState>,
) -> Result<(StatusCode, Json<CloseSessionResponse>), ApiErr> {
    // Récupérer l'ID de la session avant clôture
    let session_id = state
        .journal
        .active_session()
        .await
        .ok_or(fiscal_engine::errors::FiscalError::Session(
            fiscal_engine::errors::SessionError::NoActiveSession,
        ))?
        .id;

    // Clôturer la session (inscrit ZClose, ferme en base)
    state.journal.close_session().await?;

    // Générer le rapport Z
    let pool = state.journal.store().pool_ref().clone();
    let z_report: ZReport = generate_z_report(&pool, session_id).await?;

    let printable_text = format_z_report_text(&z_report);

    let response = CloseSessionResponse {
        message: "Session clôturée avec succès",
        z_report: z_report_to_summary(&z_report),
        printable_text,
    };

    Ok((StatusCode::OK, Json(response)))
}

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

fn z_report_to_summary(r: &ZReport) -> ZReportSummary {
    use fiscal_engine::hex_encode;
    ZReportSummary {
        id: r.id.to_string(),
        session_id: r.session_id.to_string(),
        session_sequence: r.session_sequence_number,
        generated_at_ms: r.generated_at_ms,
        total_sales_cents: r.total_sales_cents.0,
        total_refunds_cents: r.total_refunds_cents.0,
        total_cancels_cents: r.total_cancels_cents.0,
        total_discounts_cents: r.total_discounts_cents.0,
        net_revenue_cents: r.net_revenue().0,
        closing_hash_hex: hex_encode(&r.closing_hash),
        first_sequence: r.first_entry_sequence,
        last_sequence: r.last_entry_sequence,
        entry_count: r.entry_count,
    }
}
