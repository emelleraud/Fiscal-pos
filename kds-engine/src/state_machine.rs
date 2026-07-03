use std::collections::HashMap;
use std::time::Instant;

use dashmap::DashMap;
use sqlx::SqlitePool;

use crate::{
    broadcaster::KdsBroadcaster,
    printer::PrintLine,
    routing,
    types::{
        event::{KdsAckPayload, KdsEvent, KdsLine, KdsOrderPayload, LineType, TimerThresholds},
        order_type::OrderType,
        station::{Station, StationRole, StationStatus},
    },
    KdsError,
};

/// Payload d'une commande à router vers le KDS.
pub struct IncomingOrder {
    pub order_id: String,
    pub channel: String,
    pub order_type: OrderType,
    pub customer_name: Option<String>,
    pub external_order_id: Option<String>,
    pub lines: Vec<IncomingLine>,
}

pub struct IncomingLine {
    pub line_id: String,
    pub product_name: String,
    pub quantity: i64,
    pub category: String,
    pub product_id: String,
    pub tags: Vec<String>,
    pub parent_line_id: Option<String>,
    pub line_type: LineType,
    pub comment: Option<String>,
}

fn is_online(heartbeats: &DashMap<String, Instant>, station_id: &str, timeout_secs: u64) -> bool {
    match heartbeats.get(station_id) {
        None => true,
        Some(last_seen) => last_seen.elapsed().as_secs() < timeout_secs,
    }
}

fn resolve_effective_station<'a>(
    primary: &'a Station,
    all_stations: &'a [Station],
    heartbeats: &DashMap<String, Instant>,
    timeout_secs: u64,
) -> Option<&'a Station> {
    if is_online(heartbeats, &primary.id, timeout_secs) {
        return Some(primary);
    }
    let Some(ref fid) = primary.fallback_station_id else {
        tracing::warn!(station_id = %primary.id, "station down, no fallback configured");
        return None;
    };
    match all_stations.iter().find(|s| &s.id == fid) {
        Some(fallback) if is_online(heartbeats, &fallback.id, timeout_secs) => Some(fallback),
        _ => {
            tracing::warn!(
                station_id = %primary.id,
                fallback_id = %fid,
                "station down, fallback also down or not in profile"
            );
            None
        }
    }
}

/// Route une commande vers les stations KDS actives et diffuse les événements SSE.
/// Applique le failover : si une station est hors délai heartbeat, reroute vers `fallback_station_id`.
///
/// # Errors
/// Retourne `KdsError::Database` si une écriture `SQLite` échoue.
pub async fn dispatch_order(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order: &IncomingOrder,
    heartbeats: &DashMap<String, Instant>,
    timeout_secs: u64,
) -> Result<(), KdsError> {
    let profile_id = routing::active_profile_id(pool).await?;
    let rules = routing::routing_rules_for_profile(pool, &profile_id).await?;
    let stations = routing::stations_for_profile(pool, &profile_id).await?;

    let now_ms = now_ms();
    let order_number_short = generate_short_number(pool, now_ms).await?;

    // Phase 1 — résoudre les lignes par station primaire (routing rules)
    let mut station_lines: HashMap<String, Vec<&IncomingLine>> = HashMap::new();
    for line in &order.lines {
        let station_ids =
            routing::resolve_stations(&rules, &line.category, &line.product_id, &line.tags);
        for sid in station_ids {
            station_lines.entry(sid.to_string()).or_default().push(line);
        }
    }

    // Phase 2 — appliquer le failover : accumuler les lignes par station effective
    let mut effective_lines: HashMap<String, Vec<&IncomingLine>> = HashMap::new();
    for station in &stations {
        let Some(lines) = station_lines.get(&station.id) else {
            continue;
        };
        let Some(effective) =
            resolve_effective_station(station, &stations, heartbeats, timeout_secs)
        else {
            continue;
        };
        effective_lines
            .entry(effective.id.clone())
            .or_default()
            .extend(lines.iter().copied());
    }

    // Phase 3 — dispatcher une seule fois par station effective
    for station in &stations {
        let Some(lines) = effective_lines.get(&station.id) else {
            continue;
        };

        insert_kds_order(pool, order, station, &order_number_short, now_ms).await?;
        for line in lines {
            insert_kds_line(pool, order, station, line).await?;
        }

        let thresholds = load_thresholds(pool, &station.id).await?;
        let payload = build_payload(
            order,
            station,
            lines,
            &order_number_short,
            now_ms,
            thresholds,
            &stations,
        );
        broadcaster.send(KdsEvent::OrderNew(payload));

        if matches!(
            station.role,
            StationRole::Preparation | StationRole::Holding
        ) {
            if let Some(ref addr) = station.printer_address {
                tokio::spawn(crate::printer::print_order(
                    station.clone(),
                    addr.clone(),
                    order.order_id.clone(),
                    order_number_short.clone(),
                    lines.iter().map(|l| (*l).into()).collect(),
                ));
            }
        }
    }

    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

async fn generate_short_number(pool: &SqlitePool, now_ms: i64) -> Result<String, KdsError> {
    let today = {
        let secs = now_ms / 1000;
        let dt = time::OffsetDateTime::from_unix_timestamp(secs)
            .unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        format!("{:04}-{:02}-{:02}", dt.year(), dt.month() as u8, dt.day())
    };
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kds_orders WHERE DATE(arrived_at / 1000, 'unixepoch') = ?",
    )
    .bind(&today)
    .fetch_one(pool)
    .await?;

    let letter = char::from(b'A' + u8::try_from((count / 99) % 26).unwrap_or(0));
    let num = (count % 99) + 1;
    Ok(format!("{letter}{num:02}"))
}

