//! # client
//!
//! Client HTTP pour l'API REST Supabase.
//!
//! ## Protocole Supabase REST
//! Supabase expose une API REST auto-générée (PostgREST).
//! Les opérations utilisées par le sync-client :
//!
//! | Opération | Méthode | Endpoint                        | Headers spéciaux                    |
//! |-----------|---------|----------------------------------|--------------------------------------|
//! | Push batch | POST   | `/rest/v1/fiscal_entries`        | `Prefer: resolution=ignore-duplicates` |
//! | Pull config | GET  | `/rest/v1/site_configs?site_id=eq.{id}&select=*` | — |
//!
//! ## Idempotence
//! `Prefer: resolution=ignore-duplicates` sur le POST garantit qu'un batch
//! déjà reçu est ignoré (pas de doublon). L'UUID v7 de chaque entrée est
//! la clé de déduplication côté PostgreSQL.
//!
//! ## Authentification
//! Toutes les requêtes portent :
//! - `apikey: <SUPABASE_SERVICE_KEY>`
//! - `Authorization: Bearer <SUPABASE_SERVICE_KEY>`
//! - `Content-Type: application/json`

use std::time::Duration;

use reqwest::{Client, StatusCode};
use tracing::{debug, warn};

use crate::{
    config::SyncConfig,
    error::SyncError,
    serializer::{FiscalEntryPayload, RemoteConfig},
};

/// Client HTTP pour l'API Supabase REST.
///
/// Thread-safe et clonable (reqwest::Client utilise un Arc interne).
#[derive(Debug, Clone)]
pub struct SupabaseClient {
    client: Client,
    base_url: String,
    service_key: String,
}

impl SupabaseClient {
    /// Crée un nouveau client Supabase.
    ///
    /// # Errors
    /// `SyncError::FatalConfig` si la construction du client reqwest échoue
    /// (TLS non disponible, etc.).
    pub fn new(config: &SyncConfig) -> Result<Self, SyncError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.http_timeout_secs))
            .user_agent(concat!("pos-fiscal-sync/", env!("CARGO_PKG_VERSION")))
            .https_only(true)
            .build()
            .map_err(|e| SyncError::FatalConfig {
                reason: format!("Impossible de construire le client HTTP : {e}"),
            })?;

        Ok(Self {
            client,
            base_url: config.supabase_url.trim_end_matches('/').to_string(),
            service_key: config.supabase_service_key.clone(),
        })
    }

    // -----------------------------------------------------------------------
    // Push des entrées fiscales
    // -----------------------------------------------------------------------

    /// Pousse un batch d'entrées fiscales vers Supabase.
    ///
    /// Utilise `Prefer: resolution=ignore-duplicates` pour l'idempotence :
    /// si des entrées du batch sont déjà présentes, elles sont ignorées
    /// sans erreur. Seules les nouvelles entrées sont insérées.
    ///
    /// # Arguments
    /// * `payloads` - Batch d'entrées à pousser.
    ///
    /// # Errors
    /// - `SyncError::Network` sur erreur réseau
    /// - `SyncError::HttpError` si Supabase retourne un code >= 400
    ///
    /// # Examples
    /// ```no_run
    /// # use sync_client::{client::SupabaseClient, config::SyncConfig, serializer::FiscalEntryPayload};
    /// # async fn example(client: SupabaseClient, payloads: Vec<FiscalEntryPayload>)
    /// #     -> Result<(), sync_client::error::SyncError> {
    /// let inserted = client.push_entries(&payloads).await?;
    /// println!("{inserted} entrées insérées");
    /// # Ok(())
    /// # }
    /// ```
    pub async fn push_entries(&self, payloads: &[FiscalEntryPayload]) -> Result<u64, SyncError> {
        if payloads.is_empty() {
            return Ok(0);
        }

        let url = format!("{}/rest/v1/fiscal_entries", self.base_url);
        let body = serde_json::to_string(payloads)?;

        debug!(url = %url, count = payloads.len(), "Push batch fiscal_entries");

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Content-Type", "application/json")
            // Idempotence : ignorer les doublons (clé primaire = id UUID)
            .header("Prefer", "resolution=ignore-duplicates,return=representation")
            .body(body)
            .send()
            .await?;

        let status = response.status();

        if status.is_success() {
            // Supabase retourne les lignes insérées avec return=representation
            let inserted_rows: serde_json::Value = response
                .json()
                .await
                .unwrap_or(serde_json::json!([]));
            let count = inserted_rows.as_array().map(|a| a.len() as u64).unwrap_or(0);
            debug!(inserted = count, "Batch poussé avec succès");
            Ok(count)
        } else {
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Erreur Supabase sur push_entries");
            Err(SyncError::HttpError {
                status: status.as_u16(),
                url,
                body,
            })
        }
    }

    //AMV Ajouté le 11/05/26 2209 ******************************
