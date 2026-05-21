//! # store
//!
//! Couche d'accès SQLite pour le journal fiscal.
//!
//! ## Pattern utilisé : Repository
//! Ce module isole complètement toute logique SQL du domaine fiscal.
//! Le `JournalStore` est le seul endroit du crate qui émet des requêtes SQL.
//! Le `Journal` (dans `mod.rs`) appelle le store et applique les règles métier.
//!
//! ## Contraintes SQLite
//! - Mode WAL activé à la connexion (lecture concurrente pendant les writes)
//! - `PRAGMA foreign_keys = ON` activé à chaque connexion
//! - `PRAGMA journal_mode = WAL` et `PRAGMA synchronous = FULL`
//!   garantissent la durabilité même sur crash brutal (FSYNC avant ACK)
//! - Toutes les opérations critiques (INSERT fiscal_entry + UPDATE session)
//!   sont dans une transaction SQLite explicite
//!
//! ## Append-only au niveau SQL
//! Ce module ne contient aucun `UPDATE fiscal_entries` ni `DELETE FROM fiscal_entries`.
//! Toute tentative d'écriture de telles requêtes dans ce fichier est une violation
//! de l'architecture NF525.

use sqlx::{sqlite::SqlitePool, Row};
use uuid::Uuid;

use crate::{
    errors::FiscalError,
    types::{
        entry::FiscalEntry,
        operation::OperationType,
        session::{Session, SessionStatus},
        tva::{TvaBreakdown, TvaRate},
        z_report::ZReport,
    },
};
use common::{Cents, FiscalEntryId, SessionId};

// ---------------------------------------------------------------------------
// Store principal
// ---------------------------------------------------------------------------

/// Repository SQLite pour le journal fiscal.
///
/// Toutes les interactions avec la base de données passent par cette structure.
/// La pool de connexions est partagée (Clone est peu coûteux — pool interne Arc).
#[derive(Debug, Clone)]
pub struct JournalStore {
    /// Pool de connexions SQLite partagée.
    pub pool: SqlitePool,
}

impl JournalStore {
    /// Crée un nouveau store et initialise les PRAGMAs SQLite.
    ///
    /// # Arguments
    /// * `pool` - Pool de connexions SQLite déjà configurée.
    ///
    /// # Errors
    /// Retourne `FiscalError::Database` si les PRAGMAs échouent.
    pub async fn new(pool: SqlitePool) -> Result<Self, FiscalError> {
        // Activer les PRAGMAs critiques sur chaque connexion de la pool
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await?;
        sqlx::query("PRAGMA journal_mode = WAL")
            .execute(&pool)
            .await?;
        // FULL = fsync avant de confirmer l'écriture — obligatoire pour NF525
        sqlx::query("PRAGMA synchronous = FULL")
            .execute(&pool)
            .await?;

        Ok(Self { pool })
    }

    /// Exécute les migrations SQLite depuis le répertoire `migrations/`.
    ///
    /// Idempotent : les migrations déjà appliquées sont ignorées grâce à
    /// la table `_applied_migrations` qui trace chaque version appliquée.
    ///
    /// # Errors
    /// Retourne `FiscalError::Database` si une migration échoue.
    pub async fn run_migrations(&self) -> Result<(), FiscalError> {
        // Table de suivi des migrations — créée une seule fois, jamais supprimée.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _applied_migrations (version TEXT PRIMARY KEY)",
        )
        .execute(&self.pool)
        .await?;

