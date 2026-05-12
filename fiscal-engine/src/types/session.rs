//! # session
//!
//! Session de caisse : période entre l'ouverture et la clôture (rapport Z).
//!
//! ## Cycle de vie NF525
//! ```text
//! ┌──────────┐   open_session()   ┌────────┐
//! │  (none)  │ ─────────────────► │  Open  │
//! └──────────┘                    └────────┘
//!                                      │
//!                              close_session()
//!                                      │
//!                                      ▼
//!                               ┌──────────┐
//!                               │  Closed  │ (immuable)
//!                               └──────────┘
//! ```
//!
//! ## Contraintes NF525
//! - Une seule session ouverte à la fois (§4.1)
//! - Une session fermée ne peut pas être rouverte (§4.1)
//! - Le rapport Z est généré au moment de la clôture (§6)

use common::{Cents, SessionId};
use serde::{Deserialize, Serialize};

/// État d'une session de caisse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SessionStatus {
    /// Session ouverte : les opérations sont acceptées.
    Open,
    /// Session clôturée : lecture seule, rapport Z généré.
    Closed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Open => write!(f, "OUVERTE"),
            Self::Closed => write!(f, "CLOTUREE"),
        }
    }
}

/// Session de caisse.
///
/// Regroupe toutes les entrées fiscales entre l'ouverture et la clôture.
/// Les totaux (`total_sales_cents`, etc.) sont des accumulateurs mis à jour
/// à chaque `record_transaction()` — ils permettent de valider le rapport Z
/// sans relire tout le journal.
///
/// # Invariant
/// `total_sales_cents >= 0` et `total_refunds_cents >= 0` (valeur absolue).
/// Le solde net = `total_sales_cents - total_refunds_cents - total_discounts_cents`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Identifiant unique de la session.
    pub id: SessionId,

    /// Numéro de séquence de la session (commence à 1, incrémenté à chaque ouverture).
    pub sequence_number: u64,

    /// État de la session.
    pub status: SessionStatus,

    /// Timestamp d'ouverture en millisecondes Unix (UTC).
    pub opened_at_ms: u64,

    /// Timestamp de clôture en millisecondes Unix (UTC).
    /// `None` si la session est encore ouverte.
    pub closed_at_ms: Option<u64>,

    /// Hash de clôture (dernière entrée de la session, type `ZClose`).
    /// `None` si la session est encore ouverte.
    pub closing_hash: Option<[u8; 32]>,

    // --- Accumulateurs de totaux (mis à jour en temps réel) ---
    /// Somme des ventes TTC en centimes (positif).
    pub total_sales_cents: Cents,

    /// Somme des remboursements en centimes (positif, valeur absolue).
    pub total_refunds_cents: Cents,

    /// Somme des annulations en centimes (positif, valeur absolue).
    pub total_cancels_cents: Cents,

    /// Somme des remises en centimes (positif, valeur absolue).
    pub total_discounts_cents: Cents,

    /// Nombre d'entrées dans la session (toutes opérations confondues).
    pub entry_count: u64,
}

impl Session {
    /// Crée une nouvelle session ouverte.
    ///
    /// # Arguments
    /// * `sequence_number` - Numéro de séquence de cette session.
    /// * `opened_at_ms` - Timestamp d'ouverture en millisecondes Unix.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::session::{Session, SessionStatus};
    ///
    /// let session = Session::new(1, 1_700_000_000_000);
    /// assert_eq!(session.status, SessionStatus::Open);
    /// assert_eq!(session.sequence_number, 1);
    /// assert!(session.closed_at_ms.is_none());
    /// ```
    #[must_use]
    pub fn new(sequence_number: u64, opened_at_ms: u64) -> Self {
        Self {
            id: SessionId::new(),
            sequence_number,
            status: SessionStatus::Open,
            opened_at_ms,
            closed_at_ms: None,
            closing_hash: None,
            total_sales_cents: Cents::ZERO,
            total_refunds_cents: Cents::ZERO,
            total_cancels_cents: Cents::ZERO,
            total_discounts_cents: Cents::ZERO,
            entry_count: 0,
        }
    }

    /// Vérifie si la session est ouverte.
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::session::Session;
    ///
    /// let session = Session::new(1, 1_700_000_000_000);
    /// assert!(session.is_open());
    /// ```
    #[must_use]
    pub fn is_open(&self) -> bool {
        self.status == SessionStatus::Open
    }

    /// Calcule le chiffre d'affaires net de la session.
    ///
    /// `CA net = ventes - remboursements - annulations - remises`
    ///
    /// # Examples
    /// ```
    /// use fiscal_engine::types::session::Session;
    /// use common::Cents;
    ///
    /// let mut session = Session::new(1, 0);
    /// session.total_sales_cents = Cents(10_000);
    /// session.total_refunds_cents = Cents(500);
    /// assert_eq!(session.net_revenue(), Cents(9_500));
    /// ```
    #[must_use]
    pub fn net_revenue(&self) -> Cents {
        self.total_sales_cents
            - self.total_refunds_cents
            - self.total_cancels_cents
            - self.total_discounts_cents
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn open_session() -> Session {
        Session::new(1, 1_700_000_000_000)
    }

    // --- Cas nominaux ---

    #[test]
    fn new_session_is_open() {
        let s = open_session();
        assert_eq!(s.status, SessionStatus::Open);
        assert!(s.is_open());
        assert!(s.closed_at_ms.is_none());
        assert!(s.closing_hash.is_none());
    }

    #[test]
    fn new_session_has_zero_totals() {
        let s = open_session();
        assert_eq!(s.total_sales_cents, Cents::ZERO);
        assert_eq!(s.total_refunds_cents, Cents::ZERO);
        assert_eq!(s.total_cancels_cents, Cents::ZERO);
        assert_eq!(s.total_discounts_cents, Cents::ZERO);
        assert_eq!(s.entry_count, 0);
    }

    #[test]
    fn net_revenue_calculation() {
        let mut s = open_session();
        s.total_sales_cents = Cents(10_000);
        s.total_refunds_cents = Cents(500);
        s.total_cancels_cents = Cents(200);
        s.total_discounts_cents = Cents(100);
        // 100 - 5 - 2 - 1 = 92 €
        assert_eq!(s.net_revenue(), Cents(9_200));
    }

    // --- Cas limite ---

    #[test]
    fn net_revenue_zero_session() {
        let s = open_session();
        assert_eq!(s.net_revenue(), Cents::ZERO);
    }

    #[test]
    fn net_revenue_only_refunds_is_negative() {
        let mut s = open_session();
        s.total_refunds_cents = Cents(500);
        // Session avec remboursements sans ventes préalables (cas d'erreur opérationnelle,
        // mais arithmétiquement valide — le journal l'aura de toute façon rejeté)
        assert_eq!(s.net_revenue(), Cents(-500));
    }

    // --- SessionStatus Display ---

    #[test]
    fn session_status_display() {
        assert_eq!(SessionStatus::Open.to_string(), "OUVERTE");
        assert_eq!(SessionStatus::Closed.to_string(), "CLOTUREE");
    }

    #[test]
    fn session_sequence_number_stored() {
        let s = Session::new(42, 0);
        assert_eq!(s.sequence_number, 42);
    }
}
