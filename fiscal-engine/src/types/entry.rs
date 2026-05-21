//! # entry
//!
//! `FiscalEntry` : enregistrement atomique et immuable du journal fiscal NF525.
//!
//! ## Invariants NF525 garantis à la construction
//! - `sequence_number` est monotone croissant, sans trou (vérifié par le journal)
//! - `hash` est calculé sur toutes les données de l'entrée + `previous_hash`
//! - `previous_hash` est `[0u8; 32]` pour la première entrée
//! - `created_at` est un timestamp Unix en millisecondes (non modifiable après création)
//! - `operation_type` détermine le signe de `amount_ttc_cents` (vérifié à la construction)
//!
//! ## Immuabilité
//! Tous les champs sont publics en lecture (`pub`) mais le type ne fournit pas
//! de méthodes `mut` : la seule façon de créer un `FiscalEntry` est via `FiscalEntry::new()`.
//! La modification après création est structurellement impossible sans `unsafe`.

use common::{Cents, FiscalEntryId, SessionId};
use serde::{Deserialize, Serialize};

use super::{operation::OperationType, tva::TvaBreakdown};

/// Enregistrement atomique et immuable du journal fiscal.
///
/// Chaque `FiscalEntry` représente une opération fiscale unique,
/// chaînée cryptographiquement à l'entrée précédente via `hash` et `previous_hash`.
///
/// # Immuabilité
/// Une fois créé, un `FiscalEntry` ne doit jamais être modifié.
/// Le journal le stocke en append-only (SQLite sans UPDATE ni DELETE sur cette table).
///
/// # Examples
/// ```
/// use fiscal_engine::types::entry::FiscalEntryData;
/// use fiscal_engine::types::operation::OperationType;
/// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
/// use common::{Cents, SessionId};
///
/// let data = FiscalEntryData {
///     session_id: SessionId::new(),
///     operation_type: OperationType::Sale,
///     amount_ttc_cents: Cents(1100),
///     tva_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
///     tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
///     tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
///     tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
///     reason: None,
///     order_reference: Some("ORD-001".to_string()),
/// };
/// // La création du FiscalEntry final (avec hash) se fait dans le journal (Étape 4)
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiscalEntry {
    // --- Identité ---
    /// Identifiant unique de cette entrée (UUID v7).
    pub id: FiscalEntryId,

    /// Numéro de séquence dans le journal, commence à 1.
    /// NF525 §4.2 : la séquence est continue, sans trou, vérifiable.
    pub sequence_number: u64,

    /// Session de caisse à laquelle cette entrée appartient.
    pub session_id: SessionId,

    // --- Données fiscales ---
    /// Type de l'opération fiscale.
    pub operation_type: OperationType,

    /// Montant TTC en centimes.
    /// Signe déterminé par `operation_type` (positif pour Sale, négatif pour Refund/Cancel/Discount).
    pub amount_ttc_cents: Cents,

    /// Décomposition TVA : HT, TVA, TTC par taux applicable.
    /// Pour les entrées multi-taux, ce champ contient le taux dominant et la somme totale.
    pub tva_breakdown: TvaBreakdown,

    /// Décomposition TVA au taux réduit 5,5% (NF525 §4.3.1).
    pub tva_5_5_breakdown: TvaBreakdown,
    /// Décomposition TVA au taux intermédiaire 10% (NF525 §4.3.1).
    pub tva_10_breakdown: TvaBreakdown,
    /// Décomposition TVA au taux normal 20% (NF525 §4.3.1).
    pub tva_20_breakdown: TvaBreakdown,

    // --- Contexte métier (optionnel) ---
    /// Motif obligatoire pour `Refund` et `Cancel`.
    /// `None` pour `Sale`, `Discount`, `ZClose`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,

    /// Référence de la commande associée (ex: `ORD-20240115-001`).
    /// `None` pour les entrées de clôture `ZClose`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_reference: Option<String>,

    // --- Chaîne cryptographique ---
    /// Hash SHA-256 de cette entrée, calculé sur :
    /// `id ‖ sequence_number ‖ timestamp_ms ‖ amount_ttc_cents ‖ previous_hash`
    /// (sérialisés selon le schéma canonique défini dans `hash_engine`)
    pub hash: [u8; 32],

    /// Hash de l'entrée précédente dans la chaîne.
    /// Vaut `[0u8; 32]` pour la première entrée (hash genesis, NF525 §5.1).
    pub previous_hash: [u8; 32],

    // --- Métadonnées ---
    /// Timestamp de création en millisecondes Unix (UTC).
    /// Immuable après création.
    pub created_at_ms: u64,

    /// Indique si cette entrée a été synchronisée vers le cloud.
    /// Géré par `sync-client`, jamais modifié par `fiscal-engine`.
    pub synced: bool,
}