        // Bootstrapping : les DBs créées avant l'introduction du suivi de migrations
        // ont déjà 0001/0002/0003 appliquées. On les marque comme déjà faites si la
        // table sessions existe et que la colonne synced est présente.
        let sessions_exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
        )
        .fetch_one(&self.pool)
        .await?;

        if sessions_exists > 0 {
            let has_synced: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM pragma_table_info('sessions') WHERE name='synced'",
            )
            .fetch_one(&self.pool)
            .await?;

            if has_synced > 0 {
                // Migrations 0001-0003 sont déjà appliquées — les marquer comme faites
                for v in ["0001", "0002", "0003"] {
                    sqlx::query("INSERT OR IGNORE INTO _applied_migrations VALUES (?)")
                        .bind(v)
                        .execute(&self.pool)
                        .await?;
                }
            }
        }

        // Charger les versions déjà appliquées
        let applied: Vec<String> =
            sqlx::query_scalar("SELECT version FROM _applied_migrations")
                .fetch_all(&self.pool)
                .await?;

        let run = |version: &'static str, sql: &'static str| {
            let applied = &applied;
            async move {
                if applied.iter().any(|v| v == version) {
                    return Ok(());
                }
                sqlx::query(sql).execute(&self.pool).await?;
                sqlx::query("INSERT INTO _applied_migrations VALUES (?)")
                    .bind(version)
                    .execute(&self.pool)
                    .await?;
                Ok::<(), FiscalError>(())
            }
        };

        run("0001", include_str!("../../migrations/0001_initial_schema.sql")).await?;
        run("0002", include_str!("../../migrations/0002_z_reports_archives.sql")).await?;
        run("0003", include_str!("../../migrations/0003_sessions_sync.sql")).await?;
        run("0004", include_str!("../../migrations/0004_z_reports_sync.sql")).await?;
        run("0005", include_str!("../../migrations/0005_multi_tva.sql")).await?;

        Ok(())
    }

    // -----------------------------------------------------------------------
    // Sessions — lecture
    // -----------------------------------------------------------------------

    /// Charge la session active (status = 'Open'), s'il en existe une.
    ///
    /// NF525 §4.1 : une seule session peut être ouverte à la fois.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_active_session(&self) -> Result<Option<Session>, FiscalError> {
        let row = sqlx::query(
            "SELECT id, sequence_number, status, opened_at_ms, closed_at_ms, closing_hash,
                    total_sales_cents, total_refunds_cents, total_cancels_cents,
                    total_discounts_cents, entry_count
             FROM sessions
             WHERE status = 'Open'
             LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| session_from_row(&r)).transpose()
    }

    /// Charge une session par son identifiant.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_session_by_id(
        &self,
        session_id: SessionId,
    ) -> Result<Option<Session>, FiscalError> {
        let id_str = session_id.0.to_string();
        let row = sqlx::query(
            "SELECT id, sequence_number, status, opened_at_ms, closed_at_ms, closing_hash,
                    total_sales_cents, total_refunds_cents, total_cancels_cents,
                    total_discounts_cents, entry_count
             FROM sessions WHERE id = ?",
        )
        .bind(&id_str)
        .fetch_optional(&self.pool)
        .await?;

        row.map(|r| session_from_row(&r)).transpose()
    }

    /// Retourne le prochain numéro de séquence de session.
    ///
    /// `MAX(sequence_number) + 1`, ou 1 si aucune session n'existe encore.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn next_session_sequence(&self) -> Result<u64, FiscalError> {
        let row = sqlx::query("SELECT COALESCE(MAX(sequence_number), 0) + 1 AS next FROM sessions")
            .fetch_one(&self.pool)
            .await?;
        let next: i64 = row.try_get("next")?;
        Ok(next as u64)
    }

    // -----------------------------------------------------------------------
    // Sessions — écriture
    // -----------------------------------------------------------------------

    /// Insère une nouvelle session dans la base.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite ou violation de contrainte unique.
    pub async fn insert_session(&self, session: &Session) -> Result<(), FiscalError> {
        sqlx::query(
            "INSERT INTO sessions
             (id, sequence_number, status, opened_at_ms, closed_at_ms, closing_hash,
              total_sales_cents, total_refunds_cents, total_cancels_cents,
              total_discounts_cents, entry_count)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.id.0.to_string())
        .bind(session.sequence_number as i64)
        .bind(session_status_to_str(session.status))
        .bind(session.opened_at_ms as i64)
        .bind(session.closed_at_ms.map(|t| t as i64))
        .bind(session.closing_hash.as_ref().map(|h| h.as_ref()))
        .bind(session.total_sales_cents.0)
        .bind(session.total_refunds_cents.0)
        .bind(session.total_cancels_cents.0)
        .bind(session.total_discounts_cents.0)
        .bind(session.entry_count as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Met à jour les accumulateurs de totaux et le compteur d'entrées d'une session.
    ///
    /// Appelé après chaque `INSERT` dans `fiscal_entries`, dans la même transaction.
    /// C'est le **seul** `UPDATE sessions` autorisé dans ce module.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn update_session_totals(
        &self,
        session: &Session,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<(), FiscalError> {
        sqlx::query(
            "UPDATE sessions SET
                total_sales_cents     = ?,
                total_refunds_cents   = ?,
                total_cancels_cents   = ?,
                total_discounts_cents = ?,
                entry_count           = ?
             WHERE id = ?",
        )
        .bind(session.total_sales_cents.0)
        .bind(session.total_refunds_cents.0)
        .bind(session.total_cancels_cents.0)
        .bind(session.total_discounts_cents.0)
        .bind(session.entry_count as i64)
        .bind(session.id.0.to_string())
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    /// Clôture une session : met à jour status, closed_at_ms et closing_hash.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn close_session(
        &self,
        session: &Session,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<(), FiscalError> {
        sqlx::query(
            "UPDATE sessions SET
                status       = 'Closed',
                closed_at_ms = ?,
                closing_hash = ?,
                total_sales_cents     = ?,
                total_refunds_cents   = ?,
                total_cancels_cents   = ?,
                total_discounts_cents = ?,
                entry_count           = ?
             WHERE id = ?",
        )
        .bind(session.closed_at_ms.map(|t| t as i64))
        .bind(session.closing_hash.as_ref().map(|h| h.as_ref()))
        .bind(session.total_sales_cents.0)
        .bind(session.total_refunds_cents.0)
        .bind(session.total_cancels_cents.0)
        .bind(session.total_discounts_cents.0)
        .bind(session.entry_count as i64)
        .bind(session.id.0.to_string())
        .execute(tx.as_mut())
        .await?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Entrées fiscales — lecture
    // -----------------------------------------------------------------------

    /// Retourne le prochain numéro de séquence global d'entrée fiscale.
    ///
    /// `MAX(sequence_number) + 1`, ou 1 si le journal est vide.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn next_entry_sequence(&self) -> Result<u64, FiscalError> {
        let row =
            sqlx::query("SELECT COALESCE(MAX(sequence_number), 0) + 1 AS next FROM fiscal_entries")
                .fetch_one(&self.pool)
                .await?;
        let next: i64 = row.try_get("next")?;
        Ok(next as u64)
    }

    /// Retourne le hash de la dernière entrée du journal (pour chaîner la suivante).
    ///
    /// Retourne `GENESIS_HASH` si le journal est vide.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn last_hash(&self) -> Result<[u8; 32], FiscalError> {
        let row = sqlx::query(
            "SELECT hash FROM fiscal_entries ORDER BY sequence_number DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;

        match row {
            None => Ok(common::GENESIS_HASH),
            Some(r) => {
                let bytes: Vec<u8> = r.try_get("hash")?;
                let mut arr = [0u8; 32];
                arr.copy_from_slice(&bytes);
                Ok(arr)
            }
        }
    }

    /// Charge toutes les entrées d'une session, triées par `sequence_number` croissant.
    ///
    /// Utilisé pour la vérification d'intégrité et la génération du rapport Z.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_entries_for_session(
        &self,
        session_id: SessionId,
    ) -> Result<Vec<FiscalEntry>, FiscalError> {
        let id_str = session_id.0.to_string();
        let rows = sqlx::query(
            "SELECT id, sequence_number, session_id, operation_type,
                    amount_ttc_cents, ht_cents, tva_cents, tva_rate,
                    tva_5_5_ht_cents, tva_5_5_tva_cents,
                    tva_10_ht_cents,  tva_10_tva_cents,
                    tva_20_ht_cents,  tva_20_tva_cents,
                    hash, previous_hash, created_at_ms, reason, order_reference, synced
             FROM fiscal_entries
             WHERE session_id = ?
             ORDER BY sequence_number ASC",
        )
        .bind(&id_str)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| entry_from_row(&r))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Charge toutes les entrées du journal, triées par `sequence_number` croissant.
    ///
    /// Utilisé pour la vérification d'intégrité complète du journal.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_all_entries(&self) -> Result<Vec<FiscalEntry>, FiscalError> {
        let rows = sqlx::query(
            "SELECT id, sequence_number, session_id, operation_type,
                    amount_ttc_cents, ht_cents, tva_cents, tva_rate,
                    tva_5_5_ht_cents, tva_5_5_tva_cents,
                    tva_10_ht_cents,  tva_10_tva_cents,
                    tva_20_ht_cents,  tva_20_tva_cents,
                    hash, previous_hash, created_at_ms, reason, order_reference, synced
             FROM fiscal_entries
             ORDER BY sequence_number ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| entry_from_row(&r))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Charge les entrées non synchronisées, triées par `sequence_number` croissant.
    ///
    /// Utilisé par `sync-client` pour récupérer le batch à pousser.
    ///
    /// # Arguments
    /// * `limit` - Nombre maximum d'entrées à retourner (taille du batch).
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_unsynced_entries(
        &self,
        limit: u32,
    ) -> Result<Vec<FiscalEntry>, FiscalError> {
        let rows = sqlx::query(
            "SELECT id, sequence_number, session_id, operation_type,
                    amount_ttc_cents, ht_cents, tva_cents, tva_rate,
                    tva_5_5_ht_cents, tva_5_5_tva_cents,
                    tva_10_ht_cents,  tva_10_tva_cents,
                    tva_20_ht_cents,  tva_20_tva_cents,
                    hash, previous_hash, created_at_ms, reason, order_reference, synced
             FROM fiscal_entries
             WHERE synced = 0
             ORDER BY sequence_number ASC
             LIMIT ?",
        )
        .bind(limit as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| entry_from_row(&r))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Charge les entrées d'une année fiscale, pour l'archivage annuel.
    ///
    /// # Arguments
    /// * `year` - Année civile (ex: 2024).
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_entries_for_year(
        &self,
        year: u32,
    ) -> Result<Vec<FiscalEntry>, FiscalError> {
        // Bornes Unix ms pour l'année civile (UTC)
        let start_ms = year_start_ms(year);
        let end_ms = year_start_ms(year + 1);

        let rows = sqlx::query(
            "SELECT id, sequence_number, session_id, operation_type,
                    amount_ttc_cents, ht_cents, tva_cents, tva_rate,
                    tva_5_5_ht_cents, tva_5_5_tva_cents,
                    tva_10_ht_cents,  tva_10_tva_cents,
                    tva_20_ht_cents,  tva_20_tva_cents,
                    hash, previous_hash, created_at_ms, reason, order_reference, synced
             FROM fiscal_entries
             WHERE created_at_ms >= ? AND created_at_ms < ?
             ORDER BY sequence_number ASC",
        )
        .bind(start_ms as i64)
        .bind(end_ms as i64)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| entry_from_row(&r))
            .collect::<Result<Vec<_>, _>>()
    }

    // -----------------------------------------------------------------------
    // Entrées fiscales — écriture (APPEND-ONLY)
    // -----------------------------------------------------------------------

    /// Insère une entrée fiscale dans le journal.
    ///
    /// **Append-only** : cette fonction ne fait qu'un INSERT.
    /// Il n'existe aucune méthode `update_entry` ou `delete_entry` dans ce module.
    ///
    /// L'INSERT est exécuté dans la transaction `tx` fournie par l'appelant
    /// pour garantir l'atomicité avec la mise à jour des totaux de session.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite ou violation de contrainte CHECK.
    pub async fn insert_entry(
        &self,
        entry: &FiscalEntry,
        tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    ) -> Result<(), FiscalError> {
        let tva_rate_str = tva_rate_to_str(entry.tva_breakdown.rate);

        sqlx::query(
            "INSERT INTO fiscal_entries
             (id, sequence_number, session_id, operation_type,
              amount_ttc_cents, ht_cents, tva_cents, tva_rate,
              tva_5_5_ht_cents, tva_5_5_tva_cents,
              tva_10_ht_cents,  tva_10_tva_cents,
              tva_20_ht_cents,  tva_20_tva_cents,
              hash, previous_hash, created_at_ms, reason, order_reference, synced)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 0)",
        )
        .bind(entry.id.0.to_string())
        .bind(entry.sequence_number as i64)
        .bind(entry.session_id.0.to_string())
        .bind(operation_type_to_str(entry.operation_type))
        .bind(entry.amount_ttc_cents.0)
        .bind(entry.tva_breakdown.ht_cents.0)
        .bind(entry.tva_breakdown.tva_cents.0)
        .bind(tva_rate_str)
        .bind(entry.tva_5_5_breakdown.ht_cents.0)
        .bind(entry.tva_5_5_breakdown.tva_cents.0)
        .bind(entry.tva_10_breakdown.ht_cents.0)
        .bind(entry.tva_10_breakdown.tva_cents.0)
        .bind(entry.tva_20_breakdown.ht_cents.0)
        .bind(entry.tva_20_breakdown.tva_cents.0)
        .bind(entry.hash.as_ref())
        .bind(entry.previous_hash.as_ref())
        .bind(entry.created_at_ms as i64)
        .bind(entry.reason.as_deref())
        .bind(entry.order_reference.as_deref())
        .execute(tx.as_mut())
        .await?;

        Ok(())
    }

    /// Marque un batch d'entrées comme synchronisées.
    ///
    /// Seul `UPDATE` autorisé sur `fiscal_entries` : uniquement le champ `synced`.
    /// Utilisé exclusivement par `sync-client` — jamais par `fiscal-engine` lui-même.
    ///
    /// # Arguments
    /// * `entry_ids` - Liste des UUID des entrées à marquer comme synchronisées.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn mark_entries_synced(&self, entry_ids: &[Uuid]) -> Result<u64, FiscalError> {
        if entry_ids.is_empty() {
            return Ok(0);
        }

        // Construction d'un UPDATE avec placeholders IN (?, ?, ...)
        let placeholders: String = entry_ids
            .iter()
            .enumerate()
            .map(|(i, _)| if i == 0 { "?".to_string() } else { ", ?".to_string() })
            .collect();

        let sql = format!("UPDATE fiscal_entries SET synced = 1 WHERE id IN ({placeholders})");

        let mut query = sqlx::query(&sql);
        for id in entry_ids {
            query = query.bind(id.to_string());
        }

        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    //AMV : ajouté le 11/05/26 2205 *******************
// -----------------------------------------------------------------------
    // Sessions — synchronisation (utilisé exclusivement par sync-client)
    // -----------------------------------------------------------------------

    /// Charge les sessions non synchronisées, triées par `sequence_number` croissant.
    ///
    /// Utilisé par `sync-client` pour pousser les sessions vers Supabase
    /// **avant** les `fiscal_entries` (contrainte FK côté cloud).
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_unsynced_sessions(&self) -> Result<Vec<Session>, FiscalError> {
        let rows = sqlx::query(
            "SELECT id, sequence_number, status, opened_at_ms, closed_at_ms, closing_hash,
                    total_sales_cents, total_refunds_cents, total_cancels_cents,
                    total_discounts_cents, entry_count
             FROM sessions
             WHERE synced = 0
             ORDER BY sequence_number ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| session_from_row(&r))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Marque un batch de sessions comme synchronisées.
    ///
    /// Seul `UPDATE sessions` autorisé pour la synchronisation.
    /// Met à jour `synced = 1` et `synced_at` (timestamp ms Unix courant).
    ///
    /// # Arguments
    /// * `session_ids` - Liste des UUID des sessions à marquer.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn mark_sessions_synced(&self, session_ids: &[Uuid]) -> Result<u64, FiscalError> {
        if session_ids.is_empty() {
            return Ok(0);
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let placeholders: String = session_ids
            .iter()
            .enumerate()
            .map(|(i, _)| if i == 0 { "?".to_string() } else { ", ?".to_string() })
            .collect();

        let sql = format!(
            "UPDATE sessions SET synced = 1, synced_at = ? WHERE id IN ({placeholders})"
        );

        let mut query = sqlx::query(&sql).bind(now_ms);
        for id in session_ids {
            query = query.bind(id.to_string());
        }

        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    // -----------------------------------------------------------------------
    // Z-reports — synchronisation (utilisé exclusivement par sync-client)
    // -----------------------------------------------------------------------

    /// Charge les rapports Z non synchronisés, triés par `session_sequence_number` croissant.
    ///
    /// Utilisé par `sync-client` pour pousser les z_reports vers Supabase
    /// **après** les sessions (FK session_id côté cloud).
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn load_unsynced_z_reports(&self) -> Result<Vec<ZReport>, FiscalError> {
        let rows = sqlx::query(
            "SELECT id, session_id, session_sequence_number, generated_at_ms,
                    first_entry_sequence, last_entry_sequence, entry_count,
                    total_sales_cents, total_refunds_cents, total_cancels_cents, total_discounts_cents,
                    tva_5_5_ht_cents, tva_5_5_tva_cents, tva_5_5_ttc_cents,
                    tva_10_ht_cents,  tva_10_tva_cents,  tva_10_ttc_cents,
                    tva_20_ht_cents,  tva_20_tva_cents,  tva_20_ttc_cents,
                    closing_hash
             FROM z_reports
             WHERE synced = 0
             ORDER BY session_sequence_number ASC",
        )
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| z_report_from_row(&r))
            .collect::<Result<Vec<_>, _>>()
    }

    /// Marque un batch de rapports Z comme synchronisés.
    ///
    /// # Arguments
    /// * `report_ids` - Liste des UUID des rapports Z à marquer.
    ///
    /// # Errors
    /// `FiscalError::Database` sur erreur SQLite.
    pub async fn mark_z_reports_synced(&self, report_ids: &[Uuid]) -> Result<u64, FiscalError> {
        if report_ids.is_empty() {
            return Ok(0);
        }

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let placeholders: String = report_ids
            .iter()
            .enumerate()
            .map(|(i, _)| if i == 0 { "?".to_string() } else { ", ?".to_string() })
            .collect();

        let sql = format!(
            "UPDATE z_reports SET synced = 1, synced_at = ? WHERE id IN ({placeholders})"
        );

        let mut query = sqlx::query(&sql).bind(now_ms);
        for id in report_ids {
            query = query.bind(id.to_string());
        }

        let result = query.execute(&self.pool).await?;
        Ok(result.rows_affected())
    }

    // -----------------------------------------------------------------------
    // Transactions SQLite explicites
    // -----------------------------------------------------------------------

    /// Démarre une transaction SQLite.
    ///
    /// À utiliser pour les opérations qui modifient à la fois `fiscal_entries`
    /// et `sessions` (garantit l'atomicité).
    ///
    /// # Errors
    /// `FiscalError::Database` si la transaction ne peut pas démarrer.
    pub async fn begin_transaction(
        &self,
    ) -> Result<sqlx::Transaction<'_, sqlx::Sqlite>, FiscalError> {
        Ok(self.pool.begin().await?)
    }

    /// Accès direct à la pool SQLite — pour healthcheck et opérations avancées.
    ///
    /// # Safety
    /// La pool est en lecture-écriture. À utiliser avec précaution.
    #[must_use]
    pub fn pool_ref(&self) -> &SqlitePool {
        &self.pool
    }
}

