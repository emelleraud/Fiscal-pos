//! # common
//!
//! Types et constantes partagés entre toutes les crates du workspace `pos-fiscal`.
//!
//! Ce crate ne contient **aucune logique métier**. Il expose uniquement :
//! - Des newtypes d'identifiants fortement typés (évite les confusions `OrderId`/`SessionId`)
//! - Des constantes réglementaires (taux TVA, limites NF525)
//! - Une interface d'erreur commune pour la sérialisation des erreurs en JSON API
//!
//! ## Philosophie
//! Tout type utilisé par plus d'une crate vit ici. Évite les dépendances circulaires
//! et garantit une source unique de vérité pour les identifiants et constantes.

#![deny(clippy::all, clippy::pedantic)]
#![warn(missing_docs)]

use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Identifiants fortement typés
// ---------------------------------------------------------------------------

/// Identifiant unique d'une transaction fiscale (UUID v7, ordre temporel garanti).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FiscalEntryId(pub Uuid);

impl FiscalEntryId {
    /// Génère un nouvel identifiant fiscal unique.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for FiscalEntryId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for FiscalEntryId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifiant unique d'une session de caisse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SessionId(pub Uuid);

impl SessionId {
    /// Génère un nouvel identifiant de session.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Identifiant unique d'une commande (order).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct OrderId(pub Uuid);

impl OrderId {
    /// Génère un nouvel identifiant de commande.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }
}

impl Default for OrderId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OrderId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

// ---------------------------------------------------------------------------
// Constantes réglementaires
// ---------------------------------------------------------------------------

/// Taux de TVA réduit (alimentation sur place, catégorie restauration rapide).
/// Applicable aux ventes de repas consommés sur place depuis 2012.
pub const TVA_RATE_REDUCED: f64 = 0.055; // 5,5 %

/// Taux de TVA intermédiaire (boissons alcoolisées, ventes à emporter).
pub const TVA_RATE_INTERMEDIATE: f64 = 0.10; // 10 %

/// Taux de TVA normal (alcool fort, produits hors restauration).
pub const TVA_RATE_NORMAL: f64 = 0.20; // 20 %

/// Séquence de départ pour le numéro de séquence fiscal (NF525 §4.2 : commence à 1).
pub const FISCAL_SEQUENCE_START: u64 = 1;

/// Hash initial (entrée zéro) de la chaîne fiscale : 32 octets à zéro.
/// NF525 §5.1 : le premier enregistrement utilise un vecteur d'initialisation nul.
pub const GENESIS_HASH: [u8; 32] = [0u8; 32];

// ---------------------------------------------------------------------------
// Représentation monétaire
// ---------------------------------------------------------------------------

/// Montant monétaire en centimes d'euro (entier, pas de virgule flottante).
///
/// Utiliser des centimes évite les erreurs d'arrondi IEEE 754 sur les calculs fiscaux.
/// Toute conversion depuis/vers f64 doit être explicite et arrondie avec `round()`.
///
/// # Invariant
/// La valeur est toujours positive ou nulle pour les ventes ; négative pour les remboursements.
///
/// # Examples
/// ```
/// use common::Cents;
/// let price = Cents::from_euros(12.50);
/// assert_eq!(price, Cents(1250));
/// assert_eq!(price.to_string(), "12.50 €");
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Cents(pub i64);

impl Cents {
    /// Zéro centime.
    pub const ZERO: Self = Self(0);

    /// Crée un montant depuis des euros (arrondi au centime le plus proche).
    ///
    /// # Examples
    /// ```
    /// use common::Cents;
    /// let price = Cents::from_euros(12.50);
    /// assert_eq!(price, Cents(1250));
    /// ```
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub fn from_euros(euros: f64) -> Self {
        Self((euros * 100.0).round() as i64)
    }

