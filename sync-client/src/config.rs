//! # config
//!
//! Configuration du `sync-client` chargée depuis les variables d'environnement.
//!
//! ## Variables obligatoires
//! - `DATABASE_URL` — chemin SQLite de la base du restaurant
//! - `SUPABASE_URL` — URL de l'instance Supabase
//! - `SUPABASE_SERVICE_KEY` — clé service_role (jamais la anon key)
//!
//! ## Variables optionnelles (avec défauts)
//! - `SYNC_INTERVAL_SECONDS` — intervalle entre deux cycles (défaut : 30)
//! - `SYNC_BATCH_SIZE` — nombre max d'entrées par push (défaut : 100)
//! - `SYNC_MAX_RETRIES` — tentatives avant quarantaine (défaut : 5)
//! - `SYNC_BACKOFF_INITIAL_MS` — délai initial backoff exponentiel (défaut : 500)
//! - `SYNC_BACKOFF_MAX_SECONDS` — délai maximum de backoff (défaut : 300)
//! - `SITE_ID` — identifiant du restaurant (SIRET ou ID réseau)

use crate::error::SyncError;

/// Configuration complète du sync-client.
#[derive(Debug, Clone)]
pub struct SyncConfig {
    /// URL SQLite locale (ex: `sqlite:./restaurant.db`).
    pub database_url: String,

    /// URL de base Supabase (ex: `https://xxxx.supabase.co`).
    pub supabase_url: String,

    /// Clé service_role Supabase. Ne jamais utiliser la anon key.
    pub supabase_service_key: String,

    /// Identifiant unique du restaurant (SIRET ou ID réseau interne).
    pub site_id: String,

    /// Intervalle entre deux cycles de synchronisation en secondes.
    pub sync_interval_secs: u64,

    /// Nombre maximum d'entrées à pousser par batch HTTP.
    pub batch_size: u32,

    /// Nombre maximum de tentatives avant mise en quarantaine d'un batch.
    pub max_retries: u32,

    /// Délai initial du backoff exponentiel en millisecondes.
    pub backoff_initial_ms: u64,

    /// Délai maximum du backoff exponentiel en secondes.
    pub backoff_max_secs: u64,

    /// Timeout des appels HTTP en secondes.
    pub http_timeout_secs: u64,

    /// Répertoire de données local (pour écrire menu.json reçu du cloud).
    pub data_dir: String,
}

impl SyncConfig {
    /// Charge la configuration depuis les variables d'environnement.
    ///
    /// # Errors
    /// Retourne `SyncError::FatalConfig` si une variable obligatoire est absente
    /// ou si une valeur numérique est invalide.
    pub fn from_env() -> Result<Self, SyncError> {
        let database_url = require_env("DATABASE_URL")?;
        let supabase_url = require_env("SUPABASE_URL")?;
        let supabase_service_key = require_env("SUPABASE_SERVICE_KEY")?;

        // Validation des URLs
        if !supabase_url.starts_with("https://") {
            return Err(SyncError::FatalConfig {
                reason: format!("SUPABASE_URL doit commencer par https://, reçu : {supabase_url}"),
            });
        }

        Ok(Self {
            database_url,
            supabase_url,
            supabase_service_key,
            site_id: std::env::var("SITE_ID")
                .or_else(|_| std::env::var("FISCAL_SITE_ID"))
                .unwrap_or_else(|_| "SITE_INCONNU".to_string()),
            sync_interval_secs: parse_env_u64("SYNC_INTERVAL_SECONDS", 30)?,
            batch_size: parse_env_u32("SYNC_BATCH_SIZE", 100)?,
            max_retries: parse_env_u32("SYNC_MAX_RETRIES", 5)?,
            backoff_initial_ms: parse_env_u64("SYNC_BACKOFF_INITIAL_MS", 500)?,
            backoff_max_secs: parse_env_u64("SYNC_BACKOFF_MAX_SECONDS", 300)?,
            http_timeout_secs: parse_env_u64("SYNC_HTTP_TIMEOUT_SECONDS", 30)?,
            data_dir: std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
        })
    }