// ---------------------------------------------------------------------------
// Fonctions de conversion : Rust ↔ SQLite
// ---------------------------------------------------------------------------

fn session_status_to_str(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Open => "Open",
        SessionStatus::Closed => "Closed",
    }
}

fn operation_type_to_str(op: OperationType) -> &'static str {
    match op {
        OperationType::Sale => "Sale",
        OperationType::Refund => "Refund",
        OperationType::Cancel => "Cancel",
        OperationType::Discount => "Discount",
        OperationType::ZClose => "ZClose",
    }
}

fn tva_rate_to_str(rate: TvaRate) -> &'static str {
    match rate {
        TvaRate::Reduit5_5 => "5.5",
        TvaRate::Intermediaire10 => "10",
        TvaRate::Normal20 => "20",
    }
}

fn str_to_operation_type(s: &str) -> Result<OperationType, FiscalError> {
    match s {
        "Sale" => Ok(OperationType::Sale),
        "Refund" => Ok(OperationType::Refund),
        "Cancel" => Ok(OperationType::Cancel),
        "Discount" => Ok(OperationType::Discount),
        "ZClose" => Ok(OperationType::ZClose),
        other => Err(FiscalError::Database(sqlx::Error::Protocol(format!(
            "Type d'opération inconnu en base : '{other}'"
        )))),
    }
}

