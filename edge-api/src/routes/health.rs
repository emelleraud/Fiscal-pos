//! # `routes::health`
//!
//! Route de healthcheck pour le monitoring.
//!
//! ## GET /api/v1/health
//!
//! Vérifie que le serveur est opérationnel et que `SQLite` est accessible.
//! Utilisé par les outils de monitoring (Uptime Kuma, nagios, etc.)
//! et par le `sync-client` avant de tenter une synchronisation.
//!
//! ## Niveaux de santé
//! - `ok` : serveur et base accessibles
//! - `degraded` : serveur actif mais base non accessible (cas de corruption)

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::app::AppState;

/// Réponse du healthcheck.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    /// Statut global : `"ok"` ou `"degraded"`.
    pub status: &'static str,
    /// Version du logiciel.
    pub version: &'static str,
    /// Statut de la base `SQLite`.
    pub database: &'static str,
    /// Session active en cours, si présente.
    pub active_session: Option<ActiveSessionInfo>,
}

/// Informations résumées sur la session active.
#[derive(Debug, Serialize, Deserialize)]
pub struct ActiveSessionInfo {
    /// Identifiant de la session.
    pub session_id: String,
    /// Numéro de séquence de la session.
    pub session_sequence: u64,
    /// Nombre d'entrées dans la session.
    pub entry_count: u64,
}

/// `GET /api/v1/health`
///
/// Retourne l'état de santé du serveur edge.
///
/// # Responses
/// - `200 OK` avec `status: "ok"` si tout est opérationnel
/// - `503 Service Unavailable` avec `status: "degraded"` si la base est inaccessible
pub async fn health_handler(State(state): State<AppState>) -> (StatusCode, Json<HealthResponse>) {
    // Vérifier la connectivité SQLite avec une requête légère
    let db_ok = sqlx::query("SELECT 1")
        .fetch_one(state.journal.store().pool_ref())
        .await
        .is_ok();

    let active_session = state
        .journal
        .active_session()
        .await
        .map(|s| ActiveSessionInfo {
            session_id: s.id.to_string(),
            session_sequence: s.sequence_number,
            entry_count: s.entry_count,
        });

    if db_ok {
        (
            StatusCode::OK,
            Json(HealthResponse {
                status: "ok",
                version: env!("CARGO_PKG_VERSION"),
                database: "connected",
                active_session,
            }),
        )
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthResponse {
                status: "degraded",
                version: env!("CARGO_PKG_VERSION"),
                database: "error",
                active_session: None,
            }),
        )
    }
}
