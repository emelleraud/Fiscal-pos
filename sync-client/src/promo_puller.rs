//! # promo_puller
//!
//! Réception des promotions actives depuis Supabase et upsert dans SQLite local.
//!
//! ## Flux
//! 1. Appeler `GET /rest/v1/promotions` avec filtre `active=true` et scope chain ou site
//! 2. Pour chaque promotion reçue : upsert dans la table `promotions` locale
//!
//! ## Mapping Supabase → SQLite
//! Le champ `trigger` (Supabase) est stocké dans la colonne `trigger_type` (SQLite)
//! pour éviter le mot-clé réservé SQLite `TRIGGER`.

use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{debug, info};

use crate::{client::SupabaseClient, config::SyncConfig, error::SyncError};

/// Représentation d'une promotion reçue depuis Supabase.
#[derive(Debug, Deserialize)]
struct RemotePromo {
    id: String,
    name: String,
    promo_type: String,
    value_cents: Option<i64>,
    value_bps: Option<i64>,
    target_sku: Option<String>,
    trigger: String,
    exclusion_group: Option<String>,
    priority: i32,
    valid_from: Option<String>,
    valid_to: Option<String>,
    days_of_week: Option<serde_json::Value>,
    time_from: Option<String>,
    time_to: Option<String>,
    active: Option<bool>,
    updated_at_ms: i64,
}

/// Tire les promotions actives depuis Supabase et les upsert dans SQLite local.
///
/// # Arguments
/// * `client` - Client HTTP Supabase.
/// * `config` - Configuration de synchronisation (contient le `site_id`).
/// * `pool` - Pool SQLite local.
///
/// # Returns
/// Nombre de promotions traitées.
///
/// # Errors
/// `SyncError::Database` si une insertion SQLite échoue (non fatal — warn + continue en sync_loop).
pub async fn pull_promotions(
    client: &SupabaseClient,
    config: &SyncConfig,
    pool: &SqlitePool,
) -> Result<usize, SyncError> {
    let raw = client.pull_promotions(&config.site_id).await?;
    let count = raw.len();

    for value in raw {
        let p: RemotePromo = match serde_json::from_value(value) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "Promotion ignorée — désérialisation échouée");
                continue;
            }
        };

        let days_json = p.days_of_week.map(|d| d.to_string());
        let active_int: i64 = if p.active.unwrap_or(false) { 1 } else { 0 };

        sqlx::query(
            "INSERT INTO promotions
             (id, name, promo_type, value_cents, value_bps, target_sku, trigger_type,
              exclusion_group, priority, valid_from, valid_to, days_of_week,
              time_from, time_to, active, updated_at_ms)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, promo_type=excluded.promo_type,
               value_cents=excluded.value_cents, value_bps=excluded.value_bps,
               target_sku=excluded.target_sku, trigger_type=excluded.trigger_type,
               exclusion_group=excluded.exclusion_group, priority=excluded.priority,
               valid_from=excluded.valid_from, valid_to=excluded.valid_to,
               days_of_week=excluded.days_of_week, time_from=excluded.time_from,
               time_to=excluded.time_to, active=excluded.active,
               updated_at_ms=excluded.updated_at_ms",
        )
        .bind(&p.id)
        .bind(&p.name)
        .bind(&p.promo_type)
        .bind(p.value_cents)
        .bind(p.value_bps)
        .bind(&p.target_sku)
        .bind(&p.trigger) // Supabase field 'trigger' → SQLite column 'trigger_type'
        .bind(&p.exclusion_group)
        .bind(p.priority)
        .bind(&p.valid_from)
        .bind(&p.valid_to)
        .bind(&days_json)
        .bind(&p.time_from)
        .bind(&p.time_to)
        .bind(active_int)
        .bind(p.updated_at_ms)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;
    }

    if count > 0 {
        info!(count, "Promotions synchronisées depuis Supabase");
    } else {
        debug!("Aucune nouvelle promotion");
    }
    Ok(count)
}
