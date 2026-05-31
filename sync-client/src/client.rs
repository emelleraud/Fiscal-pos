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
    serializer::{FiscalEntryPayload, RemoteConfig, ZReportPayload},
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

    /// Constructeur pour les tests — pointe vers un serveur HTTP local (wiremock, sans https_only).
    #[allow(dead_code)]
    pub fn new_for_test(base_url: &str, service_key: &str) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .expect("reqwest client");
        Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            service_key: service_key.to_string(),
        }
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
    // Push des rapports Z
    // -----------------------------------------------------------------------

    /// Pousse un batch de rapports Z vers Supabase.
    ///
    /// Idempotent via `ignore-duplicates`. Doit être appelé **après** `push_sessions`
    /// car la table Supabase a une FK `session_id → sessions.id`.
    ///
    /// # Errors
    /// - `SyncError::Network` sur erreur réseau
    /// - `SyncError::HttpError` si Supabase retourne un code >= 400
    pub async fn push_z_reports(&self, payloads: &[ZReportPayload]) -> Result<u64, SyncError> {
        if payloads.is_empty() {
            return Ok(0);
        }

        let url = format!("{}/rest/v1/z_reports", self.base_url);
        let body = serde_json::to_string(payloads)?;

        debug!(url = %url, count = payloads.len(), "Push batch z_reports");

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
            debug!(inserted = count, "Z-reports poussés avec succès");
            Ok(count)
        } else {
            let body = response.text().await.unwrap_or_default();
            warn!(status = %status, body = %body, "Erreur Supabase sur push_z_reports");
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
    // Pull des promotions
    // -----------------------------------------------------------------------

    /// Récupère les promotions actives pour ce site depuis Supabase.
    ///
    /// Retourne les promotions chain-level et site-level (filtre groupe côté caisse — MVP).
    ///
    /// # Arguments
    /// * `site_id` - Identifiant du restaurant.
    ///
    /// # Errors
    /// - `SyncError::Network` sur erreur réseau ou désérialisation
    pub async fn pull_promotions(
        &self,
        site_id: &str,
    ) -> Result<Vec<serde_json::Value>, SyncError> {
        // site_id is always a UUID — hex chars + hyphens are URL-safe, no encoding needed
        let url = format!(
            "{}/rest/v1/promotions?select=*&active=eq.true&or=(scope.eq.chain,site_id.eq.{})",
            self.base_url, site_id
        );

        debug!(url = %url, site_id = %site_id, "Pull promotions depuis Supabase");

        let resp = self
            .client
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .header("Accept", "application/json")
            .send()
            .await?;

        if !resp.status().is_success() {
            warn!(status = %resp.status(), "pull_promotions HTTP error");
            return Ok(vec![]);
        }

        let promos: Vec<serde_json::Value> = resp.json().await?;
        Ok(promos)
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

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::{Mock, MockServer, ResponseTemplate};
    use wiremock::matchers::{method, path};
    use crate::serializer::SessionPayload;

    fn make_session_payload(id: &str, session_ref: &str) -> SessionPayload {
        SessionPayload {
            id:             id.to_string(),
            site_id:        "SITE-001".to_string(),
            session_ref:    session_ref.to_string(),
            opened_at:      "2024-01-01T10:00:00Z".to_string(),
            closed_at:      None,
            operator_id:    None,
            opening_amount: 0,
            status:         "open".to_string(),
        }
    }

    #[tokio::test]
    async fn push_sessions_all_duplicates_returns_ok_zero() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/sessions"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
            .mount(&server)
            .await;

        let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
        let payloads = vec![
            make_session_payload("uuid-1", "S0001"),
            make_session_payload("uuid-2", "S0002"),
        ];
        let result = client.push_sessions(&payloads).await;
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn push_sessions_one_inserted_returns_ok_one() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/sessions"))
            .respond_with(
                ResponseTemplate::new(201)
                    .set_body_json(serde_json::json!([{"id": "uuid-1"}])),
            )
            .mount(&server)
            .await;

        let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
        let payloads = vec![make_session_payload("uuid-1", "S0001")];
        let result = client.push_sessions(&payloads).await;
        assert_eq!(result.unwrap(), 1);
    }

    #[tokio::test]
    async fn push_sessions_empty_batch_skips_http() {
        let server = MockServer::start().await;
        let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
        let result = client.push_sessions(&[]).await;
        assert_eq!(result.unwrap(), 0);
    }

    #[tokio::test]
    async fn push_sessions_http_error_returns_err() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/rest/v1/sessions"))
            .respond_with(
                ResponseTemplate::new(409)
                    .set_body_string(r#"{"code":"23505","message":"duplicate key"}"#),
            )
            .mount(&server)
            .await;

        let client = SupabaseClient::new_for_test(&server.uri(), "test-key");
        let payloads = vec![make_session_payload("uuid-1", "S0001")];
        let result = client.push_sessions(&payloads).await;
        assert!(matches!(result, Err(SyncError::HttpError { status: 409, .. })));
    }
}
