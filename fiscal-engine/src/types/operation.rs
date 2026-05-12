//! # operation
//!
//! Types d'opérations fiscales enregistrables dans le journal NF525.
//!
//! ## Classification NF525 (§3.2)
//! Le référentiel NF525 distingue les opérations selon leur impact sur le chiffre d'affaires :
//! - **Opérations positives** : `Sale` — augmentent le CA
//! - **Opérations négatives** : `Refund` — diminuent le CA (nécessitent un motif)
//! - **Opérations neutres** : `Cancel`, `Discount`, `ZClose` — pas d'impact direct sur le CA
//!   mais doivent être tracées dans le journal
//!
//! ## Règle de signe sur les montants
//! - `Sale` : montant TTC **positif**
//! - `Refund` : montant TTC **négatif** (remboursement client)
//! - `Cancel` : montant TTC **négatif** (annulation d'une vente antérieure)
//! - `Discount` : montant TTC **négatif** (remise accordée)
//! - `ZClose` : montant = 0 (opération de clôture, pas de flux financier)

use serde::{Deserialize, Serialize};

/// Type d'opération fiscale enregistrable dans le journal.
///
/// Chaque variante détermine :
/// - Le signe attendu du montant associé
/// - L'impact sur les totaux du rapport Z
/// - Les champs obligatoires de `FiscalEntry`
///
/// # Examples
/// ```
/// use fiscal_engine::types::operation::OperationType;
///
/// let op = OperationType::Sale;
/// assert!(op.increases_revenue());
/// assert!(!op.requires_reason());
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OperationType {
    /// Vente : enregistrement d'une transaction commerciale.
    ///
    /// Montant TTC attendu : **strictement positif**.
    /// Impact rapport Z : incrémente `total_sales`.
    Sale,

    /// Remboursement : restitution d'un montant à un client.
    ///
    /// Montant TTC attendu : **strictement négatif**.
    /// Impact rapport Z : incrémente `total_refunds` (en valeur absolue).
    /// Champ obligatoire : `reason` dans `FiscalEntry`.
    Refund,

    /// Annulation : invalidation d'une opération de vente antérieure.
    ///
    /// Différent du remboursement : l'annulation cible une `FiscalEntry` existante.
    /// Montant TTC attendu : **strictement négatif** (symétrique à la vente annulée).
    /// Impact rapport Z : incrémente `total_cancels`.
    /// Champ obligatoire : `cancelled_entry_id` dans `FiscalEntry`.
    Cancel,

    /// Remise commerciale : réduction accordée sur une vente.
    ///
    /// Montant TTC attendu : **strictement négatif** (valeur de la remise).
    /// Impact rapport Z : incrémente `total_discounts`.
    Discount,

    /// Clôture de session : opération synthétique générée lors d'un rapport Z.
    ///
    /// Montant = 0 (pas de flux financier).
    /// Cette entrée sert de marqueur de fin de session dans la chaîne de hash.
    /// Il ne peut y avoir qu'une seule entrée `ZClose` par session.
    ZClose,
}

impl OperationType {
    /// Indique si l'opération augmente le chiffre d'affaires.
    ///
    /// Seule `Sale` est positive du point de vue du CA.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::operation::OperationType;
    /// assert!(OperationType::Sale.increases_revenue());
    /// assert!(!OperationType::Refund.increases_revenue());
    /// ```
    #[must_use]
    pub fn increases_revenue(self) -> bool {
        matches!(self, Self::Sale)
    }

    /// Indique si l'opération nécessite obligatoirement un motif textuel.
    ///
    /// NF525 §4.3 : les remboursements et annulations doivent être justifiés.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::operation::OperationType;
    /// assert!(OperationType::Refund.requires_reason());
    /// assert!(OperationType::Cancel.requires_reason());
    /// assert!(!OperationType::Sale.requires_reason());
    /// ```
    #[must_use]
    pub fn requires_reason(self) -> bool {
        matches!(self, Self::Refund | Self::Cancel)
    }

    /// Signe attendu du montant TTC pour cette opération.
    ///
    /// Retourne :
    /// - `1`  — montant doit être positif (`Sale`)
    /// - `-1` — montant doit être négatif (`Refund`, `Cancel`, `Discount`)
    /// - `0`  — montant doit être nul (`ZClose`)
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::operation::OperationType;
    /// assert_eq!(OperationType::Sale.expected_amount_sign(), 1);
    /// assert_eq!(OperationType::Refund.expected_amount_sign(), -1);
    /// assert_eq!(OperationType::ZClose.expected_amount_sign(), 0);
    /// ```
    #[must_use]
    pub fn expected_amount_sign(self) -> i8 {
        match self {
            Self::Sale => 1,
            Self::Refund | Self::Cancel | Self::Discount => -1,
            Self::ZClose => 0,
        }
    }

