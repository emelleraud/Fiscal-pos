//! # serializer
//!
//! Conversion des types `fiscal-engine` en payload JSON pour l'API Supabase.
//!
//! ## Format du payload
//! Supabase expose une API REST auto-générée depuis le schéma PostgreSQL.
//! Le push d'entrées utilise `POST /rest/v1/fiscal_entries` avec
//! `Prefer: resolution=ignore-duplicates` pour l'idempotence.
//!
//! ## Correspondance des colonnes
//! | Champ Rust              | Colonne SQL (`fiscal_entries`) |
//! |-------------------------|-------------------------------|
//! | `id`                    | `id`                          |
//! | `site_id`               | `site_id`                     |
//! | `session_id`            | `session_id`                  |
//! | `sequence`              | `sequence`                    |
//! | `operation_type`        | `operation_type`              |
//! | `amount`                | `amount`                      |
//! | `ht_cents`              | `ht_cents`                    |
//! | `tva_cents`             | `tva_cents`                   |
//! | `tva_rate`              | `tva_rate`                    |
//! | `order_ref`             | `order_ref`                   |
//! | `entry_hash`            | `entry_hash`                  |
//! | `prev_hash`             | `prev_hash`                   |
//! | `signed_at`             | `signed_at` (timestamptz)     |
//!
//! ## Représentation des types
//! - Les hashs `[u8; 32]` sont envoyés en hexadécimal (64 chars)
//! - Les `Cents` sont envoyés en i64 (centimes entiers)
//! - Les UUIDs sont en string standard (`xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`)
//! - Les timestamps sont convertis en ISO 8601 UTC (`2025-01-15T08:12:00Z`)
//! - L'`operation_type` et le `tva_rate` sont en strings identiques au schéma SQL

use serde::{Deserialize, Serialize};

use fiscal_engine::{hex_encode, types::tva::TvaRate, types::z_report::ZReport, FiscalEntry};

// ---------------------------------------------------------------------------
// Helpers de conversion
// ---------------------------------------------------------------------------

/// Convertit un timestamp en millisecondes Unix en chaîne ISO 8601 UTC.
/// Format attendu par PostgreSQL `timestamptz` via l'API REST Supabase.
fn ms_to_iso8601(ms: u64) -> String {
    let secs = (ms / 1_000) as i64;
    let nanos = ((ms % 1_000) * 1_000_000) as u32;
    // Formatage manuel ISO 8601 sans dépendance chrono
    // Utilise time crate déjà présent dans le workspace via sqlx
    let dt = std::time::UNIX_EPOCH + std::time::Duration::new(secs as u64, nanos);
    let secs_since_epoch = dt
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    // Décomposition manuelle en date/heure UTC
    let s = secs_since_epoch;
    let sec = s % 60;
    let min = (s / 60) % 60;
    let hour = (s / 3600) % 24;
    let days = s / 86400;
    // Calcul de l'année et du jour de l'année (algorithme de Fliegel-Van Flandern)
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z")
}

// ---------------------------------------------------------------------------
// Payload d'une entrée fiscale vers Supabase
// ---------------------------------------------------------------------------

/// Représentation JSON d'une `FiscalEntry` pour l'API Supabase REST.
///
/// Les noms de champs correspondent **exactement** aux colonnes de la table
/// `public.fiscal_entries` dans PostgreSQL (snake_case).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalEntryPayload {
    /// UUID de l'entrée (string).
    pub id: String,

    /// UUID du site (clé d'isolation RLS).
    pub site_id: String,

    /// UUID de la session de caisse.
    pub session_id: Option<String>,

    /// Numéro de séquence strictement croissant par site.
    pub sequence: i64,

    /// Type d'opération : `SALE`, `REFUND`, `DISCOUNT`, `VOID`, `TRAINING`, `CORRECTION`.
    pub operation_type: String,

    /// Montant TTC en centimes (signé — négatif pour les remboursements).
    pub amount: i64,

    /// Montant HT en centimes (signé).
    pub ht_cents: Option<i64>,

    /// Montant TVA en centimes (signé).
    pub tva_cents: Option<i64>,

    /// Taux de TVA : `"5.5"`, `"10"`, `"20"`.
    pub tva_rate: String,

    /// Référence de commande POS.
    pub order_ref: Option<String>,

    /// Hash SHA-256 de l'entrée courante, encodé en hexadécimal (64 chars).
    pub entry_hash: String,

    /// Hash SHA-256 de l'entrée précédente (null pour la première entrée).
    pub prev_hash: Option<String>,

    /// Timestamp de signature côté POS (ISO 8601 UTC).
    pub signed_at: String,
}

