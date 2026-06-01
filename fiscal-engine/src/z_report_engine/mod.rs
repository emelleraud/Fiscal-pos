//! # `z_report_engine`
//!
//! Génération du rapport Z de clôture de session (NF525 §6).
//!
//! ## Responsabilités
//! - Agréger les entrées d'une session fermée en totaux financiers
//! - Décomposer la TVA par taux (5,5% / 10% / 20%)
//! - Construire et persister le `ZReport` (immuable après création)
//! - Vérifier qu'un rapport Z n'est pas régénéré (idempotence stricte)
//!
//! ## Contrainte NF525 §6 — Immuabilité
//! Un rapport Z ne peut être généré qu'**une seule fois** par session.
//! Si la session a déjà un rapport Z en base, `generate_z_report()` retourne
//! `SessionError::ZReportAlreadyGenerated` sans aucune écriture.
//!
//! ## Relation avec `Journal::close_session()`
//! `close_session()` inscrit l'entrée `ZClose` et ferme la session en base.
//! `generate_z_report()` lit ensuite les entrées pour produire le rapport.
//! Les deux opérations sont délibérément séparées pour respecter le principe
//! de responsabilité unique : la clôture est dans `journal`, l'agrégation ici.

use sqlx::{Row, sqlite::SqlitePool};
use uuid::Uuid;

use crate::{
    errors::{FiscalError, SessionError},
    hash_engine::hex_encode,
    types::{
        entry::FiscalEntry,
        operation::OperationType,
        tva::{TvaBreakdown, TvaRate},
        z_report::ZReport,
    },
};
use common::{Cents, SessionId};

// ---------------------------------------------------------------------------
// Génération du rapport Z
// ---------------------------------------------------------------------------

/// Génère le rapport Z d'une session clôturée.
///
/// ## Préconditions
/// - La session identifiée par `session_id` doit exister et être **clôturée**
///   (statut `Closed` en base — c'est `Journal::close_session()` qui l'a fait)
/// - Aucun rapport Z ne doit exister pour cette session
///
/// ## Algorithme
/// 1. Charger la session et vérifier son statut
/// 2. Vérifier qu'aucun rapport Z n'existe déjà (idempotence)
/// 3. Charger toutes les entrées de la session depuis `SQLite`
/// 4. Agréger les totaux par type d'opération
/// 5. Décomposer la TVA par taux
/// 6. Construire le `ZReport`
/// 7. Persister le rapport en base
///
/// # Arguments
/// * `pool` - Pool `SQLite` partagée avec le journal.
/// * `session_id` - Session à clôturer.
///
/// # Errors
/// - `SessionError::NoActiveSession` si la session est introuvable
/// - `SessionError::ZReportAlreadyGenerated` si un rapport Z existe déjà
/// - `FiscalError::Database` sur erreur `SQLite`
///
/// # Examples
/// ```no_run
/// use fiscal_engine::z_report_engine::generate_z_report;
/// use common::SessionId;
///
/// # async fn example(pool: sqlx::sqlite::SqlitePool, session_id: SessionId)
/// #     -> Result<(), fiscal_engine::errors::FiscalError> {
/// let z_report = generate_z_report(&pool, session_id).await?;
/// println!("Rapport Z #{} — CA net : {}",
///     z_report.session_sequence_number,
///     z_report.net_revenue());
/// # Ok(())
/// # }
/// ```
pub async fn generate_z_report(
    pool: &SqlitePool,
    session_id: SessionId,
) -> Result<ZReport, FiscalError> {
    let session_id_str = session_id.0.to_string();

    // 1. Charger la session et vérifier son statut
    let session_row = sqlx::query(
        "SELECT id, sequence_number, status, closing_hash FROM sessions WHERE id = ?",
    )
    .bind(&session_id_str)
    .fetch_optional(pool)
    .await?
    .ok_or(SessionError::NoActiveSession)?;

    let status_str: String = session_row.try_get("status")?;
    if status_str != "Closed" {
        return Err(FiscalError::SessionClosed {
            session_id: session_id_str.clone(),
        });
    }

    let session_sequence_number: i64 = session_row.try_get("sequence_number")?;
    let closing_hash_bytes: Vec<u8> = session_row.try_get("closing_hash")?;
    let mut closing_hash = [0u8; 32];
    closing_hash.copy_from_slice(&closing_hash_bytes);

    // 2. Vérifier qu'aucun rapport Z n'existe déjà (idempotence NF525 §6)
    let existing = sqlx::query("SELECT id FROM z_reports WHERE session_id = ?")
        .bind(&session_id_str)
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
        return Err(SessionError::ZReportAlreadyGenerated {
            session_id: session_id_str,
        }
        .into());
    }

    // 3. Charger toutes les entrées de la session
    let entries = load_session_entries(pool, &session_id_str).await?;

    if entries.is_empty() {
        return Err(FiscalError::SessionClosed {
            session_id: session_id_str,
        });
    }

    // 4 & 5. Agréger les totaux et décomposer la TVA
    let aggregated = aggregate_entries(&entries);

    let first_entry_sequence = entries.first().map_or(1, |e| e.sequence_number);
    let last_entry_sequence = entries.last().map_or(1, |e| e.sequence_number);

    // 6. Construire le rapport Z
    let report = ZReport {
        id: Uuid::now_v7(),
        session_id,
        session_sequence_number: session_sequence_number.cast_unsigned(),
        generated_at_ms: now_ms(),
        first_entry_sequence,
        last_entry_sequence,
        entry_count: u64::try_from(entries.len()).unwrap_or(u64::MAX),
        total_sales_cents: aggregated.total_sales,
        total_refunds_cents: aggregated.total_refunds,
        total_cancels_cents: aggregated.total_cancels,
        total_discounts_cents: aggregated.total_discounts,
        tva_5_5_breakdown: aggregated.tva_5_5,
        tva_10_breakdown: aggregated.tva_10,
        tva_20_breakdown: aggregated.tva_20,
        closing_hash,
    };

    // 7. Persister en base
    persist_z_report(pool, &report).await?;

    Ok(report)
}

