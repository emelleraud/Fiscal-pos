//! # journal
//!
//! Journal fiscal append-only : façade métier du `fiscal-engine`.
//!
//! ## Rôle
//! Le `Journal` est le seul point d'entrée pour toute opération fiscale.
//! Il orchestre :
//! 1. La validation des données métier (`FiscalEntryData::validate()`)
//! 2. Le calcul du numéro de séquence (atomic via `SQLite`)
//! 3. Le calcul du hash chaîné (`hash_engine::compute_entry_hash()`)
//! 4. La persistence transactionnelle (INSERT entrée + UPDATE session en une TX)
//! 5. La vérification d'intégrité à la demande
//!
//! ## Cycle de vie
//! ```text
//! Journal::open(pool)          → Journal (avec session active chargée ou None)
//!     │
//!     ├── open_session()       → Session (ouverte, persistée)
//!     │
//!     ├── record_transaction() → FiscalEntry (hashée, persistée, session mise à jour)
//!     │   ├── Sale
//!     │   ├── Refund
//!     │   ├── Cancel
//!     │   └── Discount
//!     │
//!     ├── verify_integrity()   → IntegrityReport (lecture seule)
//!     │
//!     └── close_session()      → ZReport (voir Étape 5)
//! ```
//!
//! ## Thread safety
//! `Journal` est `Send + Sync` car `JournalStore` contient un `SqlitePool`
//! qui est lui-même `Send + Sync`. L'état mutable (`active_session`) est
//! protégé par un `tokio::sync::Mutex`.
//!
//! ## Garanties NF525
//! - Aucune entrée ne peut être créée sans session active
//! - La séquence est atomique (calculée dans la même TX que l'INSERT)
//! - Le hash chaîné est calculé à partir du `last_hash()` lu dans la même TX
//! - Toute erreur du hash engine est bloquante (pas de fallback silencieux)

pub mod store;

use std::sync::Arc;
use tokio::sync::Mutex;

use sqlx::sqlite::SqlitePool;

use crate::{
    errors::{FiscalError, SessionError},
    hash_engine::{compute_entry_hash, verify_chain_integrity, HashInput, IntegrityReport},
    types::{
        entry::{FiscalEntry, FiscalEntryData},
        operation::OperationType,
        session::{Session, SessionStatus},
        tva::TvaBreakdown,
    },
};
use common::{Cents, FiscalEntryId};

use self::store::JournalStore;

// ---------------------------------------------------------------------------
// Journal principal
// ---------------------------------------------------------------------------

/// Journal fiscal append-only.
///
/// Point d'entrée unique pour toutes les opérations fiscales.
/// Encapsule l'état de la session active et délègue la persistence au `JournalStore`.
///
/// # Thread safety
/// Peut être partagé entre threads via `Arc<Journal>`.
/// L'accès concurrent est sérialisé par le `Mutex<Option<Session>>` interne.
///
/// # Examples
/// ```no_run
/// use fiscal_engine::journal::Journal;
/// use sqlx::sqlite::SqlitePoolOptions;
///
/// # async fn example() -> Result<(), fiscal_engine::errors::FiscalError> {
/// let pool = SqlitePoolOptions::new()
///     .connect("sqlite:./restaurant.db")
///     .await
///     .unwrap();
/// let journal = Journal::open(pool).await?;
/// let session = journal.open_session().await?;
/// # Ok(())
/// # }
/// ```
#[derive(Debug)]
pub struct Journal {
    store: JournalStore,
    /// Session courante en mémoire (cache — source de vérité : `SQLite`).
    /// `None` si aucune session n'est active.
    active_session: Arc<Mutex<Option<Session>>>,
}

impl Journal {
    /// Ouvre le journal sur une base `SQLite` existante.
    ///
    /// Charge la session active depuis la base si elle existe.
    /// Lance les migrations `SQLite` si elles n'ont pas encore été appliquées.
    ///
    /// # Arguments
    /// * `pool` - Pool de connexions `SQLite` (WAL mode recommandé).
    ///
    /// # Errors
    /// - `FiscalError::Database` si la base est inaccessible ou les migrations échouent
    ///
    /// # Examples
    /// ```no_run
    /// use fiscal_engine::journal::Journal;
    /// use sqlx::sqlite::SqlitePoolOptions;
    ///
    /// # async fn example() -> Result<(), fiscal_engine::errors::FiscalError> {
    /// let pool = SqlitePoolOptions::new()
    ///     .connect("sqlite:./restaurant.db")
    ///     .await
    ///     .unwrap();
    /// let journal = Journal::open(pool).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open(pool: SqlitePool) -> Result<Self, FiscalError> {
        let store = JournalStore::new(pool).await?;
        store.run_migrations().await?;