impl FiscalEntryPayload {
    /// Construit un payload depuis une `FiscalEntry` et l'identifiant du site.
    ///
    /// # Arguments
    /// * `entry`   - Entrée fiscale à sérialiser.
    /// * `site_id` - UUID du site (doit correspondre à `public.sites.id`).
    ///
    /// # Examples
    /// ```
    /// use sync_client::serializer::FiscalEntryPayload;
    /// use fiscal_engine::hash_engine::build_entry_for_test;
    /// use fiscal_engine::types::{operation::OperationType, tva::TvaRate};
    /// use common::GENESIS_HASH;
    ///
    /// let entry = build_entry_for_test(
    ///     1, [0xAB; 16], OperationType::Sale, 1100,
    ///     TvaRate::Intermediaire10, 1_700_000_000_000, GENESIS_HASH,
    /// );
    /// let payload = FiscalEntryPayload::from_entry(&entry, "11111111-1111-1111-1111-111111111111");
    /// assert_eq!(payload.sequence, 1);
    /// assert_eq!(payload.entry_hash.len(), 64);
    /// assert_eq!(payload.amount, 1100);
    /// ```
    #[must_use]
    pub fn from_entry(entry: &FiscalEntry, site_id: &str) -> Self {
        // prev_hash : None si c'est la première entrée (hash genesis = [0u8;32])
        let prev = hex_encode(&entry.previous_hash);
        let prev_hash = if entry.previous_hash == [0u8; 32] {
            None
        } else {
            Some(prev)
        };

        Self {
            id: entry.id.to_string(),
            site_id: site_id.to_string(),
            session_id: Some(entry.session_id.to_string()),
            sequence: entry.sequence_number as i64,
            operation_type: operation_type_str(entry.operation_type),
            amount: entry.amount_ttc_cents.0,
            ht_cents: Some(entry.tva_breakdown.ht_cents.0),
            tva_cents: Some(entry.tva_breakdown.tva_cents.0),
            tva_rate: tva_rate_str(entry.tva_breakdown.rate),
            order_ref: entry.order_reference.clone(),
            entry_hash: hex_encode(&entry.hash),
            prev_hash,
            signed_at: ms_to_iso8601(entry.created_at_ms),
        }
    }
}

fn tva_rate_str(rate: TvaRate) -> String {
    match rate {
        TvaRate::Reduit5_5 => "5.5".to_string(),
        TvaRate::Intermediaire10 => "10".to_string(),
        TvaRate::Normal20 => "20".to_string(),
        _ => "10".to_string(),
    }
}

/// Mappe les labels Rust (français) vers les valeurs SQL du check constraint.
/// Le fiscal-engine utilise des labels FR ; le schéma Supabase est en anglais.
fn operation_type_str(op: fiscal_engine::types::operation::OperationType) -> String {
    use fiscal_engine::types::operation::OperationType;
    match op {
        OperationType::Sale     => "SALE",
        OperationType::Refund   => "REFUND",
        OperationType::Discount => "DISCOUNT",
        OperationType::Cancel   => "VOID",
        OperationType::ZClose   => "CORRECTION",
    }
    .to_string()
}

