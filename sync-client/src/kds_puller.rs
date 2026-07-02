//! # `kds_puller`
//!
//! Pull la configuration KDS depuis Supabase et upsert dans `SQLite` local.
//!
//! ## Tables cloud → `SQLite`
//! | Cloud (Supabase)         | Local (`SQLite`)          |
//! |--------------------------|-------------------------|
//! | `kds_station_configs`    | `kds_stations`          |
//! | `kds_routing_profiles`   | `kds_routing_profiles`  |
//! | `kds_routing_configs`    | `kds_routing_rules`     |
//! | `kds_channel_triggers`   | `kds_channel_triggers`  |
//! | `kds_timer_thresholds`   | `kds_timer_thresholds`  |
//!
//! La table `kds_active_profile` n'est pas touchée — gérée localement.

use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::{client::SupabaseClient, config::SyncConfig, error::SyncError};

fn str_val(v: &serde_json::Value, key: &str) -> Option<String> {
    v.get(key).and_then(|x| x.as_str()).map(str::to_string)
}

fn str_req(v: &serde_json::Value, key: &str, ctx: &str) -> Option<String> {
    let s = str_val(v, key);
    if s.is_none() {
        warn!(key = %key, context = %ctx, "Champ obligatoire absent — ligne ignorée");
    }
    s
}

fn int_val(v: &serde_json::Value, key: &str) -> Option<i64> {
    v.get(key).and_then(serde_json::Value::as_i64)
}

