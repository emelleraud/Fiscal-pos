//! # `z_report`
//!
//! Rapport Z : document fiscal de clôture de session de caisse.
//!
//! ## Définition NF525 (§6)
//! Le rapport Z est généré à la clôture de chaque session de caisse.
//! Il est **immuable** : une fois produit, son contenu ne peut jamais être modifié.
//! Il contient notamment :
//! - Les totaux par type d'opération (ventes, remboursements, annulations, remises)
//! - La décomposition de la TVA par taux
//! - Le hash de clôture (dernière entrée de la session)
//! - La plage de numéros de séquence couverte
//!
//! ## Immuabilité
//! Le `ZReport` est produit une seule fois par session. Toute tentative de régénération
//! retourne `SessionError::ZReportAlreadyGenerated`.

use common::{Cents, SessionId};
use serde::{Deserialize, Serialize};

use super::tva::TvaBreakdown;

/// Rapport Z de clôture de session.
///
/// Document fiscal immuable produit lors de la clôture d'une session de caisse.
///
/// # Examples
/// ```
/// use fiscal_engine::types::z_report::ZReport;
/// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
/// use common::{Cents, SessionId};
///
/// // En pratique, généré par Journal::generate_z_report() (Étape 5)
/// let report = ZReport {
///     id: uuid::Uuid::now_v7(),
///     session_id: SessionId::new(),
///     session_sequence_number: 1,
///     generated_at_ms: 1_700_000_000_000,
///     first_entry_sequence: 1,
///     last_entry_sequence: 42,
///     entry_count: 42,
///     total_sales_cents: Cents(150_000),
///     total_refunds_cents: Cents(5_000),
///     total_cancels_cents: Cents(2_000),
///     total_discounts_cents: Cents(1_000),
///     tva_5_5_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Reduit5_5),
///     tva_10_breakdown: TvaBreakdown::from_ttc(Cents(142_000), TvaRate::Intermediaire10),
///     tva_20_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Normal20),
///     closing_hash: [0u8; 32],
/// };
/// assert_eq!(report.net_revenue(), Cents(142_000));
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZReport {
    /// Identifiant unique du rapport Z.
    pub id: uuid::Uuid,

    /// Session de caisse clôturée par ce rapport.
    pub session_id: SessionId,

    /// Numéro de séquence de la session (pour identification dans les archives).
    pub session_sequence_number: u64,

    /// Timestamp de génération en millisecondes Unix (UTC).
    pub generated_at_ms: u64,

    // --- Plage de séquences couverte ---
    /// Numéro de séquence de la première entrée de la session.
    pub first_entry_sequence: u64,

    /// Numéro de séquence de la dernière entrée de la session (entrée `ZClose`).
    pub last_entry_sequence: u64,

    /// Nombre total d'entrées dans la session (incluant l'entrée `ZClose`).
    pub entry_count: u64,

    // --- Totaux financiers ---
    /// Total des ventes TTC en centimes.
    pub total_sales_cents: Cents,

    /// Total des remboursements en centimes (valeur absolue, positif).
    pub total_refunds_cents: Cents,

    /// Total des annulations en centimes (valeur absolue, positif).
    pub total_cancels_cents: Cents,

    /// Total des remises en centimes (valeur absolue, positif).
    pub total_discounts_cents: Cents,

    // --- Décomposition TVA (obligatoire sur le rapport Z, CGI art. 289) ---
    /// Part des ventes soumises au taux réduit 5,5%.
    pub tva_5_5_breakdown: TvaBreakdown,

    /// Part des ventes soumises au taux intermédiaire 10%.
    pub tva_10_breakdown: TvaBreakdown,

    /// Part des ventes soumises au taux normal 20%.
    pub tva_20_breakdown: TvaBreakdown,

    // --- Ancrage cryptographique ---
    /// Hash SHA-256 de la dernière entrée de la session (type `ZClose`).
    /// Permet de vérifier que le rapport Z est cohérent avec le journal.
    pub closing_hash: [u8; 32],
}

impl ZReport {
    /// Calcule le chiffre d'affaires net de la session.
    ///
    /// `CA net = ventes - remboursements - annulations - remises`
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::z_report::ZReport;
    /// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
    /// use common::{Cents, SessionId};
    ///
    /// let mut report = ZReport {
    ///     id: uuid::Uuid::now_v7(),
    ///     session_id: SessionId::new(),
    ///     session_sequence_number: 1,
    ///     generated_at_ms: 0,
    ///     first_entry_sequence: 1,
    ///     last_entry_sequence: 10,
    ///     entry_count: 10,
    ///     total_sales_cents: Cents(10_000),
    ///     total_refunds_cents: Cents(500),
    ///     total_cancels_cents: Cents(0),
    ///     total_discounts_cents: Cents(200),
    ///     tva_5_5_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Reduit5_5),
    ///     tva_10_breakdown: TvaBreakdown::from_ttc(Cents(9_300), TvaRate::Intermediaire10),
    ///     tva_20_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Normal20),
    ///     closing_hash: [0u8; 32],
    /// };
    /// assert_eq!(report.net_revenue(), Cents(9_300));
    /// ```
    #[must_use]
    pub fn net_revenue(&self) -> Cents {
        self.total_sales_cents
            - self.total_refunds_cents
            - self.total_cancels_cents
            - self.total_discounts_cents
    }

