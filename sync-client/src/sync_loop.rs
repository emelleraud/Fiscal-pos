//! # `sync_loop`
//!
//! Boucle de synchronisation offline-first.
//!
//! ## Cycle de synchronisation
//!
//! ```text
//! loop {
//!     sleep(sync_interval)
//!
//!     if !supabase.is_reachable() {
//!         log "Mode offline — en attente"
//!         continue  ← on attend le prochain cycle, la queue locale grandit
//!     }
//!
//!     // Push des sessions non synchronisées (avant les entrées — FK cloud)
//!     push_sessions_with_retry()
//!
//!     // Push des entrées non synchronisées
//!     loop {
//!         batch = store.load_unsynced(batch_size)
//!         if batch.is_empty() { break }
//!
//!         for attempt in 0..max_retries {
//!             match client.push_entries(batch) {
//!                 Ok(_) => store.mark_synced(batch_ids); break
//!                 Err(transient) => sleep(backoff(attempt)); continue
//!                 Err(fatal) => log + break  ← on abandonne ce batch
//!             }
//!         }
//!     }
//!
//!     // Pull de la configuration
//!     config_puller.pull_and_apply()
//! }
//! ```
//!
//! ## Garanties offline-first
//! - Aucune donnée fiscale n'est perdue en cas de panne réseau :
//!   la queue locale (colonne `synced=0`) grandit indéfiniment jusqu'au retour du réseau
//! - Le journal fiscal n'est **jamais bloqué** par la synchronisation :
//!   les deux processus opèrent sur des connexions `SQLite` indépendantes
//! - La synchronisation est **read-only** sur le journal :
//!   `mark_entries_synced()` est le seul UPDATE autorisé sur `fiscal_entries`
//!
//! ## Métriques
//! Chaque cycle log ses résultats pour le monitoring :
//! - Nombre de sessions poussées
//! - Nombre d'entrées poussées / nombre de batches
//! - Durée du cycle
//! - Mode (online / offline)

use std::time::Instant;

use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::{
    client::SupabaseClient, config::SyncConfig, config_puller::pull_and_apply_config,
    error::SyncError, kds_puller::pull_kds_config, promo_puller::pull_promotions,
};
use fiscal_engine::journal::store::JournalStore;