/// Charge un rapport Z existant depuis la base, par `session_id`.
///
/// # Errors
/// - `FiscalError::Database` sur erreur `SQLite`
pub async fn load_z_report(
    pool: &SqlitePool,
    session_id: SessionId,
) -> Result<Option<ZReport>, FiscalError> {
    let session_id_str = session_id.0.to_string();

    let row = sqlx::query(
        "SELECT id, session_id, session_sequence_number, generated_at_ms,
                first_entry_sequence, last_entry_sequence, entry_count,
                total_sales_cents, total_refunds_cents, total_cancels_cents, total_discounts_cents,
                tva_5_5_ht_cents, tva_5_5_tva_cents, tva_5_5_ttc_cents,
                tva_10_ht_cents,  tva_10_tva_cents,  tva_10_ttc_cents,
                tva_20_ht_cents,  tva_20_tva_cents,  tva_20_ttc_cents,
                closing_hash
         FROM z_reports WHERE session_id = ?",
    )
    .bind(&session_id_str)
    .fetch_optional(pool)
    .await?;

    row.map(|r| z_report_from_row(&r)).transpose()
}

// ---------------------------------------------------------------------------
// Agrégation des entrées
// ---------------------------------------------------------------------------

/// Résultat de l'agrégation des entrées d'une session.
struct AggregatedTotals {
    total_sales: Cents,
    total_refunds: Cents,
    total_cancels: Cents,
    total_discounts: Cents,
    tva_5_5: TvaBreakdown,
    tva_10: TvaBreakdown,
    tva_20: TvaBreakdown,
}

