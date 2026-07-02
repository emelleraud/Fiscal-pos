use serde::{Deserialize, Serialize};

/// Rôle fonctionnel d'une station KDS dans le flux de production.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationRole {
    /// Poste de préparation (cuisson, assemblage partiel).
    Preparation,
    /// Poste de maintien en température.
    Holding,
    /// Poste d'assemblage final du plateau.
    Assembly,
    /// Tableau d'affichage des commandes prêtes (client-facing).
    ReadyBoard,
}

/// Canal de sortie d'une station.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    /// Écran KDS uniquement.
    Screen,
    /// Imprimante ticket uniquement.
    Printer,
    /// Écran et imprimante.
    Both,
}

/// Protocole de connexion de l'imprimante.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterType {
    /// Imprimante réseau TCP/IP.
    Tcpip,
    /// Imprimante USB locale.
    Usb,
    /// Sortie vers fichier (mode test / CI).
    File,
}

/// Format d'impression.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterMode {
    /// Ticket de caisse standard.
    Receipt,
    /// Étiquette sans lignes (mode label).
    LinelessLabel,
}

/// Configuration d'une station KDS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    /// Identifiant unique de la station.
    pub id: String,
    /// Nom affiché de la station.
    pub name: String,
    /// Rôle fonctionnel dans le flux de production.
    pub role: StationRole,
    /// Groupe de température pour le routage (ex: "chaud", "froid").
    pub temperature_group: Option<String>,
    /// Canal de sortie : écran, imprimante ou les deux.
    pub output_type: OutputType,
    /// Adresse réseau ou chemin USB de l'imprimante.
    pub printer_address: Option<String>,
    /// Type de connexion imprimante.
    pub printer_type: Option<PrinterType>,
    /// Mode d'impression.
    pub printer_mode: Option<PrinterMode>,
    /// Largeur du papier en millimètres (58, 80, etc.).
    pub paper_width_mm: Option<i64>,
    /// Station de repli si cette station est hors ligne.
    pub fallback_station_id: Option<String>,
    /// Profils de service dans lesquels cette station est active.
    pub active_in_profiles: Vec<String>,
    /// Ordre d'affichage dans l'interface de configuration.
    pub sort_order: i64,
    /// Indique si la station est opérationnelle.
    pub enabled: bool,
}

/// État d'avancement d'une commande sur une station donnée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationStatus {
    /// Commande reçue, non démarrée.
    New,
    /// Préparation en cours.
    InProgress,
    /// Prête à la station (en attente d'assemblage).
    Ready,
    /// En attente (maintien en température).
    Held,
    /// Assemblée, prête à servir.
    Assembled,
    /// Servie au client.
    Served,
}

impl StationStatus {
    /// Retourne la valeur TEXT pour `SQLite` et les événements SSE.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::InProgress => "in_progress",
            Self::Ready => "ready",
            Self::Held => "held",
            Self::Assembled => "assembled",
            Self::Served => "served",
        }
    }
}

impl std::fmt::Display for StationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_status_as_str() {
        assert_eq!(StationStatus::New.as_str(), "new");
        assert_eq!(StationStatus::InProgress.as_str(), "in_progress");
        assert_eq!(StationStatus::Ready.as_str(), "ready");
        assert_eq!(StationStatus::Held.as_str(), "held");
        assert_eq!(StationStatus::Assembled.as_str(), "assembled");
        assert_eq!(StationStatus::Served.as_str(), "served");
    }

    #[test]
    fn station_status_display() {
        assert_eq!(StationStatus::InProgress.to_string(), "in_progress");
    }

    #[test]
    fn station_status_serialization() {
        let json = serde_json::to_string(&StationStatus::InProgress).unwrap();
        assert_eq!(json, r#""in_progress""#);
    }

    #[test]
    fn station_roundtrip_json() {
        let station = Station {
            id: "s1".to_string(),
            name: "Chaud".to_string(),
            role: StationRole::Preparation,
            temperature_group: Some("hot".to_string()),
            output_type: OutputType::Both,
            printer_address: Some("192.168.1.10".to_string()),
            printer_type: Some(PrinterType::Tcpip),
            printer_mode: Some(PrinterMode::Receipt),
            paper_width_mm: Some(80),
            fallback_station_id: None,
            active_in_profiles: vec!["service_midi".to_string()],
            sort_order: 1,
            enabled: true,
        };
        let json = serde_json::to_string(&station).unwrap();
        let deserialized: Station = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, "s1");
        assert_eq!(deserialized.role, StationRole::Preparation);
        assert_eq!(deserialized.output_type, OutputType::Both);
    }

    #[test]
    fn output_type_serialization() {
        assert_eq!(
            serde_json::to_string(&OutputType::Screen).unwrap(),
            r#""screen""#
        );
        assert_eq!(
            serde_json::to_string(&OutputType::Printer).unwrap(),
            r#""printer""#
        );
        assert_eq!(
            serde_json::to_string(&OutputType::Both).unwrap(),
            r#""both""#
        );
    }
}