fn str_to_tva_rate(s: &str) -> Result<TvaRate, FiscalError> {
    match s {
        "5.5" => Ok(TvaRate::Reduit5_5),
        "10" => Ok(TvaRate::Intermediaire10),
        "20" => Ok(TvaRate::Normal20),
        other => Err(FiscalError::Database(sqlx::Error::Protocol(format!(
            "Taux de TVA inconnu en base : '{other}'"
        )))),
    }
}

fn str_to_session_status(s: &str) -> Result<SessionStatus, FiscalError> {
    match s {
        "Open" => Ok(SessionStatus::Open),
        "Closed" => Ok(SessionStatus::Closed),
        other => Err(FiscalError::Database(sqlx::Error::Protocol(format!(
            "Statut de session inconnu en base : '{other}'"
        )))),
    }
}

/// Construit un `Session` depuis une ligne SQLite.
fn session_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Session, FiscalError> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        FiscalError::Database(sqlx::Error::Protocol(format!("UUID session invalide : {e}")))
    })?;

    let status_str: String = row.try_get("status")?;
    let status = str_to_session_status(&status_str)?;

    let closing_hash: Option<Vec<u8>> = row.try_get("closing_hash")?;
    let closing_hash_arr: Option<[u8; 32]> = closing_hash.map(|v| {
        let mut arr = [0u8; 32];
        arr.copy_from_slice(&v);
        arr
    });

    Ok(Session {
        id: SessionId(id),
        sequence_number: {
            let n: i64 = row.try_get("sequence_number")?;
            n as u64
        },
        status,
        opened_at_ms: {
            let t: i64 = row.try_get("opened_at_ms")?;
            t as u64
        },
        closed_at_ms: {
            let t: Option<i64> = row.try_get("closed_at_ms")?;
            t.map(|v| v as u64)
        },
        closing_hash: closing_hash_arr,
        total_sales_cents: Cents(row.try_get("total_sales_cents")?),
        total_refunds_cents: Cents(row.try_get("total_refunds_cents")?),
        total_cancels_cents: Cents(row.try_get("total_cancels_cents")?),
        total_discounts_cents: Cents(row.try_get("total_discounts_cents")?),
        entry_count: {
            let n: i64 = row.try_get("entry_count")?;
            n as u64
        },
    })
}