    /// Calcule le délai de backoff pour la n-ième tentative (backoff exponentiel + jitter).
    ///
    /// Formule : `min(initial_ms * 2^attempt, max_ms) + jitter(0..1000ms)`
    ///
    /// Le jitter évite les effets de "thundering herd" quand plusieurs
    /// restaurants redémarrent simultanément après une panne réseau.
    ///
    /// # Examples
    /// ```
    /// # use sync_client::config::SyncConfig;
    /// # let config = SyncConfig {
    /// #     database_url: "".to_string(), supabase_url: "https://x.supabase.co".to_string(),
    /// #     supabase_service_key: "".to_string(), site_id: "".to_string(),
    /// #     sync_interval_secs: 30, batch_size: 100, max_retries: 5,
    /// #     backoff_initial_ms: 500, backoff_max_secs: 300, http_timeout_secs: 30,
    /// #     data_dir: "".to_string(),
    /// # };
    /// let delay = config.backoff_duration(0); // Première tentative
    /// assert!(delay.as_millis() >= 500);
    /// let delay3 = config.backoff_duration(3); // 4ème tentative
    /// assert!(delay3.as_millis() >= 4000); // 500 * 2^3 = 4000ms minimum
    /// ```
    #[must_use]
    pub fn backoff_duration(&self, attempt: u32) -> std::time::Duration {
        let max_ms = self.backoff_max_secs * 1_000;
        // 2^attempt avec saturation pour éviter l'overflow
        let factor = 1u64.checked_shl(attempt.min(63)).unwrap_or(u64::MAX);
        let base_ms = (self.backoff_initial_ms.saturating_mul(factor)).min(max_ms);

        // Jitter pseudo-aléatoire (0..1000ms) sans dépendance rand
        let jitter_ms = jitter_ms();

        std::time::Duration::from_millis(base_ms + jitter_ms)
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_env(key: &str) -> Result<String, SyncError> {
    std::env::var(key).map_err(|_| SyncError::FatalConfig {
        reason: format!("Variable d'environnement obligatoire absente : {key}"),
    })
}

fn parse_env_u64(key: &str, default: u64) -> Result<u64, SyncError> {
    match std::env::var(key) {
        Ok(val) => val.parse::<u64>().map_err(|_| SyncError::FatalConfig {
            reason: format!("{key} doit être un entier positif, reçu : '{val}'"),
        }),
        Err(_) => Ok(default),
    }
}

fn parse_env_u32(key: &str, default: u32) -> Result<u32, SyncError> {
    match std::env::var(key) {
        Ok(val) => val.parse::<u32>().map_err(|_| SyncError::FatalConfig {
            reason: format!("{key} doit être un entier positif, reçu : '{val}'"),
        }),
        Err(_) => Ok(default),
    }
}

/// Jitter pseudo-aléatoire (0..1000ms) basé sur le timestamp système.
/// Pas de dépendance `rand` pour rester minimal.
fn jitter_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_millis() as u64
        % 1_000
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_config() -> SyncConfig {
        SyncConfig {
            database_url: "sqlite:./test.db".to_string(),
            supabase_url: "https://test.supabase.co".to_string(),
            supabase_service_key: "key".to_string(),
            site_id: "SITE-001".to_string(),
            sync_interval_secs: 30,
            batch_size: 100,
            max_retries: 5,
            backoff_initial_ms: 500,
            backoff_max_secs: 300,
            http_timeout_secs: 30,
            data_dir: "./data".to_string(),
        }
    }

    #[test]
    fn backoff_attempt_0_equals_initial() {
        let config = base_config();
        let d = config.backoff_duration(0);
        // 500ms * 2^0 = 500ms + jitter (0..1000ms) => entre 500 et 1500ms
        assert!(d.as_millis() >= 500);
        assert!(d.as_millis() < 2_000);
    }

    #[test]
    fn backoff_grows_exponentially() {
        let config = base_config();
        // Tentative 0 : 500ms, Tentative 1 : 1000ms, Tentative 2 : 2000ms
        // (hors jitter, qui est <=1000ms)
        let d0 = config.backoff_duration(0);
        let d1 = config.backoff_duration(1);
        let d2 = config.backoff_duration(2);
        // Avec jitter, d1 > d0 n'est pas garanti à la ms près
        // mais d2 doit être >= 2000ms (hors jitter)
        assert!(d2.as_millis() >= 2_000);
        let _ = (d0, d1); // utilisés pour éviter unused warning
    }

    #[test]
    fn backoff_capped_at_max() {
        let config = base_config(); // max = 300s = 300_000ms
        // À partir de la tentative 10 : 500 * 2^10 = 512_000ms > 300_000ms
        let d = config.backoff_duration(20);
        assert!(d.as_millis() <= 301_000, "Backoff dépasse le maximum + jitter");
    }

    #[test]
    fn backoff_no_overflow_on_large_attempt() {
        let config = base_config();
        // Ne doit pas paniquer avec un grand numéro de tentative
        let _ = config.backoff_duration(100);
    }

    #[test]
    fn sync_error_transient_classification() {
        // Les erreurs réseau sont transitoires
        assert!(SyncError::Timeout {
            seconds: 30,
            operation: "push_batch".to_string()
        }
        .is_transient());
        // Les erreurs 4xx ne sont pas transitoires (données invalides)
        assert!(!SyncError::HttpError {
            status: 400,
            url: "https://x.supabase.co".to_string(),
            body: "Bad request".to_string()
        }
        .is_transient());
        // Les erreurs 5xx sont transitoires
        assert!(SyncError::HttpError {
            status: 503,
            url: "https://x.supabase.co".to_string(),
            body: "Service unavailable".to_string()
        }
        .is_transient());
    }
}