/// Agrège les totaux financiers et la décomposition TVA depuis les entrées de la session.
///
/// ## Décomposition TVA
/// Chaque entrée porte son propre `tva_breakdown.rate`.
/// On accumule les montants HT, TVA et TTC séparément par taux.
/// Les remboursements/annulations/remises ont des montants négatifs et
/// sont exécutés **en valeur absolue** pour les totaux du rapport Z.
///
/// ## Entrées `ZClose`
/// L'entrée `ZClose` (montant 0) n'impacte aucun total financier.
fn aggregate_entries(entries: &[FiscalEntry]) -> AggregatedTotals {
    let mut total_sales = Cents::ZERO;
    let mut total_refunds = Cents::ZERO;
    let mut total_cancels = Cents::ZERO;
    let mut total_discounts = Cents::ZERO;

    // Accumulateurs TVA par taux (HT / TVA / TTC)
    let mut ht_5_5 = Cents::ZERO;
    let mut tva_5_5 = Cents::ZERO;
    let mut ttc_5_5 = Cents::ZERO;

    let mut ht_10 = Cents::ZERO;
    let mut tva_10 = Cents::ZERO;
    let mut ttc_10 = Cents::ZERO;

    let mut ht_20 = Cents::ZERO;
    let mut tva_20 = Cents::ZERO;
    let mut ttc_20 = Cents::ZERO;

    for entry in entries {
        match entry.operation_type {
            OperationType::Sale => {
                total_sales = total_sales + entry.amount_ttc_cents;

                // Accumulation TVA par taux — utilise les breakdowns per-rate (NF525 §4.3.1)
                // Permet la ventilation correcte des entrées multi-taux
                ht_5_5 = ht_5_5 + entry.tva_5_5_breakdown.ht_cents;
                tva_5_5 = tva_5_5 + entry.tva_5_5_breakdown.tva_cents;
                ttc_5_5 = ttc_5_5 + entry.tva_5_5_breakdown.ttc_cents;

                ht_10 = ht_10 + entry.tva_10_breakdown.ht_cents;
                tva_10 = tva_10 + entry.tva_10_breakdown.tva_cents;
                ttc_10 = ttc_10 + entry.tva_10_breakdown.ttc_cents;

                ht_20 = ht_20 + entry.tva_20_breakdown.ht_cents;
                tva_20 = tva_20 + entry.tva_20_breakdown.tva_cents;
                ttc_20 = ttc_20 + entry.tva_20_breakdown.ttc_cents;
            }
            OperationType::Refund => {
                // Montant négatif → valeur absolue pour le total
                total_refunds = total_refunds + Cents(-entry.amount_ttc_cents.0);
            }
            OperationType::Cancel => {
                total_cancels = total_cancels + Cents(-entry.amount_ttc_cents.0);
            }
            OperationType::Discount => {
                total_discounts = total_discounts + Cents(-entry.amount_ttc_cents.0);
            }
            OperationType::ZClose => {
                // Pas d'impact sur les totaux financiers
            }
        }
    }

    AggregatedTotals {
        total_sales,
        total_refunds,
        total_cancels,
        total_discounts,
        tva_5_5: TvaBreakdown {
            rate: TvaRate::Reduit5_5,
            ht_cents: ht_5_5,
            tva_cents: tva_5_5,
            ttc_cents: ttc_5_5,
        },
        tva_10: TvaBreakdown {
            rate: TvaRate::Intermediaire10,
            ht_cents: ht_10,
            tva_cents: tva_10,
            ttc_cents: ttc_10,
        },
        tva_20: TvaBreakdown {
            rate: TvaRate::Normal20,
            ht_cents: ht_20,
            tva_cents: tva_20,
            ttc_cents: ttc_20,
        },
    }
}

// ---------------------------------------------------------------------------
// Persistence
// ---------------------------------------------------------------------------