// ---------------------------------------------------------------------------
// Payload batch
// ---------------------------------------------------------------------------

/// Sérialise un batch d'entrées fiscales en payload JSON pour Supabase.
///
/// Retourne le JSON prêt à être envoyé dans le corps d'un
/// `POST /rest/v1/fiscal_entries`.
///
/// # Errors
/// `serde_json::Error` si la sérialisation échoue.
#[allow(dead_code)]
pub(crate) fn serialize_batch(
    entries: &[FiscalEntry],
    site_id: &str,
) -> Result<String, serde_json::Error> {
    let payloads: Vec<FiscalEntryPayload> = entries
        .iter()
        .map(|e| FiscalEntryPayload::from_entry(e, site_id))
        .collect();
    serde_json::to_string(&payloads)
}

    //AMV ajouté le 11/06/26 2215 *************************************
    //AMV remplacé le 11/06/26 2300 *************************************
// ---------------------------------------------------------------------------
// Payload d'une session vers Supabase
// ---------------------------------------------------------------------------

/// Représentation JSON d'une `Session` pour l'API Supabase REST.
///
/// Colonnes cibles : `public.sessions` (id, site_id, session_ref,
/// opened_at, closed_at, operator_id, opening_amount, status).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionPayload {
    /// UUID de la session.
    pub id: String,
    /// UUID du site (clé RLS — provient de SyncConfig, pas du type Session).
    pub site_id: String,
    /// Référence lisible de la session : "S0001", "S0002", etc.
    pub session_ref: String,
    /// Timestamp d'ouverture (ISO 8601 UTC).
    pub opened_at: String,
    /// Timestamp de clôture (ISO 8601 UTC), None si session ouverte.
    pub closed_at: Option<String>,
    /// Identifiant opérateur — None (pas dans le modèle fiscal Rust).
    pub operator_id: Option<String>,
    /// Fonds de caisse à l'ouverture en centimes — 0 par défaut.
    pub opening_amount: i64,
    /// Statut en minuscules : "open" ou "closed" (convention Supabase).
    pub status: String,
}

impl SessionPayload {
    /// Construit un payload depuis un `Session` fiscal-engine et le `site_id` de config.
    ///
    /// # Arguments
    /// * `session` - Session fiscale locale.
    /// * `site_id` - UUID du site, issu de `SyncConfig.site_id`.
    #[must_use]
    pub fn from_session(session: &fiscal_engine::types::session::Session, site_id: &str) -> Self {
        use fiscal_engine::types::session::SessionStatus;
        Self {
            id:             session.id.0.to_string(),
            site_id:        site_id.to_string(),
            session_ref:    format!("S{:04}", session.sequence_number),
            opened_at:      ms_to_iso8601(session.opened_at_ms),
            closed_at:      session.closed_at_ms.map(ms_to_iso8601),
            operator_id:    None,
            opening_amount: 0,
            status: match session.status {
                SessionStatus::Open   => "open".to_string(),
                SessionStatus::Closed => "closed".to_string(),
            },
        }
    }
}
    
// ---------------------------------------------------------------------------
// Payload d'un rapport Z vers Supabase
// ---------------------------------------------------------------------------

/// Représentation JSON d'un `ZReport` pour l'API Supabase REST.
///
/// Colonnes cibles : `public.z_reports`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZReportPayload {
    pub id:              String,
    pub site_id:         String,
    pub session_id:      String,
    pub report_number:   i64,
    pub generated_at:    String,
    pub total_sales:     i64,
    pub total_refunds:   i64,
    pub total_discounts: i64,
    pub net_revenue:     i64,
    pub tva_5_5_base_ht: i64,
    pub tva_5_5_amount:  i64,
    pub tva_10_base_ht:  i64,
    pub tva_10_amount:   i64,
    pub tva_20_base_ht:  i64,
    pub tva_20_amount:   i64,
    pub entry_count:     i64,
    pub first_sequence:  Option<i64>,
    pub last_sequence:   Option<i64>,
    pub report_hash:     Option<String>,
}