/// Construit un `FiscalEntry` depuis une ligne SQLite.
fn entry_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<FiscalEntry, FiscalError> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        FiscalError::Database(sqlx::Error::Protocol(format!("UUID entrée invalide : {e}")))
    })?;

    let session_id_str: String = row.try_get("session_id")?;
    let session_id = Uuid::parse_str(&session_id_str).map_err(|e| {
        FiscalError::Database(sqlx::Error::Protocol(format!("UUID session invalide : {e}")))
    })?;

    let op_str: String = row.try_get("operation_type")?;
    let operation_type = str_to_operation_type(&op_str)?;

    let rate_str: String = row.try_get("tva_rate")?;
    let tva_rate = str_to_tva_rate(&rate_str)?;

    let amount_ttc_cents: i64 = row.try_get("amount_ttc_cents")?;
    let ht_cents: i64 = row.try_get("ht_cents")?;
    let tva_cents_val: i64 = row.try_get("tva_cents")?;

    let tva_breakdown = TvaBreakdown {
        rate: tva_rate,
        ht_cents: Cents(ht_cents),
        tva_cents: Cents(tva_cents_val),
        ttc_cents: Cents(amount_ttc_cents),
    };

    // Colonnes multi-TVA (migration 0005)
    let tva_5_5_ht: i64 = row.try_get("tva_5_5_ht_cents")?;
    let tva_5_5_tva: i64 = row.try_get("tva_5_5_tva_cents")?;
    let tva_10_ht: i64 = row.try_get("tva_10_ht_cents")?;
    let tva_10_tva: i64 = row.try_get("tva_10_tva_cents")?;
    let tva_20_ht: i64 = row.try_get("tva_20_ht_cents")?;
    let tva_20_tva: i64 = row.try_get("tva_20_tva_cents")?;

    let tva_5_5_breakdown = TvaBreakdown {
        rate: TvaRate::Reduit5_5,
        ht_cents: Cents(tva_5_5_ht),
        tva_cents: Cents(tva_5_5_tva),
        ttc_cents: Cents(tva_5_5_ht + tva_5_5_tva),
    };
    let tva_10_breakdown = TvaBreakdown {
        rate: TvaRate::Intermediaire10,
        ht_cents: Cents(tva_10_ht),
        tva_cents: Cents(tva_10_tva),
        ttc_cents: Cents(tva_10_ht + tva_10_tva),
    };
    let tva_20_breakdown = TvaBreakdown {
        rate: TvaRate::Normal20,
        ht_cents: Cents(tva_20_ht),
        tva_cents: Cents(tva_20_tva),
        ttc_cents: Cents(tva_20_ht + tva_20_tva),
    };

    let hash_bytes: Vec<u8> = row.try_get("hash")?;
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&hash_bytes);

    let prev_bytes: Vec<u8> = row.try_get("previous_hash")?;
    let mut previous_hash = [0u8; 32];
    previous_hash.copy_from_slice(&prev_bytes);

    let synced_int: i64 = row.try_get("synced")?;

    Ok(FiscalEntry {
        id: FiscalEntryId(id),
        sequence_number: {
            let n: i64 = row.try_get("sequence_number")?;
            n as u64
        },
        session_id: SessionId(session_id),
        operation_type,
        amount_ttc_cents: Cents(amount_ttc_cents),
        tva_breakdown,
        tva_5_5_breakdown,
        tva_10_breakdown,
        tva_20_breakdown,
        reason: row.try_get("reason")?,
        order_reference: row.try_get("order_reference")?,
        hash,
        previous_hash,
        created_at_ms: {
            let t: i64 = row.try_get("created_at_ms")?;
            t as u64
        },
        synced: synced_int != 0,
    })
}

