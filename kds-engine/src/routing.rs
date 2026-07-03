use sqlx::SqlitePool;

use crate::{
    types::{routing::RoutingRule, station::Station},
    KdsError,
};

/// Charge le profil actif depuis `SQLite`.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn active_profile_id(pool: &SqlitePool) -> Result<String, KdsError> {
    sqlx::query_scalar::<_, String>("SELECT profile_id FROM kds_active_profile WHERE singleton = 1")
        .fetch_optional(pool)
        .await?
        .ok_or(KdsError::NoActiveProfile)
}

/// Met à jour le profil actif.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn set_active_profile(pool: &SqlitePool, profile_id: &str) -> Result<(), KdsError> {
    sqlx::query("UPDATE kds_active_profile SET profile_id = ? WHERE singleton = 1")
        .bind(profile_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Charge toutes les stations activées pour le profil donné.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn stations_for_profile(
    pool: &SqlitePool,
    profile_id: &str,
) -> Result<Vec<Station>, KdsError> {
    let rows = sqlx::query_as::<_, StationRow>(
        r"SELECT id, name, role, temperature_group, output_type,
                  printer_address, printer_type, printer_mode, paper_width_mm,
                  fallback_station_id, active_in_profiles, sort_order, enabled
           FROM kds_stations
           WHERE enabled = 1
           ORDER BY sort_order",
    )
    .fetch_all(pool)
    .await?;

    let stations = rows
        .into_iter()
        .filter_map(|r| {
            let profiles: Vec<String> = serde_json::from_str(&r.active_in_profiles).ok()?;
            if !profiles.contains(&profile_id.to_string()) {
                return None;
            }
            Some(Station {
                id: r.id,
                name: r.name,
                role: serde_json::from_str(&format!("\"{}\"", r.role)).ok()?,
                temperature_group: r.temperature_group,
                output_type: serde_json::from_str(&format!("\"{}\"", r.output_type)).ok()?,
                printer_address: r.printer_address,
                printer_type: r
                    .printer_type
                    .and_then(|t| serde_json::from_str(&format!("\"{t}\"")).ok()),
                printer_mode: r
                    .printer_mode
                    .and_then(|m| serde_json::from_str(&format!("\"{m}\"")).ok()),
                paper_width_mm: r.paper_width_mm,
                fallback_station_id: r.fallback_station_id,
                active_in_profiles: profiles,
                sort_order: r.sort_order,
                enabled: r.enabled != 0,
            })
        })
        .collect();

    Ok(stations)
}

/// Charge les règles de routage pour le profil actif.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn routing_rules_for_profile(
    pool: &SqlitePool,
    profile_id: &str,
) -> Result<Vec<RoutingRule>, KdsError> {
    let rows = sqlx::query_as::<_, RoutingRuleRow>(
        "SELECT id, profile_id, rule_type, match_value, station_ids, priority
         FROM kds_routing_rules WHERE profile_id = ? ORDER BY priority DESC",
    )
    .bind(profile_id)
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(RoutingRule {
                id: r.id,
                profile_id: r.profile_id,
                rule_type: serde_json::from_str(&format!("\"{}\"", r.rule_type))?,
                match_value: r.match_value,
                station_ids: serde_json::from_str(&r.station_ids)?,
                priority: r.priority,
            })
        })
        .collect()
}

/// Détermine les `station_ids` cibles pour un article donné (catégorie + `product_id` + tags).
/// Applique la règle de plus haute priorité qui matche.
/// Retourne une liste vide si aucune règle ne matche.
#[must_use]
pub fn resolve_stations<'a>(
    rules: &'a [RoutingRule],
    category: &str,
    product_id: &str,
    tags: &[String],
) -> Vec<&'a str> {
    let mut best: Option<&RoutingRule> = None;

    for rule in rules {
        let matches = match rule.rule_type {
            crate::types::routing::RuleType::Category => rule.match_value == category,
            crate::types::routing::RuleType::Product => rule.match_value == product_id,
            crate::types::routing::RuleType::Tag => tags.contains(&rule.match_value),
        };
        if matches && best.is_none_or(|b: &RoutingRule| rule.priority > b.priority) {
            best = Some(rule);
        }
    }

    best.map_or_else(Vec::new, |r| {
        r.station_ids.iter().map(String::as_str).collect()
    })
}

// ---------------------------------------------------------------------------
// Private row types for sqlx::query_as (no compile-time DB verification needed)
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct StationRow {
    id: String,
    name: String,
    role: String,
    temperature_group: Option<String>,
    output_type: String,
    printer_address: Option<String>,
    printer_type: Option<String>,
    printer_mode: Option<String>,
    paper_width_mm: Option<i64>,
    fallback_station_id: Option<String>,
    active_in_profiles: String,
    sort_order: i64,
    enabled: i64,
}

#[derive(sqlx::FromRow)]
struct RoutingRuleRow {
    id: String,
    profile_id: String,
    rule_type: String,
    match_value: String,
    station_ids: String,
    priority: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::routing::{RoutingRule, RuleType};

    fn rule(
        rule_type: RuleType,
        match_value: &str,
        station_ids: &[&str],
        priority: i64,
    ) -> RoutingRule {
        RoutingRule {
            id: uuid::Uuid::now_v7().to_string(),
            profile_id: "normal".to_string(),
            rule_type,
            match_value: match_value.to_string(),
            station_ids: station_ids
                .iter()
                .map(std::string::ToString::to_string)
                .collect(),
            priority,
        }
    }

    #[test]
    fn product_override_wins_over_category() {
        let rules = vec![
            rule(RuleType::Category, "Burgers", &["grill"], 0),
            rule(RuleType::Product, "burger-vegan", &["cold-station"], 10),
        ];
        let result = resolve_stations(&rules, "Burgers", "burger-vegan", &[]);
        assert_eq!(result, vec!["cold-station"]);
    }

    #[test]
    fn category_fallback_when_no_product_rule() {
        let rules = vec![rule(RuleType::Category, "Boissons", &["drinks"], 0)];
        let result = resolve_stations(&rules, "Boissons", "coca-001", &[]);
        assert_eq!(result, vec!["drinks"]);
    }

    #[test]
    fn empty_when_no_match() {
        let rules = vec![rule(RuleType::Category, "Burgers", &["grill"], 0)];
        let result = resolve_stations(&rules, "Desserts", "brownie-001", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn tag_matches() {
        let rules = vec![rule(RuleType::Tag, "froid", &["cold-station"], 5)];
        let tags = vec!["froid".to_string()];
        let result = resolve_stations(&rules, "Salades", "salade-001", &tags);
        assert_eq!(result, vec!["cold-station"]);
    }
}