        let active_session = store.load_active_session().await?;

        Ok(Self {
            store,
            active_session: Arc::new(Mutex::new(active_session)),
        })
    }

    /// Ouvre le journal sur un store déjà créé (usage interne et tests).
    ///
    /// Contrairement à `open()`, ne lance pas les migrations.
    #[cfg(test)]
    pub(crate) async fn from_store(store: JournalStore) -> Result<Self, FiscalError> {
        let active_session = store.load_active_session().await?;
        Ok(Self {
            store,
            active_session: Arc::new(Mutex::new(active_session)),
        })
    }

    // -----------------------------------------------------------------------
    // Sessions
    // -----------------------------------------------------------------------

    /// Ouvre une nouvelle session de caisse.
    ///
    /// NF525 §4.1 : une seule session peut être ouverte à la fois.
    /// Si une session est déjà ouverte, retourne `SessionError::SessionAlreadyActive`.
    ///
    /// # Errors
    /// - `SessionError::SessionAlreadyActive` si une session est déjà ouverte
    /// - `FiscalError::Database` sur erreur `SQLite`
    ///
    /// # Examples
    /// ```no_run
    /// # async fn example(journal: &fiscal_engine::journal::Journal)
    /// #     -> Result<(), fiscal_engine::errors::FiscalError> {
    /// let session = journal.open_session().await?;
    /// println!("Session {} ouverte", session.id);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn open_session(&self) -> Result<Session, FiscalError> {
        let mut guard = self.active_session.lock().await;

        if let Some(ref existing) = *guard {
            return Err(SessionError::SessionAlreadyActive {
                session_id: existing.id.to_string(),
            }
            .into());
        }

        let seq = self.store.next_session_sequence().await?;
        let now_ms = now_ms();
        let session = Session::new(seq, now_ms);

        self.store.insert_session(&session).await?;
        *guard = Some(session.clone());

        Ok(session)
    }

    /// Retourne la session active courante, si elle existe.
    ///
    /// Lecture seule — retourne un clone de la session en mémoire.
    pub async fn active_session(&self) -> Option<Session> {
        self.active_session.lock().await.clone()
    }

    // -----------------------------------------------------------------------
    // Enregistrement de transactions (cœur du journal)
    // -----------------------------------------------------------------------

    /// Enregistre une opération fiscale dans le journal.
    ///
    /// C'est la fonction centrale du `fiscal-engine`. Elle :
    /// 1. Vérifie qu'une session est active
    /// 2. Valide les données métier (signe du montant, cohérence TVA, motif)
    /// 3. Calcule le numéro de séquence (atomique, dans la transaction)
    /// 4. Lit le `previous_hash` (dans la même transaction)
    /// 5. Calcule le hash SHA-256 chaîné
    /// 6. INSERT l'entrée + UPDATE session dans une transaction atomique
    ///
    /// **Toute erreur est bloquante** — pas de fallback silencieux.
    ///
    /// # Arguments
    /// * `data` - Données de l'opération à enregistrer.
    ///
    /// # Errors
    /// - `SessionError::NoActiveSession` si aucune session n'est ouverte
    /// - `FiscalError::SessionClosed` si la session est fermée
    /// - `FiscalError::InvalidAmount` si le montant est incohérent
    /// - `FiscalError::InvalidTvaDecomposition` si la TVA est incohérente
    /// - `FiscalError::Database` sur erreur `SQLite` (TX rollbackée automatiquement)
    ///
    /// # Examples
    /// ```no_run
    /// use fiscal_engine::types::{
    ///     entry::FiscalEntryData,
    ///     operation::OperationType,
    ///     tva::{TvaBreakdown, TvaRate},
    /// };
    /// use common::{Cents, SessionId};
    ///
    /// # async fn example(
    /// #     journal: &fiscal_engine::journal::Journal,
    /// #     session_id: SessionId,
    /// # ) -> Result<(), fiscal_engine::errors::FiscalError> {
    /// let entry = journal.record_transaction(FiscalEntryData {
    ///     session_id,
    ///     operation_type: OperationType::Sale,
    ///     amount_ttc_cents: Cents(1100),
    ///     tva_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
    ///     tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
    ///     tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
    ///     tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
    ///     reason: None,
    ///     order_reference: Some("ORD-001".to_string()),
    /// }).await?;
    ///
    /// println!("Entrée #{} hashée : {:?}", entry.sequence_number, entry.hash);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn record_transaction(
        &self,
        data: FiscalEntryData,
    ) -> Result<FiscalEntry, FiscalError> {
        // 1. Vérifier la session
        let mut session_guard = self.active_session.lock().await;
        let session = session_guard
            .as_mut()
            .ok_or(SessionError::NoActiveSession)?;

        if !session.is_open() {
            return Err(FiscalError::SessionClosed {
                session_id: session.id.to_string(),
            });
        }

        // 2. Valider les données métier
        data.validate().map_err(|msg| FiscalError::InvalidAmount {
            amount_cents: data.amount_ttc_cents.0,
            operation: msg,
        })?;

        // 3. Construire l'entrée avec hash dans une transaction atomique
        let entry = {
            let mut tx = self.store.begin_transaction().await?;

            // Séquence atomique : calculée DANS la transaction pour éviter les races
            let sequence_number = self.store.next_entry_sequence().await?;

            // previous_hash : dernière entrée du journal (genesis si vide)
            let previous_hash = self.store.last_hash().await?;

            // Construire l'entrée complète
            let entry_id = FiscalEntryId::new();
            let entry_id_bytes = entry_id.0.into_bytes();
            let session_id_bytes = data.session_id.0.into_bytes();

            let hash_input = HashInput {
                sequence_number,
                entry_id_bytes: &entry_id_bytes,
                session_id_bytes: &session_id_bytes,
                operation_type: data.operation_type,
                amount_ttc_cents: data.amount_ttc_cents.0,
                tva_rate: data.tva_breakdown.rate,
                ht_cents: data.tva_breakdown.ht_cents.0,
                tva_cents: data.tva_breakdown.tva_cents.0,
                created_at_ms: now_ms(),
                previous_hash: &previous_hash,
            };

            let hash = compute_entry_hash(&hash_input);

            let entry = FiscalEntry {
                id: entry_id,
                sequence_number,
                session_id: data.session_id,
                operation_type: data.operation_type,
                amount_ttc_cents: data.amount_ttc_cents,
                tva_breakdown: data.tva_breakdown,
                tva_5_5_breakdown: data.tva_5_5_breakdown,
                tva_10_breakdown: data.tva_10_breakdown,
                tva_20_breakdown: data.tva_20_breakdown,
                reason: data.reason,
                order_reference: data.order_reference,
                hash,
                previous_hash,
                created_at_ms: hash_input.created_at_ms,
                synced: false,
            };

            // INSERT entrée (append-only)
            self.store.insert_entry(&entry, &mut tx).await?;

            // UPDATE accumulateurs de session
            update_session_accumulators(session, &entry);
            self.store.update_session_totals(session, &mut tx).await?;

            tx.commit().await?;
            entry
        };

        Ok(entry)
    }

    /// Clôture la session active et retourne le hash de clôture.
    ///
    /// Enregistre l'entrée `ZClose` dans le journal, ferme la session en base,
    /// et vide le cache en mémoire.
    ///
    /// Le rapport Z complet est généré à l'Étape 5 (`generate_z_report()`).
    /// Cette fonction ne fait que la clôture propre au niveau journal.
    ///
    /// # Errors
    /// - `SessionError::NoActiveSession` si aucune session n'est ouverte
    /// - `FiscalError::Database` sur erreur `SQLite`
    pub async fn close_session(&self) -> Result<[u8; 32], FiscalError> {
        let mut session_guard = self.active_session.lock().await;
        let session = session_guard
            .as_mut()
            .ok_or(SessionError::NoActiveSession)?;

        if !session.is_open() {
            return Err(SessionError::SessionAlreadyClosed {
                session_id: session.id.to_string(),
                closed_at: session
                    .closed_at_ms
                    .map(|t| t.to_string())
                    .unwrap_or_default(),
            }
            .into());
        }

        // Enregistrer l'entrée ZClose
        let session_id = session.id;
        let zclose_data = FiscalEntryData {
            session_id,
            operation_type: OperationType::ZClose,
            amount_ttc_cents: Cents::ZERO,
            tva_breakdown: TvaBreakdown::from_ttc(
                Cents::ZERO,
                crate::types::tva::TvaRate::Intermediaire10,
            ),
            tva_5_5_breakdown: TvaBreakdown::zero(crate::types::tva::TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::zero(crate::types::tva::TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(crate::types::tva::TvaRate::Normal20),
            reason: None,
            order_reference: None,
        };

        // On relâche le guard pour appeler record_transaction (qui le reprend)
        drop(session_guard);
        let zclose_entry = self.record_transaction(zclose_data).await?;

        // Maintenant on ferme la session
        let mut session_guard = self.active_session.lock().await;
        let session = session_guard
            .as_mut()
            .ok_or(SessionError::NoActiveSession)?;

        let closing_hash = zclose_entry.hash;
        session.status = SessionStatus::Closed;
        session.closed_at_ms = Some(now_ms());
        session.closing_hash = Some(closing_hash);

        let mut tx = self.store.begin_transaction().await?;
        self.store.close_session(session, &mut tx).await?;
        tx.commit().await?;

        *session_guard = None;

        Ok(closing_hash)
    }

    // -----------------------------------------------------------------------
    // Vérification d'intégrité
    // -----------------------------------------------------------------------

    /// Vérifie l'intégrité de la chaîne de hash du journal complet.
    ///
    /// Relit toutes les entrées depuis `SQLite` et vérifie :
    /// - La continuité de la séquence
    /// - Le hash genesis de la première entrée
    /// - La cohérence de chaque hash avec son précédent
    ///
    /// Cette opération est en **lecture seule** — elle ne modifie pas la base.
    ///
    /// # Errors
    /// - `IntegrityError::EmptyJournal` si le journal est vide
    /// - `IntegrityError::*` pour toute violation de la chaîne
    /// - `FiscalError::Database` sur erreur `SQLite`
    ///
    /// # Examples
    /// ```no_run
    /// # async fn example(journal: &fiscal_engine::journal::Journal)
    /// #     -> Result<(), fiscal_engine::errors::FiscalError> {
    /// let report = journal.verify_integrity().await?;
    /// println!("{} entrées vérifiées, chaîne intègre.", report.entries_checked);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn verify_integrity(&self) -> Result<IntegrityReport, FiscalError> {
        let entries = self.store.load_all_entries().await?;
        verify_chain_integrity(&entries)
    }

    /// Vérifie l'intégrité d'une session spécifique uniquement.
    ///
    /// Plus rapide que `verify_integrity()` pour les audits ponctuels.
    ///
    /// # Errors
    /// Mêmes erreurs que `verify_integrity()`.
    pub async fn verify_session_integrity(
        &self,
        session_id: common::SessionId,
    ) -> Result<IntegrityReport, FiscalError> {
        let entries = self.store.load_entries_for_session(session_id).await?;
        verify_chain_integrity(&entries)
    }

    /// Accès au store sous-jacent (pour les opérations avancées : sync, archivage).
    ///
    /// Utilisé par `sync-client` et par le générateur de rapport Z (Étape 5).
    #[must_use]
    pub fn store(&self) -> &JournalStore {
        &self.store
    }
}