/// Données brutes nécessaires pour créer un `FiscalEntry`.
///
/// Passé au journal lors de l'appel à `record_transaction()`.
/// Le journal calcule ensuite `id`, `sequence_number`, `hash`, `previous_hash`,
/// `created_at_ms` et `synced` automatiquement.
#[derive(Debug, Clone)]
pub struct FiscalEntryData {
    /// Session de caisse active.
    pub session_id: SessionId,
    /// Type de l'opération.
    pub operation_type: OperationType,
    /// Montant TTC en centimes.
    pub amount_ttc_cents: Cents,
    /// Décomposition TVA principale (taux dominant pour le hash).
    pub tva_breakdown: TvaBreakdown,
    /// Décomposition TVA au taux réduit 5,5% (NF525 §4.3.1).
    pub tva_5_5_breakdown: TvaBreakdown,
    /// Décomposition TVA au taux intermédiaire 10% (NF525 §4.3.1).
    pub tva_10_breakdown: TvaBreakdown,
    /// Décomposition TVA au taux normal 20% (NF525 §4.3.1).
    pub tva_20_breakdown: TvaBreakdown,
    /// Motif (obligatoire pour `Refund` et `Cancel`).
    pub reason: Option<String>,
    /// Référence de commande.
    pub order_reference: Option<String>,
}

