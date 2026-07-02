use serde::{Deserialize, Serialize};

pub use common::OrderType;

/// ORB (Order Ready Board) cible selon le type de commande.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbType {
    /// Borne client (emporté / click & collect).
    Client,
    /// Borne livreur (livraison à domicile).
    Livreur,
}

/// Extension KDS sur [`OrderType`] : détermine l'ORB cible.
pub trait OrderTypeExt {
    /// ORB cible selon le type de commande, ou `None` si pas d'ORB.
    #[must_use]
    fn orb_type(self) -> Option<OrbType>;
}

impl OrderTypeExt for OrderType {
    fn orb_type(self) -> Option<OrbType> {
        match self {
            Self::Takeaway | Self::ClickAndCollect => Some(OrbType::Client),
            Self::Delivery => Some(OrbType::Livreur),
            Self::EatIn | Self::Drive => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eat_in_has_no_orb() {
        assert_eq!(OrderType::EatIn.orb_type(), None);
    }

    #[test]
    fn drive_has_no_orb() {
        assert_eq!(OrderType::Drive.orb_type(), None);
    }

    #[test]
    fn takeaway_routes_to_client_orb() {
        assert_eq!(OrderType::Takeaway.orb_type(), Some(OrbType::Client));
    }

    #[test]
    fn click_and_collect_routes_to_client_orb() {
        assert_eq!(OrderType::ClickAndCollect.orb_type(), Some(OrbType::Client));
    }

    #[test]
    fn delivery_routes_to_livreur_orb() {
        assert_eq!(OrderType::Delivery.orb_type(), Some(OrbType::Livreur));
    }

    #[test]
    fn orb_type_serialization() {
        let json = serde_json::to_string(&OrbType::Client).unwrap();
        assert_eq!(json, r#""client""#);
        let json = serde_json::to_string(&OrbType::Livreur).unwrap();
        assert_eq!(json, r#""livreur""#);
    }
}