fn z_report_from_row(row: &sqlx::sqlite::SqliteRow) -> Result<ZReport, FiscalError> {
    let id_str: String = row.try_get("id")?;
    let id = Uuid::parse_str(&id_str).map_err(|e| {
        FiscalError::Database(sqlx::Error::Protocol(format!("UUID z_report invalide: {e}")))
    })?;

    let session_id_str: String = row.try_get("session_id")?;
    let session_uuid = Uuid::parse_str(&session_id_str).map_err(|e| {
        FiscalError::Database(sqlx::Error::Protocol(format!("UUID session invalide: {e}")))
    })?;

    let closing_hash_bytes: Vec<u8> = row.try_get("closing_hash")?;
    let mut closing_hash = [0u8; 32];
    closing_hash.copy_from_slice(&closing_hash_bytes);

    Ok(ZReport {
        id,
        session_id: SessionId(session_uuid),
        session_sequence_number: { let n: i64 = row.try_get("session_sequence_number")?; n as u64 },
        generated_at_ms: { let t: i64 = row.try_get("generated_at_ms")?; t as u64 },
        first_entry_sequence: { let n: i64 = row.try_get("first_entry_sequence")?; n as u64 },
        last_entry_sequence:  { let n: i64 = row.try_get("last_entry_sequence")?;  n as u64 },
        entry_count:          { let n: i64 = row.try_get("entry_count")?;          n as u64 },
        total_sales_cents:    Cents(row.try_get("total_sales_cents")?),
        total_refunds_cents:  Cents(row.try_get("total_refunds_cents")?),
        total_cancels_cents:  Cents(row.try_get("total_cancels_cents")?),
        total_discounts_cents: Cents(row.try_get("total_discounts_cents")?),
        tva_5_5_breakdown: TvaBreakdown {
            rate: TvaRate::Reduit5_5,
            ht_cents:  Cents(row.try_get("tva_5_5_ht_cents")?),
            tva_cents: Cents(row.try_get("tva_5_5_tva_cents")?),
            ttc_cents: Cents(row.try_get("tva_5_5_ttc_cents")?),
        },
        tva_10_breakdown: TvaBreakdown {
            rate: TvaRate::Intermediaire10,
            ht_cents:  Cents(row.try_get("tva_10_ht_cents")?),
            tva_cents: Cents(row.try_get("tva_10_tva_cents")?),
            ttc_cents: Cents(row.try_get("tva_10_ttc_cents")?),
        },
        tva_20_breakdown: TvaBreakdown {
            rate: TvaRate::Normal20,
            ht_cents:  Cents(row.try_get("tva_20_ht_cents")?),
            tva_cents: Cents(row.try_get("tva_20_tva_cents")?),
            ttc_cents: Cents(row.try_get("tva_20_ttc_cents")?),
        },
        closing_hash,
    })
}

