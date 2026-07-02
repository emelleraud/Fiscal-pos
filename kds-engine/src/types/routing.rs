use serde::{Deserialize, Serialize};

/// Type de critère de routage KDS.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    /// Routage par catégorie de produit (ex: "Burgers").
    Category,
    /// Routage par identifiant produit exact.
    Product,
    /// Routage par tag produit (ex: "friture", "chaud").
    Tag,
}

/// Règle de routage : associe un critère à une ou plusieurs stations KDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    /// Identifiant unique de la règle.
    pub id: String,
    /// Profil de service auquel cette règle appartient.
    pub profile_id: String,
    /// Type de critère de correspondance.
    pub rule_type: RuleType,
    /// Valeur à faire correspondre (nom de catégorie, ID produit, tag).
    pub match_value: String,
    /// Stations KDS destinataires quand la règle s'applique.
    pub station_ids: Vec<String>,
    /// Priorité d'évaluation (plus bas = évalué en premier).
    pub priority: i64,
}

/// Profil de service regroupant un ensemble de règles de routage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingProfile {
    /// Identifiant unique du profil.
    pub id: String,
    /// Nom affiché du profil (ex: "Service midi", "Service soir").
    pub name: String,
    /// Description optionnelle du profil.
    pub description: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_type_serialization() {
        assert_eq!(
            serde_json::to_string(&RuleType::Category).unwrap(),
            r#""category""#
        );
        assert_eq!(
            serde_json::to_string(&RuleType::Product).unwrap(),
            r#""product""#
        );
        assert_eq!(serde_json::to_string(&RuleType::Tag).unwrap(), r#""tag""#);
    }

    #[test]
    fn routing_rule_roundtrip() {
        let rule = RoutingRule {
            id: "rule-1".to_string(),
            profile_id: "profile-midi".to_string(),
            rule_type: RuleType::Category,
            match_value: "Burgers".to_string(),
            station_ids: vec!["chaud".to_string(), "assembly".to_string()],
            priority: 10,
        };
        let json = serde_json::to_string(&rule).unwrap();
        let deserialized: RoutingRule = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "rule-1");
        assert_eq!(deserialized.rule_type, RuleType::Category);
        assert_eq!(deserialized.station_ids.len(), 2);
    }

    #[test]
    fn routing_profile_optional_description() {
        let profile = RoutingProfile {
            id: "p1".to_string(),
            name: "Service midi".to_string(),
            description: None,
        };
        let json = serde_json::to_value(&profile).unwrap();
        assert!(json.get("description").is_none() || json["description"].is_null());
    }

    #[test]
    fn routing_rule_priority_ordering() {
        let mut rules = vec![
            RoutingRule {
                id: "r2".to_string(),
                profile_id: "p".to_string(),
                rule_type: RuleType::Tag,
                match_value: "froid".to_string(),
                station_ids: vec![],
                priority: 20,
            },
            RoutingRule {
                id: "r1".to_string(),
                profile_id: "p".to_string(),
                rule_type: RuleType::Category,
                match_value: "Boissons".to_string(),
                station_ids: vec![],
                priority: 5,
            },
        ];
        rules.sort_by_key(|r| r.priority);
        assert_eq!(rules[0].id, "r1");
        assert_eq!(rules[1].id, "r2");
    }
}