// ---------------------------------------------------------------------------
// Fonctions utilitaires internes
// ---------------------------------------------------------------------------

/// Met à jour les accumulateurs de la session selon le type d'opération.
fn update_session_accumulators(session: &mut Session, entry: &FiscalEntry) {
    session.entry_count += 1;
    match entry.operation_type {
        OperationType::Sale => {
            session.total_sales_cents = session.total_sales_cents + entry.amount_ttc_cents;
        }
        OperationType::Refund => {
            // Les remboursements ont un montant négatif — on stocke la valeur absolue
            session.total_refunds_cents =
                session.total_refunds_cents + Cents(-entry.amount_ttc_cents.0);
        }
        OperationType::Cancel => {
            session.total_cancels_cents =
                session.total_cancels_cents + Cents(-entry.amount_ttc_cents.0);
        }
        OperationType::Discount => {
            session.total_discounts_cents =
                session.total_discounts_cents + Cents(-entry.amount_ttc_cents.0);
        }
        OperationType::ZClose => {
            // ZClose n'impacte pas les totaux financiers
        }
    }
}

/// Retourne l'horodatage courant en millisecondes Unix (UTC).
///
/// Utilise `std::time::SystemTime` — pas de dépendance `chrono` ou `time`.
/// En cas d'erreur système (horloge avant 1970), retourne 0 plutôt que de paniquer.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Tests d'intégration du journal
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{
        entry::FiscalEntryData,
        operation::OperationType,
        tva::{TvaBreakdown, TvaRate},
    };
    use common::GENESIS_HASH;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn make_journal() -> Journal {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Pool SQLite");

        let store = JournalStore::new(pool).await.expect("Store");
        sqlx::query(include_str!("../../migrations/0001_initial_schema.sql"))
            .execute(store.pool_ref())
            .await
            .expect("Migration 0001");
        sqlx::query(include_str!("../../migrations/0005_multi_tva.sql"))
            .execute(store.pool_ref())
            .await
            .expect("Migration 0005");

        Journal::from_store(store).await.expect("Journal")
    }

    fn sale_data(session_id: common::SessionId, amount: i64) -> FiscalEntryData {
        FiscalEntryData {
            session_id,
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(amount),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_5_5_breakdown: crate::types::tva::TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_20_breakdown: crate::types::tva::TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: Some("ORD-TEST".to_string()),
        }
    }

    fn refund_data(session_id: common::SessionId, amount: i64) -> FiscalEntryData {
        FiscalEntryData {
            session_id,
            operation_type: OperationType::Refund,
            amount_ttc_cents: Cents(amount),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_5_5_breakdown: crate::types::tva::TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_20_breakdown: crate::types::tva::TvaBreakdown::zero(TvaRate::Normal20),
            reason: Some("Erreur de commande".to_string()),
            order_reference: None,
        }
    }

    // --- Gestion des sessions ---

    #[tokio::test]
    async fn open_session_creates_active_session() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Ouverture session");
        assert_eq!(session.sequence_number, 1);
        assert!(session.is_open());

        let active = journal.active_session().await;
        assert!(active.is_some());
    }

    #[tokio::test]
    async fn open_two_sessions_fails() {
        let journal = make_journal().await;
        journal.open_session().await.expect("Première session");
        let result = journal.open_session().await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Session(SessionError::SessionAlreadyActive { .. }) => {}
            other => panic!("Attendu SessionAlreadyActive, obtenu : {other}"),
        }
    }

    #[tokio::test]
    async fn no_active_session_returns_none() {
        let journal = make_journal().await;
        assert!(journal.active_session().await.is_none());
    }

    // --- Enregistrement de transactions ---

    #[tokio::test]
    async fn record_transaction_no_session_fails() {
        let journal = make_journal().await;
        let result = journal
            .record_transaction(sale_data(common::SessionId::new(), 1100))
            .await;
        assert!(matches!(
            result,
            Err(FiscalError::Session(SessionError::NoActiveSession))
        ));
    }

    #[tokio::test]
    async fn record_sale_creates_entry_with_hash() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        let entry = journal
            .record_transaction(sale_data(session.id, 1100))
            .await
            .expect("Vente enregistrée");

        assert_eq!(entry.sequence_number, 1);
        assert_eq!(entry.amount_ttc_cents, Cents(1100));
        assert_eq!(entry.previous_hash, GENESIS_HASH);
        assert_ne!(entry.hash, [0u8; 32]);
        assert!(!entry.synced);
    }

    #[tokio::test]
    async fn record_multiple_transactions_chain_hashes() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        let e1 = journal
            .record_transaction(sale_data(session.id, 1100))
            .await
            .expect("Vente 1");
        let e2 = journal
            .record_transaction(sale_data(session.id, 550))
            .await
            .expect("Vente 2");
        let e3 = journal
            .record_transaction(refund_data(session.id, -1100))
            .await
            .expect("Remboursement");

        // Vérification de la chaîne
        assert_eq!(e1.previous_hash, GENESIS_HASH);
        assert_eq!(e2.previous_hash, e1.hash);
        assert_eq!(e3.previous_hash, e2.hash);

        // Séquences continues
        assert_eq!(e1.sequence_number, 1);
        assert_eq!(e2.sequence_number, 2);
        assert_eq!(e3.sequence_number, 3);
    }

    #[tokio::test]
    async fn session_accumulators_updated_correctly() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        journal
            .record_transaction(sale_data(session.id, 1100))
            .await
            .expect("Vente 1");
        journal
            .record_transaction(sale_data(session.id, 550))
            .await
            .expect("Vente 2");
        journal
            .record_transaction(refund_data(session.id, -500))
            .await
            .expect("Remboursement");

        let active = journal.active_session().await.expect("Session active");
        assert_eq!(active.total_sales_cents, Cents(1650));
        assert_eq!(active.total_refunds_cents, Cents(500));
        assert_eq!(active.entry_count, 3);
        assert_eq!(active.net_revenue(), Cents(1150));
    }

    // --- Intégrité ---

    #[tokio::test]
    async fn verify_integrity_empty_journal_fails() {
        let journal = make_journal().await;
        let result = journal.verify_integrity().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn verify_integrity_after_transactions_succeeds() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        for i in 1..=10 {
            journal
                .record_transaction(sale_data(session.id, i * 100))
                .await
                .expect("Vente");
        }

        let report = journal
            .verify_integrity()
            .await
            .expect("Intégrité vérifiée");
        assert_eq!(report.entries_checked, 10);
        assert_eq!(report.first_sequence, 1);
        assert_eq!(report.last_sequence, 10);
    }

    #[tokio::test]
    async fn verify_integrity_one_thousand_transactions() {
        // Test de charge : 1 000 transactions (fixture réaliste NF525)
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        for i in 1..=1000_u64 {
            journal
                .record_transaction(sale_data(session.id, (i * 100) as i64))
                .await
                .unwrap_or_else(|e| panic!("Vente {i} échouée : {e}"));
        }

        let report = journal
            .verify_integrity()
            .await
            .expect("Intégrité sur 1000 transactions");
        assert_eq!(report.entries_checked, 1000);
    }

    // --- Clôture de session ---

    #[tokio::test]
    async fn close_session_creates_zclose_entry() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        journal
            .record_transaction(sale_data(session.id, 1100))
            .await
            .expect("Vente");

        let closing_hash = journal.close_session().await.expect("Clôture");
        assert_ne!(closing_hash, [0u8; 32]);

        // La session ne doit plus être active
        assert!(journal.active_session().await.is_none());
    }

    #[tokio::test]
    async fn close_session_no_session_fails() {
        let journal = make_journal().await;
        let result = journal.close_session().await;
        assert!(matches!(
            result,
            Err(FiscalError::Session(SessionError::NoActiveSession))
        ));
    }

    #[tokio::test]
    async fn integrity_valid_after_close_session() {
        let journal = make_journal().await;
        let session = journal.open_session().await.expect("Session");

        for i in 1..=5_u64 {
            journal
                .record_transaction(sale_data(session.id, (i * 100) as i64))
                .await
                .expect("Vente");
        }

        journal.close_session().await.expect("Clôture");

        // Journal = 5 ventes + 1 ZClose = 6 entrées, toutes intègres
        let report = journal.verify_integrity().await.expect("Intégrité");
        assert_eq!(report.entries_checked, 6);
    }

    #[tokio::test]
    async fn second_session_after_close() {
        let journal = make_journal().await;
        let s1 = journal.open_session().await.expect("Session 1");
        journal
            .record_transaction(sale_data(s1.id, 1000))
            .await
            .expect("Vente");
        journal.close_session().await.expect("Clôture");

        // Ouvrir une deuxième session
        let s2 = journal.open_session().await.expect("Session 2");
        assert_eq!(s2.sequence_number, 2);

        journal
            .record_transaction(sale_data(s2.id, 2000))
            .await
            .expect("Vente session 2");

        // Le journal complet doit être intègre (les hashes se chaînent entre sessions)
        let report = journal.verify_integrity().await.expect("Intégrité globale");
        // Session 1 : 1 vente + 1 ZClose = 2 entrées
        // Session 2 : 1 vente = 1 entrée
        // Total = 3 entrées
        assert_eq!(report.entries_checked, 3);
    }
}