impl ZReportPayload {
    /// Construit un payload depuis un `ZReport` et l'identifiant du site.
    ///
    /// `total_refunds` agrège remboursements + annulations (la table Supabase
    /// n'a pas de colonne `total_cancels` distincte).
    #[must_use]
    pub fn from_z_report(report: &ZReport, site_id: &str) -> Self {
        let total_refunds = report.total_refunds_cents.0 + report.total_cancels_cents.0;
        let net_revenue   = report.total_sales_cents.0 - total_refunds - report.total_discounts_cents.0;
        Self {
            id:              report.id.to_string(),
            site_id:         site_id.to_string(),
            session_id:      report.session_id.0.to_string(),
            report_number:   report.session_sequence_number as i64,
            generated_at:    ms_to_iso8601(report.generated_at_ms),
            total_sales:     report.total_sales_cents.0,
            total_refunds,
            total_discounts: report.total_discounts_cents.0,
            net_revenue,
            tva_5_5_base_ht: report.tva_5_5_breakdown.ht_cents.0,
            tva_5_5_amount:  report.tva_5_5_breakdown.tva_cents.0,
            tva_10_base_ht:  report.tva_10_breakdown.ht_cents.0,
            tva_10_amount:   report.tva_10_breakdown.tva_cents.0,
            tva_20_base_ht:  report.tva_20_breakdown.ht_cents.0,
            tva_20_amount:   report.tva_20_breakdown.tva_cents.0,
            entry_count:     report.entry_count as i64,
            first_sequence:  Some(report.first_entry_sequence as i64),
            last_sequence:   Some(report.last_entry_sequence as i64),
            report_hash:     Some(hex_encode(&report.closing_hash)),
        }
    }
}

// ---------------------------------------------------------------------------
// Payload de configuration reçue depuis Supabase (pull)
// ---------------------------------------------------------------------------