/// Exécute un cycle complet de synchronisation.
///
/// Un cycle = push sessions + push entrées par batches + pull config.
/// Appelé par la boucle principale toutes les `sync_interval_secs` secondes.
///
/// # Returns
/// Métriques du cycle.
///
/// # Errors
/// Les erreurs transitoires sont gérées en interne (avec retry).
/// Cette fonction retourne une erreur uniquement si la base `SQLite` est inaccessible.
#[allow(clippy::too_many_lines)]
pub async fn run_sync_cycle(
    store: &JournalStore,
    client: &SupabaseClient,
    config: &SyncConfig,
) -> Result<CycleMetrics, SyncError> {
    let cycle_start = Instant::now();
    let mut metrics = CycleMetrics::default();

    // 1. Vérifier la connectivité Supabase
    if !client.is_reachable().await {
        warn!(
            site_id = %config.site_id,
            "Supabase inaccessible — cycle ignoré (mode offline)"
        );
        metrics.was_offline = true;
        return Ok(metrics);
    }

    info!(site_id = %config.site_id, "Début du cycle de synchronisation");

    // 2. Push des sessions non synchronisées (doit précéder les entrées — FK cloud)
    let unsynced_sessions = store.load_unsynced_sessions().await?;

    if !unsynced_sessions.is_empty() {
        let session_ids: Vec<Uuid> = unsynced_sessions.iter().map(|s| s.id.0).collect();

        let session_payloads: Vec<crate::serializer::SessionPayload> = unsynced_sessions
            .iter()
            .map(|s| crate::serializer::SessionPayload::from_session(s, &config.site_id))
            .collect();

        match push_sessions_with_retry(client, &session_payloads, config).await {
            Ok(inserted) => {
                let marked = store.mark_sessions_synced(&session_ids).await?;
                metrics.sessions_pushed += marked;
                info!(
                    inserted = inserted,
                    marked = marked,
                    "Sessions synchronisées avec succès"
                );
            }
            Err(e) => {
                error!(error = %e, "Sessions échouées — cycle interrompu");
                metrics.batches_failed += 1;
                metrics.duration_ms = cycle_start
                    .elapsed()
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                return Ok(metrics);
            }
        }
    }

    // 3. Push des rapports Z non synchronisés (après sessions — FK session_id)
    let unsynced_z_reports = store.load_unsynced_z_reports().await?;

    if !unsynced_z_reports.is_empty() {
        let z_report_ids: Vec<uuid::Uuid> = unsynced_z_reports.iter().map(|r| r.id).collect();

        let z_report_payloads: Vec<crate::serializer::ZReportPayload> = unsynced_z_reports
            .iter()
            .map(|r| crate::serializer::ZReportPayload::from_z_report(r, &config.site_id))
            .collect();

        match push_z_reports_with_retry(client, &z_report_payloads, config).await {
            Ok(inserted) => {
                let marked = store.mark_z_reports_synced(&z_report_ids).await?;
                metrics.z_reports_pushed += marked;
                info!(
                    inserted = inserted,
                    marked = marked,
                    "Rapports Z synchronisés avec succès"
                );
            }
            Err(e) => {
                error!(error = %e, "Rapports Z échoués — cycle interrompu");
                metrics.batches_failed += 1;
                metrics.duration_ms = cycle_start
                    .elapsed()
                    .as_millis()
                    .try_into()
                    .unwrap_or(u64::MAX);
                return Ok(metrics);
            }
        }
    }

    // 5. Push des entrées non synchronisées par batches
    loop {
        let entries = store.load_unsynced_entries(config.batch_size).await?;

        if entries.is_empty() {
            debug!("Aucune entrée à synchroniser");
            break;
        }

        let batch_size = entries.len();
        debug!(batch_size = batch_size, "Batch chargé depuis SQLite");

        let entry_ids: Vec<Uuid> = entries.iter().map(|e| e.id.0).collect();

        let payloads = build_payloads(&entries, &config.site_id);

        let push_result = push_with_retry(client, &payloads, config).await;

        match push_result {
            Ok(inserted) => {
                let marked = store.mark_entries_synced(&entry_ids).await?;
                metrics.entries_pushed += marked;
                metrics.batches_pushed += 1;
                info!(
                    inserted = inserted,
                    marked = marked,
                    batch_size = batch_size,
                    "Batch synchronisé avec succès"
                );
            }
            Err(e) => {
                error!(
                    error = %e,
                    batch_size = batch_size,
                    "Batch échoué après épuisement des tentatives — sera réessayé"
                );
                metrics.batches_failed += 1;
                break;
            }
        }
    }

    // 6. Pull de la configuration (si aucun batch n'a échoué)
    if metrics.batches_failed == 0 {
        match pull_and_apply_config(client, config).await {
            Ok(true) => {
                info!("Configuration mise à jour depuis Supabase");
                metrics.config_updated = true;
            }
            Ok(false) => {
                debug!("Configuration à jour, pas de mise à jour");
            }
            Err(e) => {
                warn!(error = %e, "Impossible de récupérer la configuration — non fatal");
            }
        }
    }

    // 7. Pull des promotions actives depuis Supabase → upsert SQLite local
    // (opération read-only indépendante du succès des pushs)
    if let Err(e) = pull_promotions(client, config, store.pool_ref()).await {
        warn!(error = %e, "Échec du pull des promotions (non fatal)");
    } else {
        debug!("Promotions vérifiées depuis Supabase");
    }

    // 8. Pull de la configuration KDS depuis Supabase → upsert SQLite local
    if let Err(e) = pull_kds_config(client, config, store.pool_ref()).await {
        warn!(error = %e, "Échec du pull de la config KDS (non fatal)");
    } else {
        debug!("Config KDS vérifiée depuis Supabase");
    }

    metrics.duration_ms = cycle_start
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);

    info!(
        sessions_pushed = metrics.sessions_pushed,
        z_reports_pushed = metrics.z_reports_pushed,
        entries_pushed = metrics.entries_pushed,
        batches_pushed = metrics.batches_pushed,
        batches_failed = metrics.batches_failed,
        duration_ms = metrics.duration_ms,
        config_updated = metrics.config_updated,
        "Cycle de synchronisation terminé"
    );

    Ok(metrics)
}

// ---------------------------------------------------------------------------
// Retry helpers
// ---------------------------------------------------------------------------

/// Pousse les sessions vers Supabase avec retry et backoff exponentiel.
async fn push_sessions_with_retry(
    client: &SupabaseClient,
    payloads: &[crate::serializer::SessionPayload],
    config: &SyncConfig,
) -> Result<u64, SyncError> {
    let mut last_err: Option<SyncError> = None;

    for attempt in 0..config.max_retries {
        match client.push_sessions(payloads).await {
            Ok(inserted) => return Ok(inserted),
            Err(e) if e.is_transient() => {
                let backoff = config.backoff_duration(attempt);
                warn!(
                    attempt = attempt + 1,
                    max = config.max_retries,
                    backoff_ms = backoff.as_millis(),
                    error = %e,
                    "Erreur transitoire sessions — retry"
                );
                tokio::time::sleep(backoff).await;
                last_err = Some(e);
            }
            Err(e) => {
                error!(error = %e, "Erreur non-transitoire sessions — abandon");
                return Err(e);
            }
        }
    }

    Err(last_err.unwrap_or(SyncError::Timeout {
        seconds: 0,
        operation: "push_sessions_with_retry".to_string(),
    }))
}