async fn insert_kds_order(
    pool: &SqlitePool,
    order: &IncomingOrder,
    station: &Station,
    short: &str,
    now_ms: i64,
) -> Result<(), KdsError> {
    sqlx::query(
        "INSERT OR IGNORE INTO kds_orders
         (order_id, station_id, order_number_short, external_order_id, channel, order_type, customer_name, arrived_at)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&order.order_id)
    .bind(&station.id)
    .bind(short)
    .bind(&order.external_order_id)
    .bind(&order.channel)
    .bind(order.order_type.as_str())
    .bind(&order.customer_name)
    .bind(now_ms)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_kds_line(
    pool: &SqlitePool,
    order: &IncomingOrder,
    station: &Station,
    line: &IncomingLine,
) -> Result<(), KdsError> {
    sqlx::query(
        "INSERT OR IGNORE INTO kds_order_lines
         (order_id, line_id, station_id, product_name, quantity, parent_line_id, line_type, comment)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&order.order_id)
    .bind(&line.line_id)
    .bind(&station.id)
    .bind(&line.product_name)
    .bind(line.quantity)
    .bind(&line.parent_line_id)
    .bind(
        serde_json::to_string(&line.line_type)
            .unwrap_or_default()
            .trim_matches('"')
            .to_string(),
    )
    .bind(&line.comment)
    .execute(pool)
    .await?;
    Ok(())
}

async fn load_thresholds(pool: &SqlitePool, station_id: &str) -> Result<TimerThresholds, KdsError> {
    let row = sqlx::query_as::<_, (i64, i64)>(
        "SELECT warning_secs, critical_secs FROM kds_timer_thresholds WHERE station_id = ?",
    )
    .bind(station_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map_or(
        TimerThresholds {
            warning_secs: 120,
            critical_secs: 300,
        },
        |(warning_secs, critical_secs)| TimerThresholds {
            warning_secs,
            critical_secs,
        },
    ))
}

fn build_payload(
    order: &IncomingOrder,
    station: &Station,
    lines: &[&IncomingLine],
    short: &str,
    now_ms: i64,
    thresholds: TimerThresholds,
    all_stations: &[Station],
) -> KdsOrderPayload {
    let station_statuses = all_stations
        .iter()
        .map(|s| (s.name.clone(), StationStatus::New))
        .collect();

    KdsOrderPayload {
        order_id: order.order_id.clone(),
        station_id: station.id.clone(),
        order_number_short: short.to_string(),
        external_order_id: order.external_order_id.clone(),
        channel: order.channel.clone(),
        order_type: order.order_type,
        customer_name: order.customer_name.clone(),
        stage: "preparation".to_string(),
        lines: lines
            .iter()
            .map(|l| KdsLine {
                line_id: l.line_id.clone(),
                product_name: l.product_name.clone(),
                quantity: l.quantity,
                parent_line_id: l.parent_line_id.clone(),
                line_type: l.line_type.clone(),
                comment: l.comment.clone(),
                acknowledged: false,
            })
            .collect(),
        station_statuses,
        arrived_at: now_ms,
        timer_thresholds: thresholds,
    }
}

impl From<&IncomingLine> for KdsLine {
    fn from(l: &IncomingLine) -> Self {
        Self {
            line_id: l.line_id.clone(),
            product_name: l.product_name.clone(),
            quantity: l.quantity,
            parent_line_id: l.parent_line_id.clone(),
            line_type: l.line_type.clone(),
            comment: l.comment.clone(),
            acknowledged: false,
        }
    }
}

impl From<&IncomingLine> for PrintLine {
    fn from(l: &IncomingLine) -> Self {
        Self {
            product_name: l.product_name.clone(),
            quantity: l.quantity,
            indent: 0,
            comment: l.comment.clone(),
        }
    }
}

/// Acknowledge un bon entier ou une ligne pour une station donnée.
///
/// # Errors
/// Retourne `KdsError::Database` si la mise à jour échoue.
pub async fn acknowledge(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order_id: &str,
    station_id: &str,
    line_id: Option<&str>,
) -> Result<(), KdsError> {
    let now_ms = now_ms();

    if let Some(lid) = line_id {
        sqlx::query(
            "UPDATE kds_order_lines SET acknowledged = 1, acknowledged_at = ? WHERE order_id = ? AND line_id = ? AND station_id = ?"
        )
        .bind(now_ms)
        .bind(order_id)
        .bind(lid)
        .bind(station_id)
        .execute(pool)
        .await?;
    } else {
        sqlx::query(
            "UPDATE kds_order_lines SET acknowledged = 1, acknowledged_at = ? WHERE order_id = ? AND station_id = ?"
        )
        .bind(now_ms)
        .bind(order_id)
        .bind(station_id)
        .execute(pool)
        .await?;

        sqlx::query(
            "UPDATE kds_orders SET status = 'ready', first_bump_at = COALESCE(first_bump_at, ?) WHERE order_id = ? AND station_id = ?"
        )
        .bind(now_ms)
        .bind(order_id)
        .bind(station_id)
        .execute(pool)
        .await?;
    }

    broadcaster.send(KdsEvent::OrderAcked(KdsAckPayload {
        order_id: order_id.to_string(),
        station_id: station_id.to_string(),
        line_id: line_id.map(str::to_string),
    }));

    Ok(())
}

/// Marque la commande comme servie (2e bump expo).
///
/// # Errors
/// Retourne `KdsError::Database` si la mise à jour échoue.
pub async fn mark_served(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order_id: &str,
    station_id: &str,
) -> Result<(), KdsError> {
    let now_ms = now_ms();
    sqlx::query(
        "UPDATE kds_orders SET status = 'served', served_at = ? WHERE order_id = ? AND station_id = ?"
    )
    .bind(now_ms)
    .bind(order_id)
    .bind(station_id)
    .execute(pool)
    .await?;

    broadcaster.send(KdsEvent::OrderUpdated(
        crate::types::event::KdsOrderUpdate {
            order_id: order_id.to_string(),
            status: "served".to_string(),
            stage: "served".to_string(),
            station_statuses: HashMap::new(),
        },
    ));

    Ok(())
}

#[cfg(test)]
mod failover_tests {
    use super::{is_online, resolve_effective_station};
    use crate::types::station::{OutputType, Station, StationRole};
    use dashmap::DashMap;
    use std::time::{Duration, Instant};

    fn station(id: &str, fallback: Option<&str>) -> Station {
        Station {
            id: id.to_string(),
            name: id.to_string(),
            role: StationRole::Preparation,
            temperature_group: None,
            output_type: OutputType::Screen,
            printer_address: None,
            printer_type: None,
            printer_mode: None,
            paper_width_mm: None,
            fallback_station_id: fallback.map(str::to_string),
            active_in_profiles: vec!["normal".to_string()],
            sort_order: 1,
            enabled: true,
        }
    }

    #[test]
    fn is_online_absent_means_online() {
        let hb: DashMap<String, Instant> = DashMap::new();
        assert!(is_online(&hb, "grill", 30));
    }

    #[test]
    fn is_online_recent_heartbeat() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now());
        assert!(is_online(&hb, "grill", 30));
    }

    #[test]
    fn is_online_respects_timeout() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert(
            "grill".to_string(),
            Instant::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        );
        assert!(!is_online(&hb, "grill", 30));
    }

    #[test]
    fn resolve_returns_primary_when_online() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now());
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone(), station("cold", None)];
        let result = resolve_effective_station(&primary, &all, &hb, 30);
        assert_eq!(result.map(|s| s.id.as_str()), Some("grill"));
    }

    #[test]
    fn resolve_returns_fallback_when_primary_down() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert(
            "grill".to_string(),
            Instant::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        );
        hb.insert("cold".to_string(), Instant::now());
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone(), station("cold", None)];
        let result = resolve_effective_station(&primary, &all, &hb, 30);
        assert_eq!(result.map(|s| s.id.as_str()), Some("cold"));
    }

    #[test]
    fn resolve_returns_none_when_no_fallback_configured() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert(
            "grill".to_string(),
            Instant::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        );
        let primary = station("grill", None);
        let all = vec![primary.clone()];
        assert!(resolve_effective_station(&primary, &all, &hb, 30).is_none());
    }

    #[test]
    fn resolve_returns_none_when_fallback_also_down() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert(
            "grill".to_string(),
            Instant::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        );
        hb.insert(
            "cold".to_string(),
            Instant::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        );
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone(), station("cold", None)];
        assert!(resolve_effective_station(&primary, &all, &hb, 30).is_none());
    }

    #[test]
    fn resolve_returns_none_when_fallback_not_in_profile() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert(
            "grill".to_string(),
            Instant::now()
                .checked_sub(Duration::from_secs(31))
                .unwrap(),
        );
        // "cold" n'est pas dans all_stations (non dans le profil actif)
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone()];
        assert!(resolve_effective_station(&primary, &all, &hb, 30).is_none());
    }
}
