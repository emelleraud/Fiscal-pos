//! # error
//!
//! Hiérarchie d'erreurs du `sync-client`.
//!
//! ## Philosophie
//! Les erreurs de synchronisation ne sont **jamais fatales** pour le restaurant :
//! une erreur réseau ne bloque pas la prise de commande. Le sync-client les log,
//! applique le backoff exponentiel, et réessaie silencieusement.
//!
//! La seule erreur fatale est `SyncError::FatalConfig` (configuration invalide
//! au démarrage) — dans ce cas le process s'arrête avec un log explicite.

use thiserror::Error;

/// Erreur du sync-client.
#[derive(Debug, Error)]
pub enum SyncError {
    /// Erreur réseau ou HTTP lors de l'appel à Supabase.
    #[error("Erreur réseau Supabase : {0}")]
    Network(#[from] reqwest::Error),

    /// Supabase a retourné un code d'erreur HTTP (4xx / 5xx).
    #[error("Supabase HTTP {status} sur {url} : {body}")]
    HttpError {
        /// Code HTTP reçu.
        status: u16,
        /// URL appelée.
        url: String,
        /// Corps de la réponse d'erreur.
        body: String,
    },

    /// Erreur d'accès à la base `SQLite` locale.
    #[error("Erreur SQLite locale : {0}")]
    Database(#[from] sqlx::Error),

    /// Erreur du moteur fiscal (ne devrait pas arriver en sync read-only).
    #[error("Erreur fiscal-engine : {0}")]
    Fiscal(#[from] fiscal_engine::errors::FiscalError),

    /// Erreur de sérialisation JSON.
    #[error("Erreur de sérialisation : {0}")]
    Serialization(#[from] serde_json::Error),

    /// Configuration invalide au démarrage (fatale).
    #[error("Configuration invalide : {reason}")]
    FatalConfig {
        /// Description du problème de configuration.
        reason: String,
    },

    /// Timeout dépassé sur un appel réseau.
    #[error("Timeout dépassé après {seconds}s sur {operation}")]
    Timeout {
        /// Durée du timeout en secondes.
        seconds: u64,
        /// Opération concernée.
        operation: String,
    },
}

impl SyncError {
    /// Indique si l'erreur est transitoire (retry possible) ou fatale (arrêt).
    ///
    /// Les erreurs réseau et HTTP 5xx sont transitoires.
    /// Les erreurs de configuration sont fatales.
    #[must_use]
    pub fn is_transient(&self) -> bool {
        match self {
            Self::Network(_) | Self::Timeout { .. } => true,
            Self::HttpError { status, .. } => *status >= 500,
            Self::Database(_) | Self::Fiscal(_) | Self::Serialization(_) | Self::FatalConfig { .. } => false,
        }
    }
}