/// Pousse les rapports Z vers Supabase avec retry et backoff exponentiel.
async fn push_z_reports_with_retry(
    client: &SupabaseClient,
    payloads: &[crate::serializer::ZReportPayload],
    config: &SyncConfig,
) -> Result<u64, SyncError> {
    let mut last_err: Option<SyncError> = None;

    for attempt in 0..config.max_retries {
        match client.push_z_reports(payloads).await {
            Ok(inserted) => return Ok(inserted),
            Err(e) if e.is_transient() => {
                let backoff = config.backoff_duration(attempt);
                warn!(
                    attempt = attempt + 1,
                    max = config.max_retries,
                    backoff_ms = backoff.as_millis(),
                    error = %e,
                    "Erreur transitoire z_reports — retry"
                );
                tokio::time::sleep(backoff).await;
                last_err = Some(e);
            }
            Err(e) => {
                error!(error = %e, "Erreur non-transitoire z_reports — abandon");
                return Err(e);
            }
        }
    }

    Err(last_err.unwrap_or(SyncError::Timeout {
        seconds: 0,
        operation: "push_z_reports_with_retry".to_string(),
    }))
}

/// Pousse un batch d'entrées vers Supabase avec retry et backoff exponentiel.
async fn push_with_retry(
    client: &SupabaseClient,
    payloads: &[crate::serializer::FiscalEntryPayload],
    config: &SyncConfig,
) -> Result<u64, SyncError> {
    let mut last_err: Option<SyncError> = None;

    for attempt in 0..config.max_retries {
        match client.push_entries(payloads).await {
            Ok(inserted) => return Ok(inserted),
            Err(e) if e.is_transient() => {
                let backoff = config.backoff_duration(attempt);
                warn!(
                    attempt = attempt + 1,
                    max = config.max_retries,
                    backoff_ms = backoff.as_millis(),
                    error = %e,
                    "Erreur transitoire — nouvelle tentative après backoff"
                );
                tokio::time::sleep(backoff).await;
                last_err = Some(e);
            }
            Err(e) => {
                error!(error = %e, "Erreur non-transitoire — abandon du batch");
                return Err(e);
            }
        }
    }

    Err(last_err.unwrap_or(SyncError::Timeout {
        seconds: 0,
        operation: "push_with_retry".to_string(),
    }))
}

// ---------------------------------------------------------------------------
// Helpers de sérialisation
// ---------------------------------------------------------------------------

fn build_payloads(
    entries: &[fiscal_engine::FiscalEntry],
    site_id: &str,
) -> Vec<crate::serializer::FiscalEntryPayload> {
    use crate::serializer::FiscalEntryPayload;
    entries
        .iter()
        .map(|e| FiscalEntryPayload::from_entry(e, site_id))
        .collect()
}

// ---------------------------------------------------------------------------
// Métriques
// ---------------------------------------------------------------------------

/// Résultats d'un cycle de synchronisation.
#[derive(Debug, Default, Clone)]
pub struct CycleMetrics {
    /// Nombre de sessions poussées vers Supabase.
    pub sessions_pushed: u64,
    /// Nombre de rapports Z poussés vers Supabase.
    pub z_reports_pushed: u64,
    /// Nombre total d'entrées poussées vers Supabase.
    pub entries_pushed: u64,
    /// Nombre de batches poussés avec succès.
    pub batches_pushed: u64,
    /// Nombre de batches ayant échoué après épuisement des tentatives.
    pub batches_failed: u64,
    /// Indique si la configuration a été mise à jour.
    pub config_updated: bool,
    /// Indique si Supabase était inaccessible (mode offline).
    pub was_offline: bool,
    /// Durée totale du cycle en millisecondes.
    pub duration_ms: u64,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SyncConfig;

    #[allow(dead_code)]
    fn base_config() -> SyncConfig {
        SyncConfig {
            database_url: "sqlite::memory:".to_string(),
            supabase_url: "https://test.supabase.co".to_string(),
            supabase_service_key: "key".to_string(),
            site_id: "SITE-001".to_string(),
            sync_interval_secs: 30,
            batch_size: 10,
            max_retries: 3,
            backoff_initial_ms: 10,
            backoff_max_secs: 1,
            http_timeout_secs: 5,
            data_dir: "./test_data".to_string(),
        }
    }

    #[test]
    fn cycle_metrics_default_all_zero() {
        let m = CycleMetrics::default();
        assert_eq!(m.sessions_pushed, 0);
        assert_eq!(m.entries_pushed, 0);
        assert_eq!(m.batches_pushed, 0);
        assert_eq!(m.batches_failed, 0);
        assert!(!m.config_updated);
        assert!(!m.was_offline);
        assert_eq!(m.duration_ms, 0);
    }

    #[test]
    fn build_payloads_empty_entries() {
        let payloads = build_payloads(&[], "SITE-001");
        assert!(payloads.is_empty());
    }

    #[test]
    fn build_payloads_propagates_site_id() {
        use common::GENESIS_HASH;
        use fiscal_engine::{
            hash_engine::build_entry_for_test,
            types::{operation::OperationType, tva::TvaRate},
        };

        let entry = build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            0,
            GENESIS_HASH,
        );
        let payloads = build_payloads(&[entry], "MON-SITE");
        assert_eq!(payloads[0].site_id, "MON-SITE");
    }
}