// -----------------------------------------------------------------------
    // Push des sessions
    // -----------------------------------------------------------------------

    /// Pousse un batch de sessions vers Supabase.
    ///
    /// Utilise `Prefer: resolution=ignore-duplicates` — idempotent sur l'UUID.
    /// Appelé **avant** `push_entries` pour satisfaire la FK `session_id` côté cloud.
    ///
    /// # Errors
    /// - `SyncError::Network` sur erreur réseau
    /// - `SyncError::HttpError` si Supabase retourne un code >= 400
    pub async fn push_sessions(
        &self,
        payloads: &[crate::serializer::SessionPayload],
    ) -> Result<u64, SyncError> {
        if payloads.is_empty() {
            return Ok(0);
        }

        let url = format!("{}/rest/v1/sessions", self.base_url);
        let body = serde_json::to_string(payloads)?;

        debug!(url = %url, count = payloads.len(), "Push batch sessions");

        let response = self
            .client
            .post(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Content-Type", "application/json")
            .header("Prefer", "resolution=ignore-duplicates,return=representation")
            .body(body)
            .send()
            .await?;

        let status = response.status();

        if status.is_success() {
            let inserted_rows: serde_json::Value = response
                .json()
                .await
                .unwrap_or(serde_json::json!([]));
            let count = inserted_rows.as_array().map(|a| a.len() as u64).unwrap_or(0);
            debug!(inserted = count, "Sessions poussées avec succès");
            Ok(count)
        } else {
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Erreur Supabase sur push_sessions");
            Err(SyncError::HttpError {
                status: status.as_u16(),
                url,
                body,
            })
        }
    }    

    // -----------------------------------------------------------------------
    // Pull de la configuration
    // -----------------------------------------------------------------------

    /// Récupère la configuration active pour ce site depuis Supabase.
    ///
    /// Interroge la table `site_configs` avec un filtre sur `site_id`.
    /// Retourne `None` si aucune configuration n'est disponible pour ce site.
    ///
    /// # Arguments
    /// * `site_id` - Identifiant du restaurant.
    ///
    /// # Errors
    /// - `SyncError::Network` sur erreur réseau
    /// - `SyncError::HttpError` si Supabase retourne un code >= 400
    pub async fn pull_config(&self, site_id: &str) -> Result<Option<RemoteConfig>, SyncError> {
        let url = format!(
            "{}/rest/v1/site_configs?site_id=eq.{}&select=*&order=version.desc&limit=1",
            self.base_url, site_id
        );

        debug!(url = %url, site_id = %site_id, "Pull config depuis Supabase");

        let response = self
            .client
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Accept", "application/json")
            .send()
            .await?;

        let status = response.status();

        if status == StatusCode::OK {
            let configs: Vec<RemoteConfig> = response.json().await?;
            Ok(configs.into_iter().next())
        } else if status == StatusCode::NOT_FOUND {
            Ok(None)
        } else {
            let body = response.text().await.unwrap_or_default();
            Err(SyncError::HttpError {
                status: status.as_u16(),
                url,
                body,
            })
        }
    }

    // -----------------------------------------------------------------------
    // Healthcheck Supabase
    // -----------------------------------------------------------------------

    /// Vérifie que Supabase est accessible.
    ///
    /// Utilisé au démarrage et avant chaque cycle de sync pour détecter
    /// rapidement le mode offline.
    ///
    /// # Returns
    /// `true` si Supabase répond, `false` si inaccessible (mode offline).
    pub async fn is_reachable(&self) -> bool {
        let url = format!("{}/rest/v1/", self.base_url);
        match self
            .client
            .get(&url)
            .header("apikey", &self.service_key)
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            Ok(r) => r.status() != StatusCode::SERVICE_UNAVAILABLE,
            Err(_) => false,
        }
    }
}