    /// Convertit en euros (f64). À utiliser uniquement pour l'affichage.
    #[must_use]
    #[allow(clippy::cast_precision_loss)] // affichage uniquement — perte de précision acceptable
    pub fn to_euros(self) -> f64 {
        self.0 as f64 / 100.0
    }

    /// Vérifie que le montant est positif ou nul.
    #[must_use]
    pub fn is_non_negative(self) -> bool {
        self.0 >= 0
    }
}

impl std::fmt::Display for Cents {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:.2} €", self.to_euros())
    }
}

impl std::ops::Add for Cents {
    type Output = Self;
    fn add(self, rhs: Self) -> Self::Output {
        Self(self.0 + rhs.0)
    }
}

impl std::ops::Sub for Cents {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self::Output {
        Self(self.0 - rhs.0)
    }
}

impl std::ops::Neg for Cents {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(-self.0)
    }
}

// ---------------------------------------------------------------------------
// Erreur API commune (sérialisation JSON)
// ---------------------------------------------------------------------------

/// Enveloppe d'erreur standard pour toutes les réponses d'erreur de l'`edge-api`.
///
/// Toutes les routes retournent cette structure en cas d'erreur, avec le code HTTP approprié.
/// Cela garantit un contrat d'API stable pour le frontend React.
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    /// Code d'erreur machine-readable (ex: `FISCAL_INTEGRITY_FAILURE`).
    pub code: String,
    /// Message d'erreur lisible par un développeur (jamais exposé à l'utilisateur final).
    pub message: String,
    /// Contexte additionnel optionnel (trace ID, champ invalide, etc.).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<serde_json::Value>,
}

impl ApiError {
    /// Construit une erreur API avec un code et un message.
    ///
    /// # Examples
    /// ```
    /// use common::ApiError;
    /// let err = ApiError::new("FISCAL_INTEGRITY_FAILURE", "Hash chain broken at sequence 42");
    /// assert_eq!(err.code, "FISCAL_INTEGRITY_FAILURE");
    /// ```
    #[must_use]
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            detail: None,
        }
    }

    /// Ajoute un contexte JSON à l'erreur.
    #[must_use]
    pub fn with_detail(mut self, detail: serde_json::Value) -> Self {
        self.detail = Some(detail);
        self
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cents_from_euros_exact() {
        assert_eq!(Cents::from_euros(12.50), Cents(1250));
        assert_eq!(Cents::from_euros(0.01), Cents(1));
        assert_eq!(Cents::from_euros(0.0), Cents(0));
    }

    #[test]
    fn cents_from_euros_rounding() {
        let c = Cents::from_euros(1.005);
        // IEEE 754 : 1.005 * 100 = 100.499... donc round() = 100, pas 101
        assert_eq!(c, Cents(100));
        // Avec 2 décimales exactes, pas d'ambiguïté
        assert_eq!(Cents::from_euros(1.01), Cents(101));
    }

    #[test]
    fn cents_display() {
        assert_eq!(Cents(1250).to_string(), "12.50 €");
        assert_eq!(Cents(0).to_string(), "0.00 €");
    }

    #[test]
    fn cents_arithmetic() {
        assert_eq!(Cents(500) + Cents(300), Cents(800));
        assert_eq!(Cents(500) - Cents(300), Cents(200));
        assert_eq!(-Cents(500), Cents(-500));
    }

    #[test]
    fn fiscal_entry_id_uniqueness() {
        let id1 = FiscalEntryId::new();
        let id2 = FiscalEntryId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn genesis_hash_is_zeroes() {
        assert_eq!(GENESIS_HASH, [0u8; 32]);
    }

    #[test]
    fn api_error_construction() {
        let err = ApiError::new("TEST_ERROR", "message de test");
        assert_eq!(err.code, "TEST_ERROR");
        assert!(err.detail.is_none());

        let err_with_detail = err.with_detail(serde_json::json!({ "field": "sequence" }));
        assert!(err_with_detail.detail.is_some());
    }
}