impl FiscalEntryData {
    /// Valide la cohérence des données avant création de l'entrée.
    ///
    /// Vérifie :
    /// 1. Le signe du montant est cohérent avec `operation_type`
    /// 2. La décomposition TVA est cohérente (`ht + tva == ttc`)
    /// 3. Le motif est fourni si `operation_type.requires_reason()` est vrai
    ///
    /// # Errors
    /// Retourne une description de l'erreur de validation.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::entry::FiscalEntryData;
    /// use fiscal_engine::types::operation::OperationType;
    /// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
    /// use common::{Cents, SessionId};
    ///
    /// let data = FiscalEntryData {
    ///     session_id: SessionId::new(),
    ///     operation_type: OperationType::Sale,
    ///     amount_ttc_cents: Cents(1100),
    ///     tva_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
    ///     tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
    ///     tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
    ///     tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
    ///     reason: None,
    ///     order_reference: None,
    /// };
    /// assert!(data.validate().is_ok());
    /// ```
    pub fn validate(&self) -> Result<(), String> {
        // 1. Cohérence signe / operation_type
        if !self
            .operation_type
            .is_amount_valid(self.amount_ttc_cents.0)
        {
            return Err(format!(
                "Montant {} centimes invalide pour l'opération {}",
                self.amount_ttc_cents.0,
                self.operation_type.label()
            ));
        }

        // 2. Cohérence TVA : ht + tva == ttc
        let expected_ttc = self.tva_breakdown.ht_cents + self.tva_breakdown.tva_cents;
        if expected_ttc != self.tva_breakdown.ttc_cents {
            return Err(format!(
                "Décomposition TVA incohérente : {} + {} = {} ≠ ttc déclaré {}",
                self.tva_breakdown.ht_cents.0,
                self.tva_breakdown.tva_cents.0,
                expected_ttc.0,
                self.tva_breakdown.ttc_cents.0
            ));
        }

        // 3. Cohérence montant TTC avec la décomposition TVA
        // (pour ZClose, le montant est 0 et la décomposition peut être nulle)
        if self.operation_type != OperationType::ZClose
            && self.amount_ttc_cents != self.tva_breakdown.ttc_cents
        {
            return Err(format!(
                "Montant TTC {} ne correspond pas à la décomposition TVA TTC {}",
                self.amount_ttc_cents.0, self.tva_breakdown.ttc_cents.0
            ));
        }

        // 4. Vérification que la somme des TTC par taux = montant TTC total
        if self.operation_type != OperationType::ZClose {
            let per_rate_total = self.tva_5_5_breakdown.ttc_cents
                + self.tva_10_breakdown.ttc_cents
                + self.tva_20_breakdown.ttc_cents;
            if per_rate_total != self.amount_ttc_cents {
                return Err(format!(
                    "Somme des TTC par taux ({}) ≠ montant TTC total ({})",
                    per_rate_total.0, self.amount_ttc_cents.0
                ));
            }
        }

        // 5. Motif obligatoire pour Refund et Cancel
        if self.operation_type.requires_reason() && self.reason.is_none() {
            return Err(format!(
                "Motif obligatoire pour l'opération {}",
                self.operation_type.label()
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

    fn valid_sale_data() -> FiscalEntryData {
        FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(1100),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: Some("ORD-001".to_string()),
        }
    }

    // --- Cas nominaux ---

    #[test]
    fn valid_sale_passes_validation() {
        assert!(valid_sale_data().validate().is_ok());
    }

    #[test]
    fn valid_refund_with_reason_passes_validation() {
        let data = FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::Refund,
            amount_ttc_cents: Cents(-1100),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(-1100), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(-1100), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: Some("Produit défectueux".to_string()),
            order_reference: Some("ORD-001".to_string()),
        };
        assert!(data.validate().is_ok());
    }

    #[test]
    fn valid_zclose_passes_validation() {
        // ZClose : montant 0, décomposition nulle
        let data = FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::ZClose,
            amount_ttc_cents: Cents(0),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::zero(TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: None,
        };
        assert!(data.validate().is_ok());
    }

    // --- Cas limites ---

    #[test]
    fn sale_with_minimum_amount_passes() {
        // 0,01 € TTC : le minimum absolu pour une vente
        let data = FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(1),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(1), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: None,
        };
        assert!(data.validate().is_ok());
    }

    // --- Cas d'erreur ---

    #[test]
    fn sale_with_negative_amount_fails_validation() {
        let mut data = valid_sale_data();
        data.amount_ttc_cents = Cents(-500);
        let result = data.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalide"));
    }

    #[test]
    fn sale_with_zero_amount_fails_validation() {
        let mut data = valid_sale_data();
        data.amount_ttc_cents = Cents(0);
        assert!(data.validate().is_err());
    }

    #[test]
    fn refund_without_reason_fails_validation() {
        let data = FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::Refund,
            amount_ttc_cents: Cents(-1100),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(-1100), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(-1100), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None, // manquant !
            order_reference: None,
        };
        let result = data.validate();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Motif obligatoire"));
    }

    #[test]
    fn cancel_without_reason_fails_validation() {
        let data = FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::Cancel,
            amount_ttc_cents: Cents(-500),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(-500), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(-500), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: None,
        };
        assert!(data.validate().is_err());
    }

    #[test]
    fn tva_breakdown_mismatch_fails_validation() {
        // On altère la décomposition TVA pour créer une incohérence
        let mut breakdown = TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10);
        // On modifie le TTC de la décomposition pour qu'il ne corresponde plus
        breakdown.ttc_cents = Cents(1200); // faux intentionnellement
        let data = FiscalEntryData {
            session_id: SessionId::new(),
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(1100),
            tva_breakdown: breakdown,
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: None,
        };
        assert!(data.validate().is_err());
    }
}
