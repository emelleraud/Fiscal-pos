//! # error
//!
//! Conversion de `FiscalError` en réponses HTTP Axum.
//!
//! ## Principe
//! Toute erreur du `fiscal-engine` est **bloquante** : elle produit une réponse
//! d'erreur HTTP avec le code approprié. Il n'existe pas de fallback silencieux.
//!
//! ## Mapping FiscalError → HTTP
//!
//! | Variante FiscalError              | Code HTTP |
//! |-----------------------------------|-----------|
//! | `Session(NoActiveSession)`        | 409       |
//! | `Session(SessionAlreadyActive)`   | 409       |
//! | `Session(SessionAlreadyClosed)`   | 409       |
//! | `Session(ZReportAlreadyGenerated)`| 409       |
//! | `SessionClosed`                   | 409       |
//! | `InvalidAmount`                   | 422       |
//! | `InvalidTvaDecomposition`         | 422       |
//! | `Integrity(_)`                    | 500       |
//! | `Hash(_)`                         | 500       |
//! | `Database(_)`                     | 500       |
//! | `Archive(NoDataForYear)`          | 404       |
//! | `Archive(ArchiveAlreadyExists)`   | 409       |
//! | `Archive(_)`                      | 500       |

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use fiscal_engine::errors::{ArchiveError, FiscalError, SessionError};
use common::ApiError;
use tracing::error;

/// Wrapper newtype pour implémenter `IntoResponse` sur `FiscalError`.
///
/// Axum exige que les types retournés par les handlers implémentent `IntoResponse`.
/// On ne peut pas implémenter ce trait directement sur `FiscalError` (crate externe),
/// donc on utilise ce wrapper.
pub struct ApiErr(pub FiscalError);

impl From<FiscalError> for ApiErr {
    fn from(e: FiscalError) -> Self {
        Self(e)
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        let (status, code) = classify_error(&self.0);

        // Log toutes les erreurs 5xx avec le contexte complet
        if status.is_server_error() {
            error!(
                error = %self.0,
                code = %code,
                "Erreur serveur fiscale"
            );
        }

        let body = ApiError::new(code, self.0.to_string());
        (status, Json(body)).into_response()
    }
}

/// Détermine le status HTTP et le code machine à partir du type d'erreur.
fn classify_error(err: &FiscalError) -> (StatusCode, &'static str) {
    match err {
        // --- Erreurs de session (état métier) : 409 Conflict ---
        FiscalError::Session(SessionError::NoActiveSession) => {
            (StatusCode::CONFLICT, "NO_ACTIVE_SESSION")
        }
        FiscalError::Session(SessionError::SessionAlreadyActive { .. }) => {
            (StatusCode::CONFLICT, "SESSION_ALREADY_ACTIVE")
        }
        FiscalError::Session(SessionError::SessionAlreadyClosed { .. }) => {
            (StatusCode::CONFLICT, "SESSION_ALREADY_CLOSED")
        }
        FiscalError::Session(SessionError::ZReportAlreadyGenerated { .. }) => {
            (StatusCode::CONFLICT, "Z_REPORT_ALREADY_GENERATED")
        }
        FiscalError::SessionClosed { .. } => {
            (StatusCode::CONFLICT, "SESSION_CLOSED")
        }

        // --- Erreurs de validation (données client) : 422 Unprocessable Entity ---
        FiscalError::InvalidAmount { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_AMOUNT")
        }
        FiscalError::InvalidTvaDecomposition { .. } => {
            (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_TVA_DECOMPOSITION")
        }

        // --- Erreurs d'archive ---
        FiscalError::Archive(ArchiveError::NoDataForYear { .. }) => {
            (StatusCode::NOT_FOUND, "ARCHIVE_NO_DATA_FOR_YEAR")
        }
        FiscalError::Archive(ArchiveError::ArchiveAlreadyExists { .. }) => {
            (StatusCode::CONFLICT, "ARCHIVE_ALREADY_EXISTS")
        }
        FiscalError::Archive(ArchiveError::InvalidSigningKey { .. }) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "ARCHIVE_INVALID_SIGNING_KEY")
        }
        FiscalError::Archive(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "ARCHIVE_ERROR")
        }

        // --- Erreurs internes (intégrité, hash, base) : 500 ---
        FiscalError::Integrity(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "FISCAL_INTEGRITY_FAILURE")
        }
        FiscalError::Hash(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "FISCAL_HASH_ERROR")
        }
        FiscalError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "DATABASE_ERROR")
        }
    }
}