    /// Calcule le total TVA collectée toutes tranches confondues.
    ///
    /// Somme des TVA par taux, exprimée en centimes.
    #[must_use]
    pub fn total_tva_cents(&self) -> Cents {
        self.tva_5_5_breakdown.tva_cents
            + self.tva_10_breakdown.tva_cents
            + self.tva_20_breakdown.tva_cents
    }

    /// Calcule le total HT toutes tranches confondues.
    #[must_use]
    pub fn total_ht_cents(&self) -> Cents {
        self.tva_5_5_breakdown.ht_cents
            + self.tva_10_breakdown.ht_cents
            + self.tva_20_breakdown.ht_cents
    }

    /// Vérifie la cohérence interne du rapport Z.
    ///
    /// Contrôles effectués :
    /// 1. `total_ht + total_tva ≈ total_sales` (à 1 centime d'arrondi près)
    /// 2. `first_entry_sequence <= last_entry_sequence`
    /// 3. `entry_count >= 1` (au moins l'entrée `ZClose`)
    ///
    /// # Errors
    /// Retourne une description de l'incohérence détectée.
    pub fn verify_internal_consistency(&self) -> Result<(), String> {
        // 1. Cohérence séquences
        if self.first_entry_sequence > self.last_entry_sequence {
            return Err(format!(
                "Séquence invalide : first={} > last={}",
                self.first_entry_sequence, self.last_entry_sequence
            ));
        }

        // 2. Au moins une entrée (ZClose minimum)
        if self.entry_count == 0 {
            return Err("Un rapport Z doit couvrir au moins une entrée".to_string());
        }

        // 3. Cohérence TVA vs total ventes (tolérance 1 centime pour les arrondis)
        let tva_total_ttc = self.tva_5_5_breakdown.ttc_cents
            + self.tva_10_breakdown.ttc_cents
            + self.tva_20_breakdown.ttc_cents;

        let diff = (tva_total_ttc.0 - self.total_sales_cents.0).abs();
        if diff > 1 {
            return Err(format!(
                "Décomposition TVA incohérente : somme TTC par taux={} ≠ total ventes={} (diff={})",
                tva_total_ttc.0, self.total_sales_cents.0, diff
            ));
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::tva::{TvaBreakdown, TvaRate};

    fn sample_report() -> ZReport {
        ZReport {
            id: uuid::Uuid::now_v7(),
            session_id: SessionId::new(),
            session_sequence_number: 1,
            generated_at_ms: 1_700_000_000_000,
            first_entry_sequence: 1,
            last_entry_sequence: 10,
            entry_count: 10,
            total_sales_cents: Cents(10_000),
            total_refunds_cents: Cents(500),
            total_cancels_cents: Cents(200),
            total_discounts_cents: Cents(100),
            tva_5_5_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(10_000), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Normal20),
            closing_hash: [0u8; 32],
        }
    }

    #[test]
    fn net_revenue_calculation() {
        let r = sample_report();
        // 100 - 5 - 2 - 1 = 92 €
        assert_eq!(r.net_revenue(), Cents(9_200));
    }

    #[test]
    fn total_tva_10_only() {
        let r = sample_report();
        // 10 000 TTC à 10% → TVA = 909 centimes (arrondi)
        let expected_tva = TvaBreakdown::from_ttc(Cents(10_000), TvaRate::Intermediaire10).tva_cents;
        assert_eq!(r.total_tva_cents(), expected_tva);
    }

    #[test]
    fn internal_consistency_valid_report() {
        let r = sample_report();
        assert!(r.verify_internal_consistency().is_ok());
    }

    #[test]
    fn internal_consistency_invalid_sequence() {
        let mut r = sample_report();
        r.first_entry_sequence = 20;
        r.last_entry_sequence = 10; // incohérent
        assert!(r.verify_internal_consistency().is_err());
    }

    #[test]
    fn internal_consistency_zero_entries() {
        let mut r = sample_report();
        r.entry_count = 0;
        assert!(r.verify_internal_consistency().is_err());
    }

    #[test]
    fn total_ht_and_tva_sum_to_sales() {
        let r = sample_report();
        // ht + tva doit être proche de total_sales (tolérance 1 centime)
        let diff = (r.total_ht_cents() + r.total_tva_cents() - r.total_sales_cents)
            .0
            .abs();
        assert!(diff <= 1, "diff = {diff} centimes (> 1 centime toléré)");
    }
}
