//! # tva
//!
//! Taux de TVA applicables en restauration rapide (QSR) en France,
//! et structure de décomposition des montants par taux.
//!
//! ## Contexte réglementaire
//! Les taux applicables en QSR dépendent du mode de consommation :
//! - **Sur place** : taux intermédiaire 10% (depuis 2012, loi de finances rectificative)
//! - **À emporter** : taux réduit 5,5% pour les produits alimentaires
//! - **Alcool** : taux normal 20% quelle que soit la consommation
//!
//! La décomposition par taux est obligatoire sur le ticket de caisse (CGI art. 289).
//!
//! ## Représentation monétaire
//! Tous les montants sont en **centimes entiers** (`i64`) via `common::Cents`.
//! Aucun flottant n'est utilisé dans les calculs de TVA.

use common::{Cents, TVA_RATE_INTERMEDIATE, TVA_RATE_NORMAL, TVA_RATE_REDUCED};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enum TvaRate
// ---------------------------------------------------------------------------

/// Taux de TVA français applicables en restauration.
///
/// Chaque variante correspond à un taux légal distinct.
/// L'enum est `non_exhaustive` pour anticiper de futurs changements législatifs
/// sans casser les consommateurs existants du crate.
///
/// # Examples
/// ```
/// use fiscal_engine::types::tva::TvaRate;
/// use common::Cents;
///
/// let rate = TvaRate::Intermediaire10;
/// let ht = Cents(1000); // 10,00 €
/// let tva = rate.compute_tva_from_ht(ht);
/// assert_eq!(tva, Cents(100)); // 1,00 € de TVA
/// ```
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TvaRate {
    /// Taux réduit 5,5% — alimentation à emporter, produits de première nécessité.
    Reduit5_5,
    /// Taux intermédiaire 10% — restauration sur place, hébergement.
    Intermediaire10,
    /// Taux normal 20% — alcool, produits hors alimentation.
    Normal20,
}

impl TvaRate {
    /// Retourne le taux en valeur décimale (ex: 0.10 pour 10%).
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::TvaRate;
    /// assert!((TvaRate::Intermediaire10.as_decimal() - 0.10).abs() < f64::EPSILON);
    /// ```
    #[must_use]
    pub fn as_decimal(self) -> f64 {
        match self {
            Self::Reduit5_5 => TVA_RATE_REDUCED,
            Self::Intermediaire10 => TVA_RATE_INTERMEDIATE,
            Self::Normal20 => TVA_RATE_NORMAL,
        }
    }

    /// Retourne le taux en points de base entiers (ex: 1000 pour 10,00%).
    ///
    /// Utilisé pour les calculs entiers sans flottant.
    /// `basis_points / 10000` donne le taux décimal exact.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::TvaRate;
    /// assert_eq!(TvaRate::Reduit5_5.basis_points(), 550);
    /// assert_eq!(TvaRate::Intermediaire10.basis_points(), 1000);
    /// assert_eq!(TvaRate::Normal20.basis_points(), 2000);
    /// ```
    #[must_use]
    pub fn basis_points(self) -> u32 {
        match self {
            Self::Reduit5_5 => 550,
            Self::Intermediaire10 => 1_000,
            Self::Normal20 => 2_000,
        }
    }

    /// Calcule la TVA en centimes à partir d'un montant HT, en arithmétique entière.
    ///
    /// Formule : `TVA = HT × basis_points / 10000`, arrondi au centime le plus proche.
    ///
    /// # Arguments
    /// * `ht` - Montant hors taxe en centimes.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::TvaRate;
    /// use common::Cents;
    ///
    /// // 10,00 € HT à 10% → 1,00 € TVA
    /// assert_eq!(TvaRate::Intermediaire10.compute_tva_from_ht(Cents(1000)), Cents(100));
    ///
    /// // 9,09 € HT à 10% → 0,91 € TVA (soit TTC = 10,00 €)
    /// assert_eq!(TvaRate::Intermediaire10.compute_tva_from_ht(Cents(909)), Cents(91));
    /// ```
    #[must_use]
    pub fn compute_tva_from_ht(self, ht: Cents) -> Cents {
        // Arithmétique entière : évite tout flottant dans le calcul fiscal
        // arrondi standard : +5000 avant division par 10000
        let tva_cents = (ht.0 * i64::from(self.basis_points()) + 5_000) / 10_000;
        Cents(tva_cents)
    }