async fn persist_z_report(pool: &SqlitePool, report: &ZReport) -> Result<(), FiscalError> {
    sqlx::query(
        "INSERT INTO z_reports (
            id, session_id, session_sequence_number, generated_at_ms,
            first_entry_sequence, last_entry_sequence, entry_count,
            total_sales_cents, total_refunds_cents, total_cancels_cents, total_discounts_cents,
            tva_5_5_ht_cents, tva_5_5_tva_cents, tva_5_5_ttc_cents,
            tva_10_ht_cents,  tva_10_tva_cents,  tva_10_ttc_cents,
            tva_20_ht_cents,  tva_20_tva_cents,  tva_20_ttc_cents,
            closing_hash
         ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(report.id.to_string())
    .bind(report.session_id.0.to_string())
    .bind(report.session_sequence_number.cast_signed())
    .bind(report.generated_at_ms.cast_signed())
    .bind(report.first_entry_sequence.cast_signed())
    .bind(report.last_entry_sequence.cast_signed())
    .bind(report.entry_count.cast_signed())
    .bind(report.total_sales_cents.0)
    .bind(report.total_refunds_cents.0)
    .bind(report.total_cancels_cents.0)
    .bind(report.total_discounts_cents.0)
    .bind(report.tva_5_5_breakdown.ht_cents.0)
    .bind(report.tva_5_5_breakdown.tva_cents.0)
    .bind(report.tva_5_5_breakdown.ttc_cents.0)
    .bind(report.tva_10_breakdown.ht_cents.0)
    .bind(report.tva_10_breakdown.tva_cents.0)
    .bind(report.tva_10_breakdown.ttc_cents.0)
    .bind(report.tva_20_breakdown.ht_cents.0)
    .bind(report.tva_20_breakdown.tva_cents.0)
    .bind(report.tva_20_breakdown.ttc_cents.0)
    .bind(report.closing_hash.as_ref())
    .execute(pool)
    .await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Désérialisation depuis SQLite
// ---------------------------------------------------------------------------

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
        session_sequence_number: { let n: i64 = row.try_get("session_sequence_number")?; n.cast_unsigned() },
        generated_at_ms: { let t: i64 = row.try_get("generated_at_ms")?; t.cast_unsigned() },
        first_entry_sequence: { let n: i64 = row.try_get("first_entry_sequence")?; n.cast_unsigned() },
        last_entry_sequence: { let n: i64 = row.try_get("last_entry_sequence")?; n.cast_unsigned() },
        entry_count: { let n: i64 = row.try_get("entry_count")?; n.cast_unsigned() },
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

// ---------------------------------------------------------------------------
// Helpers SQL internes
// ---------------------------------------------------------------------------

async fn load_session_entries(
    pool: &SqlitePool,
    session_id_str: &str,
) -> Result<Vec<FiscalEntry>, FiscalError> {
    use crate::journal::store::JournalStore;
    // On instancie un store temporaire pour réutiliser la désérialisation existante
    // plutôt que de dupliquer la logique entry_from_row
    let store = JournalStore { pool: pool.clone() };
    let session_uuid = Uuid::parse_str(session_id_str).map_err(|e| {
        FiscalError::Database(sqlx::Error::Protocol(format!("UUID invalide: {e}")))
    })?;
    store.load_entries_for_session(SessionId(session_uuid)).await
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

// ---------------------------------------------------------------------------
// Formatage pour impression (ticket papier / PDF)
// ---------------------------------------------------------------------------

/// Génère le texte du rapport Z formaté pour impression sur ticket 80mm.
///
/// Format lisible par l'humain, utilisé par `pos-app` pour l'impression directe.
/// Ne fait pas partie de l'archive légale — c'est uniquement pour l'opérateur.
///
/// # Examples
/// ```
/// use fiscal_engine::z_report_engine::format_z_report_text;
/// use fiscal_engine::types::z_report::ZReport;
/// use fiscal_engine::types::tva::{TvaBreakdown, TvaRate};
/// use common::{Cents, SessionId};
///
/// let report = ZReport {
///     id: uuid::Uuid::now_v7(),
///     session_id: SessionId::new(),
///     session_sequence_number: 1,
///     generated_at_ms: 1_700_000_000_000,
///     first_entry_sequence: 1,
///     last_entry_sequence: 10,
///     entry_count: 10,
///     total_sales_cents: Cents(10_000),
///     total_refunds_cents: Cents(0),
///     total_cancels_cents: Cents(0),
///     total_discounts_cents: Cents(0),
///     tva_5_5_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Reduit5_5),
///     tva_10_breakdown: TvaBreakdown::from_ttc(Cents(10_000), TvaRate::Intermediaire10),
///     tva_20_breakdown: TvaBreakdown::from_ttc(Cents(0), TvaRate::Normal20),
///     closing_hash: [0u8; 32],
/// };
/// let text = format_z_report_text(&report);
/// assert!(text.contains("RAPPORT Z"));
/// assert!(text.contains("100.00"));
/// ```
#[must_use]
pub fn format_z_report_text(report: &ZReport) -> String {
    let separator = "─".repeat(40);
    let thin = "·".repeat(40);

    // Timestamp lisible (Unix ms → secondes, approximation sans bibliothèque externe)
    let ts_secs = report.generated_at_ms / 1000;
    // Affichage simplifié — le frontend React affichera la date formatée
    let closing_hash_short = hex_encode(&report.closing_hash);
    let hash_display = &closing_hash_short[..16];

    format!(
        "\
{separator}
         RAPPORT Z DE CAISSE
{separator}
Session n°      : {session_seq:>6}
Séquences       : {first_seq} → {last_seq}
Entrées         : {entry_count:>6}
Généré le       : {ts_secs} (Unix)
{thin}
VENTES          : {sales:>10.2} €
REMBOURSEMENTS  : {refunds:>10.2} €
ANNULATIONS     : {cancels:>10.2} €
REMISES         : {discounts:>10.2} €
{thin}
CA NET          : {net:>10.2} €
{thin}
TVA 5,5%
  HT            : {ht_5_5:>10.2} €
  TVA           : {tva_5_5:>10.2} €
  TTC           : {ttc_5_5:>10.2} €
TVA 10%
  HT            : {ht_10:>10.2} €
  TVA           : {tva_10:>10.2} €
  TTC           : {ttc_10:>10.2} €
TVA 20%
  HT            : {ht_20:>10.2} €
  TVA           : {tva_20:>10.2} €
  TTC           : {ttc_20:>10.2} €
{thin}
TVA TOTALE      : {total_tva:>10.2} €
{separator}
HASH CLOTURE    : {hash_display}...
{separator}
",
        separator = separator,
        thin = thin,
        session_seq = report.session_sequence_number,
        first_seq = report.first_entry_sequence,
        last_seq = report.last_entry_sequence,
        entry_count = report.entry_count,
        ts_secs = ts_secs,
        sales = report.total_sales_cents.to_euros(),
        refunds = report.total_refunds_cents.to_euros(),
        cancels = report.total_cancels_cents.to_euros(),
        discounts = report.total_discounts_cents.to_euros(),
        net = report.net_revenue().to_euros(),
        ht_5_5 = report.tva_5_5_breakdown.ht_cents.to_euros(),
        tva_5_5 = report.tva_5_5_breakdown.tva_cents.to_euros(),
        ttc_5_5 = report.tva_5_5_breakdown.ttc_cents.to_euros(),
        ht_10 = report.tva_10_breakdown.ht_cents.to_euros(),
        tva_10 = report.tva_10_breakdown.tva_cents.to_euros(),
        ttc_10 = report.tva_10_breakdown.ttc_cents.to_euros(),
        ht_20 = report.tva_20_breakdown.ht_cents.to_euros(),
        tva_20 = report.tva_20_breakdown.tva_cents.to_euros(),
        ttc_20 = report.tva_20_breakdown.ttc_cents.to_euros(),
        total_tva = report.total_tva_cents().to_euros(),
        hash_display = hash_display,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        journal::{Journal, store::JournalStore},
        types::{
            entry::FiscalEntryData,
            operation::OperationType,
            tva::{TvaBreakdown, TvaRate},
        },
    };
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect("sqlite::memory:")
            .await
            .expect("Pool SQLite");

        // Appliquer les deux migrations
        sqlx::query(include_str!("../../migrations/0001_initial_schema.sql"))
            .execute(&pool).await.expect("Migration 0001");
        sqlx::query(include_str!("../../migrations/0002_z_reports_archives.sql"))
            .execute(&pool).await.expect("Migration 0002");
        sqlx::query(include_str!("../../migrations/0003_sessions_sync.sql"))
            .execute(&pool).await.expect("Migration 0003");
        sqlx::query(include_str!("../../migrations/0004_z_reports_sync.sql"))
            .execute(&pool).await.expect("Migration 0004");
        sqlx::query(include_str!("../../migrations/0005_multi_tva.sql"))
            .execute(&pool).await.expect("Migration 0005");

        pool
    }

    async fn make_journal_with_pool(pool: SqlitePool) -> (Journal, SqlitePool) {
        let pool2 = pool.clone();
        let store = JournalStore { pool };
        let journal = Journal::from_store(store).await.expect("Journal");
        (journal, pool2)
    }

    fn sale(session_id: SessionId, amount: i64) -> FiscalEntryData {
        FiscalEntryData {
            session_id,
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(amount),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: Some("ORD-TEST".to_string()),
        }
    }

    fn refund(session_id: SessionId, amount: i64) -> FiscalEntryData {
        FiscalEntryData {
            session_id,
            operation_type: OperationType::Refund,
            amount_ttc_cents: Cents(amount),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(amount), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: Some("Erreur commande".to_string()),
            order_reference: None,
        }
    }

    // --- Cas nominal ---

    #[tokio::test]
    async fn generate_z_report_nominal() {
        let pool = setup_pool().await;
        let (journal, pool) = make_journal_with_pool(pool).await;

        let session = journal.open_session().await.expect("Session");
        journal.record_transaction(sale(session.id, 1100)).await.expect("Vente 1");
        journal.record_transaction(sale(session.id, 550)).await.expect("Vente 2");
        journal.record_transaction(refund(session.id, -500)).await.expect("Remboursement");
        journal.close_session().await.expect("Clôture");

        let report = generate_z_report(&pool, session.id)
            .await
            .expect("Rapport Z généré");

        assert_eq!(report.session_id, session.id);
        assert_eq!(report.total_sales_cents, Cents(1650));
        assert_eq!(report.total_refunds_cents, Cents(500));
        assert_eq!(report.total_cancels_cents, Cents::ZERO);
        assert_eq!(report.net_revenue(), Cents(1150));
        // 3 ventes/remboursement + 1 ZClose = 4 entrées
        assert_eq!(report.entry_count, 4);
        assert!(report.first_entry_sequence <= report.last_entry_sequence);
    }

    // --- Immuabilité : double génération bloquée ---

    #[tokio::test]
    async fn generate_z_report_twice_fails() {
        let pool = setup_pool().await;
        let (journal, pool) = make_journal_with_pool(pool).await;

        let session = journal.open_session().await.expect("Session");
        journal.record_transaction(sale(session.id, 1000)).await.expect("Vente");
        journal.close_session().await.expect("Clôture");

        generate_z_report(&pool, session.id).await.expect("Première génération");

        let result = generate_z_report(&pool, session.id).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            FiscalError::Session(SessionError::ZReportAlreadyGenerated { .. }) => {}
            other => panic!("Attendu ZReportAlreadyGenerated, obtenu: {other}"),
        }
    }

    // --- Décomposition TVA ---

    #[tokio::test]
    async fn tva_breakdown_aggregated_correctly() {
        let pool = setup_pool().await;
        let (journal, pool) = make_journal_with_pool(pool).await;

        let session = journal.open_session().await.expect("Session");

        // Vente à 10% : 11,00 €
        journal.record_transaction(FiscalEntryData {
            session_id: session.id,
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(1100),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
            tva_5_5_breakdown: TvaBreakdown::zero(TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::from_ttc(Cents(1100), TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: None,
        }).await.expect("Vente 10%");

        // Vente à 5,5% : 10,55 €
        journal.record_transaction(FiscalEntryData {
            session_id: session.id,
            operation_type: OperationType::Sale,
            amount_ttc_cents: Cents(1055),
            tva_breakdown: TvaBreakdown::from_ttc(Cents(1055), TvaRate::Reduit5_5),
            tva_5_5_breakdown: TvaBreakdown::from_ttc(Cents(1055), TvaRate::Reduit5_5),
            tva_10_breakdown: TvaBreakdown::zero(TvaRate::Intermediaire10),
            tva_20_breakdown: TvaBreakdown::zero(TvaRate::Normal20),
            reason: None,
            order_reference: None,
        }).await.expect("Vente 5.5%");

        journal.close_session().await.expect("Clôture");

        let report = generate_z_report(&pool, session.id).await.expect("Rapport Z");

        // TVA 10% doit contenir les données de la première vente
        assert_eq!(report.tva_10_breakdown.ttc_cents, Cents(1100));
        // TVA 5,5% doit contenir les données de la deuxième vente
        assert_eq!(report.tva_5_5_breakdown.ttc_cents, Cents(1055));
        // TVA 20% vide
        assert_eq!(report.tva_20_breakdown.ttc_cents, Cents::ZERO);
    }

    // --- Cohérence interne du rapport ---

    #[tokio::test]
    async fn z_report_internal_consistency_valid() {
        let pool = setup_pool().await;
        let (journal, pool) = make_journal_with_pool(pool).await;

        let session = journal.open_session().await.expect("Session");
        for i in 1..=5_u64 {
            journal.record_transaction(sale(session.id, (i * 110) as i64))
                .await.expect("Vente");
        }
        journal.close_session().await.expect("Clôture");

        let report = generate_z_report(&pool, session.id).await.expect("Rapport Z");
        assert!(report.verify_internal_consistency().is_ok());
    }

    // --- Chargement ---

    #[tokio::test]
    async fn load_z_report_after_generate() {
        let pool = setup_pool().await;
        let (journal, pool) = make_journal_with_pool(pool).await;

        let session = journal.open_session().await.expect("Session");
        journal.record_transaction(sale(session.id, 2000)).await.expect("Vente");
        journal.close_session().await.expect("Clôture");

        let generated = generate_z_report(&pool, session.id).await.expect("Généré");
        let loaded = load_z_report(&pool, session.id).await.expect("Chargement").expect("Existe");

        assert_eq!(generated.id, loaded.id);
        assert_eq!(generated.total_sales_cents, loaded.total_sales_cents);
        assert_eq!(generated.closing_hash, loaded.closing_hash);
    }

    // --- Formatage texte ---

    #[tokio::test]
    async fn format_z_report_text_contains_key_fields() {
        let pool = setup_pool().await;
        let (journal, pool) = make_journal_with_pool(pool).await;

        let session = journal.open_session().await.expect("Session");
        journal.record_transaction(sale(session.id, 10_000)).await.expect("Vente");
        journal.close_session().await.expect("Clôture");

        let report = generate_z_report(&pool, session.id).await.expect("Rapport Z");
        let text = format_z_report_text(&report);

        assert!(text.contains("RAPPORT Z"), "Le texte doit contenir 'RAPPORT Z'");
        assert!(text.contains("100.00"), "Le montant 100,00 € doit apparaître");
        assert!(text.contains("HASH CLOTURE"), "Le hash de clôture doit apparaître");
    }

    // --- Agrégation unitaire (sans base) ---

    #[test]
    fn aggregate_entries_only_sales() {
        use crate::hash_engine::build_entry_for_test;
        let session_bytes = [0xAB; 16];
        let e1 = build_entry_for_test(1, session_bytes, OperationType::Sale,
            1100, TvaRate::Intermediaire10, 0, common::GENESIS_HASH);
        let e2 = build_entry_for_test(2, session_bytes, OperationType::Sale,
            550, TvaRate::Reduit5_5, 1000, e1.hash);
        let entries = vec![e1, e2];
        let agg = aggregate_entries(&entries);
        assert_eq!(agg.total_sales, Cents(1650));
        assert_eq!(agg.total_refunds, Cents::ZERO);
        assert_eq!(agg.tva_10.ttc_cents, Cents(1100));
        assert_eq!(agg.tva_5_5.ttc_cents, Cents(550));
    }

    #[test]
    fn aggregate_entries_with_refund() {
        use crate::hash_engine::build_entry_for_test;
        let session_bytes = [0xAB; 16];
        let e1 = build_entry_for_test(1, session_bytes, OperationType::Sale,
            2000, TvaRate::Intermediaire10, 0, common::GENESIS_HASH);
        let e2 = build_entry_for_test(2, session_bytes, OperationType::Refund,
            -500, TvaRate::Intermediaire10, 1000, e1.hash);
        let entries = vec![e1, e2];
        let agg = aggregate_entries(&entries);
        assert_eq!(agg.total_sales, Cents(2000));
        assert_eq!(agg.total_refunds, Cents(500)); // valeur absolue
    }
}