/// Pull la config KDS depuis Supabase et upsert dans `SQLite` local.
///
/// # Returns
/// Nombre total de lignes traitées (toutes tables confondues).
///
/// # Errors
/// `SyncError::Network` si Supabase est inaccessible.
/// `SyncError::Database` si un upsert `SQLite` échoue.
#[allow(clippy::too_many_lines)]
pub async fn pull_kds_config(
    client: &SupabaseClient,
    config: &SyncConfig,
    pool: &SqlitePool,
) -> Result<usize, SyncError> {
    let cloud = client.pull_kds_config(&config.site_id).await?;
    let mut total = 0usize;

    // --- kds_routing_profiles ---
    for v in &cloud.profiles {
        let Some(id) = str_req(v, "id", "kds_routing_profiles") else {
            continue;
        };
        let Some(name) = str_req(v, "name", "kds_routing_profiles") else {
            continue;
        };
        let desc = str_val(v, "description");

        sqlx::query(
            "INSERT INTO kds_routing_profiles (id, name, description)
             VALUES (?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description",
        )
        .bind(&id)
        .bind(&name)
        .bind(desc.as_deref())
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(
        count = cloud.profiles.len(),
        "kds_routing_profiles upserted"
    );

    // --- kds_stations ---
    for v in &cloud.stations {
        let Some(id) = str_req(v, "id", "kds_station_configs") else {
            continue;
        };
        let Some(name) = str_req(v, "name", "kds_station_configs") else {
            continue;
        };
        let Some(role) = str_req(v, "role", "kds_station_configs") else {
            continue;
        };
        let Some(output_type) = str_req(v, "output_type", "kds_station_configs") else {
            continue;
        };

        let active_in_profiles =
            str_val(v, "active_in_profiles").unwrap_or_else(|| r#"["normal"]"#.to_string());
        let sort_order = int_val(v, "sort_order").unwrap_or(0);
        let enabled = int_val(v, "enabled").unwrap_or(1);

        sqlx::query(
            "INSERT INTO kds_stations
             (id, name, role, temperature_group, output_type, printer_address,
              printer_type, printer_mode, paper_width_mm, fallback_station_id,
              active_in_profiles, sort_order, enabled)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, role=excluded.role,
               temperature_group=excluded.temperature_group,
               output_type=excluded.output_type,
               printer_address=excluded.printer_address,
               printer_type=excluded.printer_type,
               printer_mode=excluded.printer_mode,
               paper_width_mm=excluded.paper_width_mm,
               fallback_station_id=excluded.fallback_station_id,
               active_in_profiles=excluded.active_in_profiles,
               sort_order=excluded.sort_order,
               enabled=excluded.enabled",
        )
        .bind(&id)
        .bind(&name)
        .bind(&role)
        .bind(str_val(v, "temperature_group").as_deref())
        .bind(&output_type)
        .bind(str_val(v, "printer_address").as_deref())
        .bind(str_val(v, "printer_type").as_deref())
        .bind(str_val(v, "printer_mode").as_deref())
        .bind(int_val(v, "paper_width_mm"))
        .bind(str_val(v, "fallback_station_id").as_deref())
        .bind(&active_in_profiles)
        .bind(sort_order)
        .bind(enabled)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.stations.len(), "kds_stations upserted");

    // --- kds_routing_rules ---
    for v in &cloud.rules {
        let Some(id) = str_req(v, "id", "kds_routing_configs") else {
            continue;
        };
        let Some(profile_id) = str_req(v, "profile_id", "kds_routing_configs") else {
            continue;
        };
        let Some(rule_type) = str_req(v, "rule_type", "kds_routing_configs") else {
            continue;
        };
        let Some(match_value) = str_req(v, "match_value", "kds_routing_configs") else {
            continue;
        };
        let Some(station_ids) = str_req(v, "station_ids", "kds_routing_configs") else {
            continue;
        };
        let priority = int_val(v, "priority").unwrap_or(0);

        sqlx::query(
            "INSERT INTO kds_routing_rules (id, profile_id, rule_type, match_value, station_ids, priority)
             VALUES (?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               profile_id=excluded.profile_id, rule_type=excluded.rule_type,
               match_value=excluded.match_value, station_ids=excluded.station_ids,
               priority=excluded.priority",
        )
        .bind(&id)
        .bind(&profile_id)
        .bind(&rule_type)
        .bind(&match_value)
        .bind(&station_ids)
        .bind(priority)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(count = cloud.rules.len(), "kds_routing_rules upserted");

    // --- kds_channel_triggers ---
    for v in &cloud.triggers {
        let Some(channel) = str_req(v, "channel", "kds_channel_triggers") else {
            continue;
        };
        let Some(order_type) = str_req(v, "order_type", "kds_channel_triggers") else {
            continue;
        };
        let Some(trigger_on) = str_req(v, "trigger_on", "kds_channel_triggers") else {
            continue;
        };
        let orb_type = str_val(v, "orb_type");

        sqlx::query(
            "INSERT INTO kds_channel_triggers (channel, order_type, trigger_on, orb_type)
             VALUES (?,?,?,?)
             ON CONFLICT(channel, order_type) DO UPDATE SET
               trigger_on=excluded.trigger_on, orb_type=excluded.orb_type",
        )
        .bind(&channel)
        .bind(&order_type)
        .bind(&trigger_on)
        .bind(orb_type.as_deref())
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(
        count = cloud.triggers.len(),
        "kds_channel_triggers upserted"
    );

    // --- kds_timer_thresholds ---
    for v in &cloud.thresholds {
        let Some(station_id) = str_req(v, "station_id", "kds_timer_thresholds") else {
            continue;
        };
        let warning_secs = int_val(v, "warning_secs").unwrap_or(120);
        let critical_secs = int_val(v, "critical_secs").unwrap_or(300);

        sqlx::query(
            "INSERT INTO kds_timer_thresholds (station_id, warning_secs, critical_secs)
             VALUES (?,?,?)
             ON CONFLICT(station_id) DO UPDATE SET
               warning_secs=excluded.warning_secs, critical_secs=excluded.critical_secs",
        )
        .bind(&station_id)
        .bind(warning_secs)
        .bind(critical_secs)
        .execute(pool)
        .await
        .map_err(SyncError::Database)?;

        total += 1;
    }
    debug!(
        count = cloud.thresholds.len(),
        "kds_timer_thresholds upserted"
    );

    info!(total = total, "Config KDS synchronisée depuis Supabase");
    Ok(total)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn str_req_missing_field_returns_none() {
        let v = json!({ "name": "Grill" });
        assert!(str_req(&v, "id", "test").is_none());
    }

    #[test]
    fn str_req_present_field_returns_some() {
        let v = json!({ "id": "grill-01" });
        assert_eq!(str_req(&v, "id", "test"), Some("grill-01".to_string()));
    }

    #[test]
    fn int_val_returns_correct_value() {
        let v = json!({ "priority": 5 });
        assert_eq!(int_val(&v, "priority"), Some(5));
    }

    #[test]
    fn int_val_missing_returns_none() {
        let v = json!({});
        assert_eq!(int_val(&v, "priority"), None);
    }
}