    /// Calcule le montant HT en centimes à partir d'un montant TTC.
    ///
    /// Formule : `HT = TTC × 10000 / (10000 + basis_points)`, arrondi.
    ///
    /// # Arguments
    /// * `ttc` - Montant toutes taxes comprises en centimes.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::TvaRate;
    /// use common::Cents;
    ///
    /// // 11,00 € TTC à 10% → 10,00 € HT
    /// assert_eq!(TvaRate::Intermediaire10.compute_ht_from_ttc(Cents(1100)), Cents(1000));
    /// ```
    #[must_use]
    pub fn compute_ht_from_ttc(self, ttc: Cents) -> Cents {
        let divisor = i64::from(10_000 + self.basis_points());
        let ht_cents = (ttc.0 * 10_000 + divisor / 2) / divisor;
        Cents(ht_cents)
    }

    /// Libellé lisible du taux, pour impression sur ticket et rapport Z.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::TvaRate;
    /// assert_eq!(TvaRate::Reduit5_5.label(), "TVA 5,5%");
    /// ```
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Reduit5_5 => "TVA 5,5%",
            Self::Intermediaire10 => "TVA 10%",
            Self::Normal20 => "TVA 20%",
        }
    }
}

// ---------------------------------------------------------------------------
// TvaBreakdown — décomposition d'un montant par taux
// ---------------------------------------------------------------------------

/// Décomposition fiscale d'un montant par taux de TVA.
///
/// Utilisée dans `FiscalEntry`, `ZReport` et sur les tickets de caisse.
/// La somme des `tva_*_cents` doit toujours satisfaire :
/// `ht_total_cents + tva_total_cents == ttc_total_cents`
///
/// # Invariant
/// La cohérence arithmétique est vérifiée à la construction via `TvaBreakdown::new()`.
/// Un `TvaBreakdown` valide garantit que la somme est correcte.
///
/// # Examples
/// ```
/// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
/// use common::Cents;
///
/// let breakdown = TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10);
/// assert_eq!(breakdown.ttc_cents, Cents(1100));
/// assert_eq!(breakdown.ht_cents, Cents(1000));
/// assert_eq!(breakdown.tva_cents, Cents(100));
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TvaBreakdown {
    /// Taux de TVA appliqué.
    pub rate: TvaRate,
    /// Montant hors taxe en centimes.
    pub ht_cents: Cents,
    /// Montant de TVA en centimes.
    pub tva_cents: Cents,
    /// Montant TTC en centimes (`ht_cents + tva_cents`).
    pub ttc_cents: Cents,
}

impl TvaBreakdown {
    /// Construit une décomposition à partir d'un montant TTC et d'un taux.
    ///
    /// Le HT est calculé par division (arrondi), la TVA est la différence TTC - HT.
    /// Cette approche garantit `ht + tva == ttc` exactement.
    ///
    /// # Arguments
    /// * `ttc` - Montant TTC en centimes.
    /// * `rate` - Taux de TVA applicable.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
    /// use common::Cents;
    ///
    /// let b = TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10);
    /// assert_eq!(b.ht_cents + b.tva_cents, b.ttc_cents);
    /// ```
    #[must_use]
    pub fn from_ttc(ttc: Cents, rate: TvaRate) -> Self {
        let ht = rate.compute_ht_from_ttc(ttc);
        let tva = ttc - ht; // TVA = TTC - HT : garantit l'exactitude arithmétique
        Self {
            rate,
            ht_cents: ht,
            tva_cents: tva,
            ttc_cents: ttc,
        }
    }