    /// Valide que le montant TTC (en centimes) est cohérent avec le type d'opération.
    ///
    /// # Arguments
    /// * `amount_cents` - Montant TTC en centimes.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::operation::OperationType;
    /// assert!(OperationType::Sale.is_amount_valid(1000));
    /// assert!(!OperationType::Sale.is_amount_valid(-1000));
    /// assert!(OperationType::Refund.is_amount_valid(-500));
    /// assert!(!OperationType::Refund.is_amount_valid(0));
    /// assert!(OperationType::ZClose.is_amount_valid(0));
    /// ```
    #[must_use]
    pub fn is_amount_valid(self, amount_cents: i64) -> bool {
        match self.expected_amount_sign() {
            1 => amount_cents > 0,
            -1 => amount_cents < 0,
            _ => amount_cents == 0,
        }
    }

    /// Libellé court pour les rapports et logs.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::operation::OperationType;
    /// assert_eq!(OperationType::Sale.label(), "VENTE");
    /// ```
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Sale => "VENTE",
            Self::Refund => "REMBOURSEMENT",
            Self::Cancel => "ANNULATION",
            Self::Discount => "REMISE",
            Self::ZClose => "CLOTURE_Z",
        }
    }
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- increases_revenue ---

    #[test]
    fn only_sale_increases_revenue() {
        assert!(OperationType::Sale.increases_revenue());
        assert!(!OperationType::Refund.increases_revenue());
        assert!(!OperationType::Cancel.increases_revenue());
        assert!(!OperationType::Discount.increases_revenue());
        assert!(!OperationType::ZClose.increases_revenue());
    }

    // --- requires_reason ---

    #[test]
    fn refund_and_cancel_require_reason() {
        assert!(OperationType::Refund.requires_reason());
        assert!(OperationType::Cancel.requires_reason());
        assert!(!OperationType::Sale.requires_reason());
        assert!(!OperationType::Discount.requires_reason());
        assert!(!OperationType::ZClose.requires_reason());
    }

    // --- is_amount_valid ---

    #[test]
    fn sale_requires_positive_amount() {
        assert!(OperationType::Sale.is_amount_valid(1));
        assert!(OperationType::Sale.is_amount_valid(1_000_000));
        assert!(!OperationType::Sale.is_amount_valid(0));
        assert!(!OperationType::Sale.is_amount_valid(-1));
    }

    #[test]
    fn refund_requires_negative_amount() {
        assert!(OperationType::Refund.is_amount_valid(-1));
        assert!(OperationType::Refund.is_amount_valid(-1_000));
        assert!(!OperationType::Refund.is_amount_valid(0));
        assert!(!OperationType::Refund.is_amount_valid(100));
    }

    #[test]
    fn cancel_requires_negative_amount() {
        assert!(OperationType::Cancel.is_amount_valid(-500));
        assert!(!OperationType::Cancel.is_amount_valid(0));
        assert!(!OperationType::Cancel.is_amount_valid(500));
    }

    #[test]
    fn discount_requires_negative_amount() {
        assert!(OperationType::Discount.is_amount_valid(-10));
        assert!(!OperationType::Discount.is_amount_valid(0));
        assert!(!OperationType::Discount.is_amount_valid(10));
    }

    #[test]
    fn zclose_requires_zero_amount() {
        assert!(OperationType::ZClose.is_amount_valid(0));
        assert!(!OperationType::ZClose.is_amount_valid(1));
        assert!(!OperationType::ZClose.is_amount_valid(-1));
    }

    // --- label / Display ---

    #[test]
    fn labels_are_correct() {
        assert_eq!(OperationType::Sale.label(), "VENTE");
        assert_eq!(OperationType::Refund.label(), "REMBOURSEMENT");
        assert_eq!(OperationType::Cancel.label(), "ANNULATION");
        assert_eq!(OperationType::Discount.label(), "REMISE");
        assert_eq!(OperationType::ZClose.label(), "CLOTURE_Z");
    }

    #[test]
    fn display_matches_label() {
        assert_eq!(OperationType::Sale.to_string(), "VENTE");
    }

    // --- expected_amount_sign ---

    #[test]
    fn expected_signs() {
        assert_eq!(OperationType::Sale.expected_amount_sign(), 1);
        assert_eq!(OperationType::Refund.expected_amount_sign(), -1);
        assert_eq!(OperationType::Cancel.expected_amount_sign(), -1);
        assert_eq!(OperationType::Discount.expected_amount_sign(), -1);
        assert_eq!(OperationType::ZClose.expected_amount_sign(), 0);
    }
}