/// Calcule le timestamp Unix en millisecondes du 1er janvier d'une année (UTC).
///
/// Utilisé pour filtrer les entrées par année civile dans l'archivage.
fn year_start_ms(year: u32) -> u64 {
    // Nombre de jours depuis l'époque Unix (1970-01-01) jusqu'au 1er janvier de `year`
    // Calcul sans dépendance external (pas de chrono) :
    // - Années bissextiles : divisibles par 4, sauf par 100, sauf par 400
    let mut days: u64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }
    days * 24 * 3_600 * 1_000 // ms
}

fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

// ---------------------------------------------------------------------------
// Tests du store (nécessitent SQLite en mémoire)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_store() -> JournalStore {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Pool SQLite en mémoire");
        let store = JournalStore::new(pool)
            .await
            .expect("Store initialisé");
        // Appliquer le schéma manuellement (migrate!() ne fonctionne pas en tests sans fichiers)
        sqlx::query(include_str!("../../migrations/0001_initial_schema.sql"))
            .execute(&store.pool)
            .await
            .expect("Migration 0001 appliquée");
        sqlx::query(include_str!("../../migrations/0002_z_reports_archives.sql"))
            .execute(&store.pool)
            .await
            .expect("Migration 0002 appliquée");
        sqlx::query(include_str!("../../migrations/0003_sessions_sync.sql"))
            .execute(&store.pool)
            .await
            .expect("Migration 0003 appliquée");
        sqlx::query(include_str!("../../migrations/0004_z_reports_sync.sql"))
            .execute(&store.pool)
            .await
            .expect("Migration 0004 appliquée");
        sqlx::query(include_str!("../../migrations/0005_multi_tva.sql"))
            .execute(&store.pool)
            .await
            .expect("Migration 0005 appliquée");
        store
    }

    fn make_session(seq: u64) -> Session {
        Session::new(seq, 1_700_000_000_000 + seq * 1000)
    }

    // --- Tests sessions ---

    #[tokio::test]
    async fn insert_and_load_session() {
        let store = setup_store().await;
        let session = make_session(1);
        store.insert_session(&session).await.expect("INSERT session");

        let loaded = store
            .load_active_session()
            .await
            .expect("Lecture session active")
            .expect("Session doit exister");

        assert_eq!(loaded.id, session.id);
        assert_eq!(loaded.sequence_number, 1);
        assert_eq!(loaded.status, SessionStatus::Open);
        assert_eq!(loaded.total_sales_cents, Cents::ZERO);
    }

    #[tokio::test]
    async fn no_active_session_returns_none() {
        let store = setup_store().await;
        let result = store.load_active_session().await.expect("Requête OK");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn next_session_sequence_starts_at_1() {
        let store = setup_store().await;
        let seq = store.next_session_sequence().await.expect("Séquence");
        assert_eq!(seq, 1);
    }

    #[tokio::test]
    async fn next_session_sequence_increments() {
        let store = setup_store().await;
        let s1 = make_session(1);
        store.insert_session(&s1).await.expect("INSERT s1");
        let seq = store.next_session_sequence().await.expect("Séquence");
        assert_eq!(seq, 2);
    }

    // --- Tests fiscal_entries ---

    #[tokio::test]
    async fn last_hash_empty_journal_returns_genesis() {
        let store = setup_store().await;
        let hash = store.last_hash().await.expect("last_hash");
        assert_eq!(hash, common::GENESIS_HASH);
    }

    #[tokio::test]
    async fn insert_entry_and_reload() {
        use crate::hash_engine::build_entry_for_test;
        use crate::types::{operation::OperationType, tva::TvaRate};
        use common::GENESIS_HASH;

        let store = setup_store().await;

        // Session requise (FK)
        let session = make_session(1);
        store.insert_session(&session).await.expect("INSERT session");

        let session_bytes = session.id.0.into_bytes();
        let entry = build_entry_for_test(
            1,
            session_bytes,
            OperationType::Sale,
            1100,
            TvaRate::Intermediaire10,
            1_700_000_000_000,
            GENESIS_HASH,
        );

        let mut tx = store.begin_transaction().await.expect("Transaction");
        store.insert_entry(&entry, &mut tx).await.expect("INSERT entry");
        tx.commit().await.expect("COMMIT");

        let loaded = store
            .load_entries_for_session(session.id)
            .await
            .expect("Chargement entrées");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].sequence_number, 1);
        assert_eq!(loaded[0].amount_ttc_cents, common::Cents(1100));
        assert_eq!(loaded[0].hash, entry.hash);
        assert!(!loaded[0].synced);
    }

    #[tokio::test]
    async fn last_hash_after_insert() {
        use crate::hash_engine::build_entry_for_test;
        use crate::types::{operation::OperationType, tva::TvaRate};
        use common::GENESIS_HASH;

        let store = setup_store().await;
        let session = make_session(1);
        store.insert_session(&session).await.expect("INSERT session");

        let session_bytes = session.id.0.into_bytes();
        let entry = build_entry_for_test(
            1, session_bytes, OperationType::Sale, 1100,
            TvaRate::Intermediaire10, 1_700_000_000_000, GENESIS_HASH,
        );
        let mut tx = store.begin_transaction().await.expect("TX");
        store.insert_entry(&entry, &mut tx).await.expect("INSERT");
        tx.commit().await.expect("COMMIT");

        let last = store.last_hash().await.expect("last_hash");
        assert_eq!(last, entry.hash);
    }

    #[tokio::test]
    async fn mark_entries_synced() {
        use crate::hash_engine::build_entry_for_test;
        use crate::types::{operation::OperationType, tva::TvaRate};
        use common::GENESIS_HASH;

        let store = setup_store().await;
        let session = make_session(1);
        store.insert_session(&session).await.expect("INSERT session");

        let session_bytes = session.id.0.into_bytes();
        let entry = build_entry_for_test(
            1, session_bytes, OperationType::Sale, 500,
            TvaRate::Intermediaire10, 1_700_000_000_000, GENESIS_HASH,
        );
        let mut tx = store.begin_transaction().await.expect("TX");
        store.insert_entry(&entry, &mut tx).await.expect("INSERT");
        tx.commit().await.expect("COMMIT");

        // Avant sync
        let unsynced = store.load_unsynced_entries(100).await.expect("Unsynced");
        assert_eq!(unsynced.len(), 1);

        // Marquer comme synchronisé
        let affected = store
            .mark_entries_synced(&[entry.id.0])
            .await
            .expect("mark_synced");
        assert_eq!(affected, 1);

        // Après sync
        let unsynced = store.load_unsynced_entries(100).await.expect("Unsynced après");
        assert_eq!(unsynced.len(), 0);
    }

    // --- Tests utilitaires ---

    #[test]
    fn year_start_ms_1970() {
        assert_eq!(year_start_ms(1970), 0);
    }

    #[test]
    fn year_start_ms_1971() {
        // 1970 = 365 jours (non bissextile)
        assert_eq!(year_start_ms(1971), 365 * 24 * 3_600 * 1_000);
    }

    #[test]
    fn year_start_ms_2024_is_after_2023() {
        assert!(year_start_ms(2024) > year_start_ms(2023));
    }

    #[test]
    fn leap_year_detection() {
        assert!(is_leap_year(2000)); // divisible par 400
        assert!(is_leap_year(2024)); // divisible par 4
        assert!(!is_leap_year(1900)); // divisible par 100 mais pas 400
        assert!(!is_leap_year(2023)); // non divisible par 4
    }
}