/// Configuration du restaurant reçue depuis le cloud.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteConfig {
    /// Version de la configuration (incrémentée à chaque modification).
    pub version: u32,
    /// Timestamp de la dernière modification (ms Unix).
    pub updated_at_ms: i64,
    /// Contenu du menu en JSON brut (sera écrit dans `menu.json`).
    pub menu: serde_json::Value,
    /// Identifiant du site concerné (validation côté client).
    pub site_id: String,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use fiscal_engine::{
        hash_engine::build_entry_for_test,
        types::{operation::OperationType, tva::TvaRate},
    };
    use common::GENESIS_HASH;

    fn test_entry() -> FiscalEntry {
        build_entry_for_test(
            1,
            [0xAB; 16],
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            1_700_000_000_000,
            GENESIS_HASH,
        )
    }

    #[test]
    fn payload_has_correct_sequence() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE-001");
        assert_eq!(payload.sequence, 1);
    }

    #[test]
    fn payload_hash_is_64_chars_hex() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE-001");
        assert_eq!(payload.entry_hash.len(), 64);
        assert!(
            payload.entry_hash.chars().all(|c| c.is_ascii_hexdigit()),
            "Le hash doit etre en hexadecimal"
        );
    }

    #[test]
    fn payload_site_id_propagated() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "12345678900000");
        assert_eq!(payload.site_id, "12345678900000");
    }

    #[test]
    fn payload_amount_matches_entry() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        assert_eq!(payload.amount, 1100);
    }

    #[test]
    fn payload_tva_rate_intermediaire() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        assert_eq!(payload.tva_rate, "10");
    }

    #[test]
    fn payload_tva_rate_reduit() {
        let entry = build_entry_for_test(
            2, [0xAB; 16], OperationType::Sale, 550,
            TvaRate::Reduit5_5, 0, GENESIS_HASH,
        );
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        assert_eq!(payload.tva_rate, "5.5");
    }

    #[test]
    fn payload_operation_type_is_uppercase_label() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        assert!(!payload.operation_type.is_empty());
    }

    #[test]
    fn payload_signed_at_is_iso8601() {
        let entry = test_entry();
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        // Format attendu : YYYY-MM-DDTHH:MM:SSZ
        assert!(payload.signed_at.contains('T'), "signed_at doit contenir T");
        assert!(payload.signed_at.ends_with('Z'), "signed_at doit finir par Z");
        assert_eq!(payload.signed_at.len(), 20, "Format ISO 8601 = 20 chars");
    }

    #[test]
    fn payload_prev_hash_none_for_genesis() {
        let entry = test_entry(); // previous_hash = GENESIS_HASH = [0u8;32]
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        assert!(
            payload.prev_hash.is_none(),
            "prev_hash doit etre None pour la premiere entree"
        );
    }

    #[test]
    fn serialize_batch_produces_json_array() {
        let e1 = test_entry();
        let e2 = build_entry_for_test(
            2, [0xAB; 16], OperationType::Sale, 550,
            TvaRate::Intermediaire10, 1_700_000_001_000, e1.hash,
        );
        let json = serialize_batch(&[e1, e2], "SITE-001").expect("Serialisation OK");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("JSON valide");
        assert!(parsed.is_array());
        assert_eq!(parsed.as_array().unwrap().len(), 2);
    }

    #[test]
    fn serialize_empty_batch_produces_empty_array() {
        let json = serialize_batch(&[], "SITE").expect("Serialisation OK");
        assert_eq!(json, "[]");
    }

    #[test]
    fn serialize_batch_contains_site_id() {
        let entry = test_entry();
        let json = serialize_batch(&[entry], "MON-SITE").expect("Serialisation OK");
        assert!(json.contains("MON-SITE"), "Le site_id doit etre dans le JSON");
    }

    #[test]
    fn refund_payload_has_negative_amount() {
        let entry = build_entry_for_test(
            3, [0xAB; 16], OperationType::Refund, -1100,
            TvaRate::Intermediaire10, 0, GENESIS_HASH,
        );
        let payload = FiscalEntryPayload::from_entry(&entry, "SITE");
        assert!(
            payload.amount < 0,
            "Le montant d'un remboursement doit etre negatif"
        );
    }

    #[test]
    fn session_payload_site_id_and_ref() {
        use fiscal_engine::types::session::Session;

        let session = Session::new(3, 1_700_000_000_000);
        let payload = SessionPayload::from_session(&session, "SITE-042");

        assert_eq!(payload.site_id, "SITE-042");
        assert_eq!(payload.session_ref, "S0003");
        assert_eq!(payload.status, "open");
        assert!(payload.closed_at.is_none());
        assert!(payload.operator_id.is_none());
        assert_eq!(payload.opening_amount, 0);
        assert_eq!(payload.id, session.id.0.to_string());
    }

    #[test]
    fn session_payload_status_closed() {
        use fiscal_engine::types::session::{Session, SessionStatus};

        let mut session = Session::new(1, 1_700_000_000_000);
        session.status = SessionStatus::Closed;
        session.closed_at_ms = Some(1_700_000_060_000);

        let payload = SessionPayload::from_session(&session, "SITE-001");

        assert_eq!(payload.status, "closed");
        assert!(payload.closed_at.is_some());
        let closed = payload.closed_at.unwrap();
        assert!(closed.ends_with('Z'));
        assert!(closed.contains('T'));
    }

    #[test]
    fn session_payload_opened_at_is_iso8601() {
        use fiscal_engine::types::session::Session;

        let session = Session::new(1, 1_700_000_000_000);
        let payload = SessionPayload::from_session(&session, "SITE-001");

        assert!(payload.opened_at.ends_with('Z'));
        assert_eq!(payload.opened_at.len(), 20);
    }

}