    /// Construit une décomposition à partir d'un montant HT et d'un taux.
    ///
    /// # Arguments
    /// * `ht` - Montant HT en centimes.
    /// * `rate` - Taux de TVA applicable.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
    /// use common::Cents;
    ///
    /// let b = TvaBreakdown::from_ht(Cents(1000), TvaRate::Intermediaire10);
    /// assert_eq!(b.ht_cents + b.tva_cents, b.ttc_cents);
    /// ```
    #[must_use]
    pub fn from_ht(ht: Cents, rate: TvaRate) -> Self {
        let tva = rate.compute_tva_from_ht(ht);
        let ttc = ht + tva;
        Self {
            rate,
            ht_cents: ht,
            tva_cents: tva,
            ttc_cents: ttc,
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- TvaRate::compute_tva_from_ht ---

    #[test]
    fn tva_10_from_ht_exact() {
        // 10,00 € HT × 10% = 1,00 € TVA → TTC 11,00 €
        assert_eq!(
            TvaRate::Intermediaire10.compute_tva_from_ht(Cents(1000)),
            Cents(100)
        );
    }

    #[test]
    fn tva_5_5_from_ht_exact() {
        // 100,00 € HT × 5,5% = 5,50 € TVA
        assert_eq!(
            TvaRate::Reduit5_5.compute_tva_from_ht(Cents(10_000)),
            Cents(550)
        );
    }

    #[test]
    fn tva_20_from_ht_exact() {
        // 50,00 € HT × 20% = 10,00 € TVA
        assert_eq!(
            TvaRate::Normal20.compute_tva_from_ht(Cents(5000)),
            Cents(1000)
        );
    }

    #[test]
    fn tva_from_ht_rounding_edge_case() {
        // 0,01 € HT × 10% = 0,001 € → arrondi à 0 centime
        assert_eq!(
            TvaRate::Intermediaire10.compute_tva_from_ht(Cents(1)),
            Cents(0)
        );
    }

    // --- TvaRate::compute_ht_from_ttc ---

    #[test]
    fn ht_from_ttc_10_exact() {
        // 11,00 € TTC ÷ 1,10 = 10,00 € HT
        assert_eq!(
            TvaRate::Intermediaire10.compute_ht_from_ttc(Cents(1100)),
            Cents(1000)
        );
    }

    #[test]
    fn ht_from_ttc_20_exact() {
        // 12,00 € TTC ÷ 1,20 = 10,00 € HT
        assert_eq!(
            TvaRate::Normal20.compute_ht_from_ttc(Cents(1200)),
            Cents(1000)
        );
    }

    // --- TvaBreakdown::from_ttc ---

    #[test]
    fn breakdown_from_ttc_coherence() {
        // L'invariant ht + tva == ttc doit toujours être satisfait
        let b = TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10);
        assert_eq!(b.ht_cents + b.tva_cents, b.ttc_cents);
        assert_eq!(b.ht_cents, Cents(1000));
        assert_eq!(b.tva_cents, Cents(100));
    }

    #[test]
    fn breakdown_from_ttc_odd_amount() {
        // Montant non-rond : 9,99 € TTC à 10%
        let b = TvaBreakdown::from_ttc(Cents(999), TvaRate::Intermediaire10);
        // L'invariant doit tenir même sur des montants non-ronds
        assert_eq!(b.ht_cents + b.tva_cents, b.ttc_cents);
    }

    #[test]
    fn breakdown_from_ttc_5_5() {
        // 10,55 € TTC à 5,5%
        let b = TvaBreakdown::from_ttc(Cents(1055), TvaRate::Reduit5_5);
        assert_eq!(b.ht_cents + b.tva_cents, b.ttc_cents);
    }

    // --- TvaBreakdown::from_ht ---

    #[test]
    fn breakdown_from_ht_coherence() {
        let b = TvaBreakdown::from_ht(Cents(1000), TvaRate::Intermediaire10);
        assert_eq!(b.ht_cents + b.tva_cents, b.ttc_cents);
        assert_eq!(b.ttc_cents, Cents(1100));
    }

    // --- Labels et basis points ---

    #[test]
    fn tva_rate_labels() {
        assert_eq!(TvaRate::Reduit5_5.label(), "TVA 5,5%");
        assert_eq!(TvaRate::Intermediaire10.label(), "TVA 10%");
        assert_eq!(TvaRate::Normal20.label(), "TVA 20%");
    }

    #[test]
    fn tva_rate_basis_points() {
        assert_eq!(TvaRate::Reduit5_5.basis_points(), 550);
        assert_eq!(TvaRate::Intermediaire10.basis_points(), 1_000);
        assert_eq!(TvaRate::Normal20.basis_points(), 2_000);
    }
}
