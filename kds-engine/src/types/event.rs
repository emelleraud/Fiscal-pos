use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{order_type::OrderType, station::StationStatus};

/// Type d'une ligne de commande KDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    /// Article vendu directement.
    Item,
    /// Composant d'un combo (menu).
    ComboComponent,
    /// Modificateur (supplément, retrait, option).
    Modifier,
    /// Commentaire libre (allergie, cuisson, etc.).
    Comment,
}

/// Ligne de commande affichée sur l'écran KDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsLine {
    /// Identifiant unique de la ligne.
    pub line_id: String,
    /// Nom du produit à afficher.
    pub product_name: String,
    /// Quantité commandée.
    pub quantity: i64,
    /// Ligne parente (pour les composants et modificateurs).
    pub parent_line_id: Option<String>,
    /// Type de la ligne.
    pub line_type: LineType,
    /// Commentaire libre associé à la ligne.
    pub comment: Option<String>,
    /// Indique si la ligne a été acquittée par un opérateur.
    pub acknowledged: bool,
}

/// Seuils de minuterie pour les alertes visuelles KDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerThresholds {
    /// Délai en secondes avant passage en mode "avertissement" (jaune).
    pub warning_secs: i64,
    /// Délai en secondes avant passage en mode "critique" (rouge).
    pub critical_secs: i64,
}

/// Payload complet d'une nouvelle commande KDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsOrderPayload {
    /// Identifiant unique de la commande.
    pub order_id: String,
    /// Station KDS destinataire.
    pub station_id: String,
    /// Numéro de commande court (affiché sur le ticket et l'ORB).
    pub order_number_short: String,
    /// Identifiant externe (commande tiers : Uber Eats, Deliveroo, etc.).
    pub external_order_id: Option<String>,
    /// Canal de vente (caisse, app, borne, livraison, etc.).
    pub channel: String,
    /// Type de commande — détermine l'ORB cible.
    pub order_type: OrderType,
    /// Nom du client (pour les commandes à emporter et click & collect).
    pub customer_name: Option<String>,
    /// Étape du flux de production (ex: "cooking", "assembly").
    pub stage: String,
    /// Lignes de la commande à préparer sur cette station.
    pub lines: Vec<KdsLine>,
    /// État de la commande sur chaque station impliquée (clé = `station_id`).
    pub station_statuses: HashMap<String, StationStatus>,
    /// Timestamp d'arrivée de la commande (Unix epoch secondes).
    pub arrived_at: i64,
    /// Seuils de minuterie pour les alertes visuelles.
    pub timer_thresholds: TimerThresholds,
}

/// Mise à jour d'état d'une commande existante.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsOrderUpdate {
    /// Identifiant de la commande mise à jour.
    pub order_id: String,
    /// Nouveau statut global de la commande.
    pub status: String,
    /// Nouvelle étape du flux de production.
    pub stage: String,
    /// État mis à jour sur chaque station (clé = `station_id`).
    pub station_statuses: HashMap<String, StationStatus>,
}

/// Payload d'acquittement d'une commande ou d'une ligne.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsAckPayload {
    /// Identifiant de la commande acquittée.
    pub order_id: String,
    /// Station KDS qui émet l'acquittement.
    pub station_id: String,
    /// Ligne acquittée, ou `None` pour acquitter toute la commande.
    pub line_id: Option<String>,
}

/// Événement diffusé via broadcast à tous les handlers SSE.
///
/// Le `station_id` permet au handler SSE de filtrer les events qui le concernent.
/// Le tag `event` + content `data` suit la convention SSE JSON pour le frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum KdsEvent {
    /// Nouvelle commande à afficher sur une station.
    OrderNew(KdsOrderPayload),
    /// Mise à jour d'état d'une commande existante (broadcast toutes stations).
    OrderUpdated(KdsOrderUpdate),
    /// Acquittement d'une commande ou d'une ligne.
    OrderAcked(KdsAckPayload),
}

impl KdsEvent {
    /// Retourne la station concernée par l'événement.
    ///
    /// Pour [`KdsEvent::OrderUpdated`], retourne une chaîne vide car l'événement
    /// est diffusé à toutes les stations.
    #[must_use]
    pub fn station_id(&self) -> &str {
        match self {
            Self::OrderNew(p) => &p.station_id,
            Self::OrderUpdated(_) => "", // broadcast à toutes les stations
            Self::OrderAcked(p) => &p.station_id,
        }
    }

    /// Retourne le type d'événement sous forme de chaîne statique.
    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::OrderNew(_) => "order_new",
            Self::OrderUpdated(_) => "order_updated",
            Self::OrderAcked(_) => "order_acked",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::OrderType;

    fn make_payload(station_id: &str) -> KdsOrderPayload {
        KdsOrderPayload {
            order_id: "ord-001".to_string(),
            station_id: station_id.to_string(),
            order_number_short: "42".to_string(),
            external_order_id: None,
            channel: "caisse".to_string(),
            order_type: OrderType::EatIn,
            customer_name: None,
            stage: "cooking".to_string(),
            lines: vec![],
            station_statuses: HashMap::new(),
            arrived_at: 1_700_000_000,
            timer_thresholds: TimerThresholds {
                warning_secs: 120,
                critical_secs: 240,
            },
        }
    }

    #[test]
    fn event_new_station_id() {
        let event = KdsEvent::OrderNew(make_payload("chaud"));
        assert_eq!(event.station_id(), "chaud");
        assert_eq!(event.event_type(), "order_new");
    }

    #[test]
    fn event_updated_broadcasts_to_all() {
        let update = KdsOrderUpdate {
            order_id: "ord-001".to_string(),
            status: "ready".to_string(),
            stage: "assembly".to_string(),
            station_statuses: HashMap::new(),
        };
        let event = KdsEvent::OrderUpdated(update);
        assert_eq!(event.station_id(), "");
        assert_eq!(event.event_type(), "order_updated");
    }

    #[test]
    fn event_acked_station_id() {
        let ack = KdsAckPayload {
            order_id: "ord-001".to_string(),
            station_id: "froid".to_string(),
            line_id: Some("line-1".to_string()),
        };
        let event = KdsEvent::OrderAcked(ack);
        assert_eq!(event.station_id(), "froid");
        assert_eq!(event.event_type(), "order_acked");
    }

    #[test]
    fn kds_event_serialization_tagged() {
        let event = KdsEvent::OrderAcked(KdsAckPayload {
            order_id: "ord-001".to_string(),
            station_id: "chaud".to_string(),
            line_id: None,
        });
        let json = serde_json::to_value(&event).unwrap();
        assert_eq!(json["event"], "order_acked");
        assert_eq!(json["data"]["order_id"], "ord-001");
    }

    #[test]
    fn line_type_serialization() {
        assert_eq!(
            serde_json::to_string(&LineType::ComboComponent).unwrap(),
            r#""combo_component""#
        );
    }
}
