# KDS Full Workflow Implementation Plan
# prep → assembly → expo → ORB

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the complete marche en avant kitchen workflow: prep stations print individual tickets on line ack, assembly shows station_statuses badges and prints a complete ticket on bump, a new expo station emits `status='ready'` to the ORB on the first bump and `status='served'` on the second bump.

**Architecture:** Each role (preparation, assembly, expo) gets distinct behavior in `acknowledge()` inside `kds-engine/src/state_machine.rs`. Assembly and expo stations receive the complete order (all lines) via a new Phase 4 in `dispatch_order`. The frontend uses per-page filters so each station type only shows cards it is responsible for.

**Tech Stack:** Rust 2021 (Axum, SQLx, sqlx SQLite), React 19 + TypeScript (Vite, SSE via EventSource), SQLite 3 WAL.

## Global Constraints

- `cargo clippy -p edge-api -- -D warnings` must emit 0 warnings
- `cargo clippy -p kds-engine -- -D warnings` must emit 0 warnings
- `cargo test -p edge-api` — all 24 tests pass
- `cargo test -p kds-engine` — all tests pass
- Clippy pedantic is active; avoid `#[allow]` unless the linting issue is a known false positive
- No `unwrap()` in non-test code; use `?` or `map_or`
- Branch: `feat/kds-app-serve-edge-api`

---

## File Map

**Create:**
- `fiscal-engine/migrations/0010_expo_role.sql` — expands CHECK constraint in kds_stations to include 'expo'
- `kds-app/src/pages/ExpoStation.tsx` — new expo page (2-bump flow, keeps cards until served)

**Modify:**
- `kds-engine/src/migrations.rs` — add `apply_migration` helper, register 0010
- `kds-engine/src/types/station.rs` — add `Expo` variant to `StationRole`
- `kds-engine/src/routing.rs` — add `load_station(pool, id)` public helper
- `kds-engine/src/state_machine.rs` — Phase 4 in `dispatch_order`, full rewrite of `acknowledge()`
- `kds-app/src/types.ts` — add `'expo'` to `StationRole` union type
- `kds-app/src/pages/PreparationStation.tsx` — fix `visibleOrders` filter
- `kds-app/src/components/OrderCard.tsx` — expo button labels, station_statuses for expo, EXPO stage badge
- `kds-app/src/App.tsx` — load station role, route to ExpoStation when role='expo'
- `scripts/seed_kds_test_data.py` — add assembly-01, expo-01 stations

---

## Task 1 — SQL Migration: add 'expo' to kds_stations.role CHECK

**Files:**
- Create: `fiscal-engine/migrations/0010_expo_role.sql`
- Modify: `kds-engine/src/migrations.rs`
- Test: inline in `kds-engine/src/migrations.rs` (existing `#[cfg(test)]` module)

**Interfaces:**
- Produces: kds_stations table accepts `role = 'expo'`; `run_kds_migrations` applies 0008 then 0010 idempotently

---

- [ ] **Step 1: Write migration SQL**

Create `fiscal-engine/migrations/0010_expo_role.sql`:

```sql
-- Migration 0010 : ajoute 'expo' au CHECK de kds_stations.role
-- SQLite ne supporte pas ALTER CHECK — recréation de la table nécessaire.

PRAGMA foreign_keys = OFF;

DROP TABLE IF EXISTS kds_stations_new;

CREATE TABLE kds_stations_new (
    id                  TEXT    NOT NULL PRIMARY KEY,
    name                TEXT    NOT NULL,
    role                TEXT    NOT NULL CHECK (role IN ('preparation','holding','assembly','ready_board','expo')),
    temperature_group   TEXT    CHECK (temperature_group IN ('hot','cold','other')),
    output_type         TEXT    NOT NULL CHECK (output_type IN ('screen','printer','both')),
    printer_address     TEXT,
    printer_type        TEXT    CHECK (printer_type IN ('tcpip','usb','file')),
    printer_mode        TEXT    CHECK (printer_mode IN ('receipt','linerless_label')),
    paper_width_mm      INTEGER CHECK (paper_width_mm IN (50, 80)),
    fallback_station_id TEXT    REFERENCES kds_stations_new(id),
    active_in_profiles  TEXT    NOT NULL DEFAULT '["normal"]',
    sort_order          INTEGER NOT NULL DEFAULT 0,
    enabled             INTEGER NOT NULL DEFAULT 1
);

INSERT INTO kds_stations_new SELECT * FROM kds_stations;

DROP TABLE kds_stations;

ALTER TABLE kds_stations_new RENAME TO kds_stations;

PRAGMA foreign_keys = ON;
```

- [ ] **Step 2: Rewrite `run_kds_migrations` with a generic helper**

Replace `kds-engine/src/migrations.rs` entirely:

```rust
use sqlx::SqlitePool;

use crate::KdsError;

/// Applique les migrations KDS sur le pool fourni.
/// Idempotent via `_applied_migrations` (créée par fiscal-engine).
///
/// # Errors
/// Retourne `KdsError::Database` si une requête SQL échoue.
pub async fn run_kds_migrations(pool: &SqlitePool) -> Result<(), KdsError> {
    let tracking_exists: bool = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='_applied_migrations'",
    )
    .fetch_one(pool)
    .await?
        > 0;

    apply_migration(
        pool,
        tracking_exists,
        "0008",
        include_str!("../migrations/0008_kds_schema.sql"),
    )
    .await?;

    apply_migration(
        pool,
        tracking_exists,
        "0010",
        include_str!("../migrations/0010_expo_role.sql"),
    )
    .await?;

    Ok(())
}

async fn apply_migration(
    pool: &SqlitePool,
    tracking_exists: bool,
    version: &str,
    sql: &str,
) -> Result<(), KdsError> {
    if tracking_exists {
        let already: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM _applied_migrations WHERE version = ?",
        )
        .bind(version)
        .fetch_one(pool)
        .await?;
        if already > 0 {
            return Ok(());
        }
    }

    sqlx::query(sql).execute(pool).await?;

    if tracking_exists {
        sqlx::query("INSERT OR IGNORE INTO _applied_migrations (version) VALUES (?)")
            .bind(version)
            .execute(pool)
            .await?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn in_memory_pool() -> SqlitePool {
        SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("in-memory pool")
    }

    #[tokio::test]
    async fn kds_migration_creates_tables() {
        let pool = in_memory_pool().await;

        sqlx::query("CREATE TABLE IF NOT EXISTS _applied_migrations (version TEXT PRIMARY KEY)")
            .execute(&pool)
            .await
            .unwrap();

        run_kds_migrations(&pool).await.expect("migration ok");

        // Idempotence
        run_kds_migrations(&pool).await.expect("idempotent");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count >= 7, "expected at least 7 kds_ tables, got {count}");

        let v008: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _applied_migrations WHERE version = '0008'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(v008, 1);

        let v010: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM _applied_migrations WHERE version = '0010'")
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(v010, 1);
    }

    #[tokio::test]
    async fn kds_migration_without_tracking_table() {
        let pool = in_memory_pool().await;

        run_kds_migrations(&pool)
            .await
            .expect("migration ok sans table de suivi");

        run_kds_migrations(&pool)
            .await
            .expect("idempotent sans table de suivi");

        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%'",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(count >= 7, "expected at least 7 kds_ tables, got {count}");
    }

    #[tokio::test]
    async fn expo_role_accepted_after_migration() {
        let pool = in_memory_pool().await;
        run_kds_migrations(&pool).await.unwrap();

        sqlx::query(
            "INSERT INTO kds_stations (id, name, role, output_type, active_in_profiles, sort_order, enabled)
             VALUES ('expo-01', 'Expo', 'expo', 'screen', '[\"normal\"]', 5, 1)",
        )
        .execute(&pool)
        .await
        .expect("expo station insert should succeed after 0010 migration");
    }
}
```

- [ ] **Step 3: Run migration tests**

```bash
cargo test -p kds-engine migrations -- --nocapture
```
Expected: 3 tests pass (`kds_migration_creates_tables`, `kds_migration_without_tracking_table`, `expo_role_accepted_after_migration`).

- [ ] **Step 4: Run full kds-engine test suite**

```bash
cargo test -p kds-engine
```
Expected: all existing tests still pass.

- [ ] **Step 5: Run clippy**

```bash
cargo clippy -p kds-engine -- -D warnings
```
Expected: 0 warnings.

- [ ] **Step 6: Commit**

```bash
git add fiscal-engine/migrations/0010_expo_role.sql kds-engine/src/migrations.rs
git commit -m "feat(kds): migration 0010 — add expo role to kds_stations CHECK constraint"
```

---

## Task 2 — StationRole::Expo (Rust + TypeScript)

**Files:**
- Modify: `kds-engine/src/types/station.rs`
- Modify: `kds-app/src/types.ts`

**Interfaces:**
- Produces: `StationRole::Expo` variant in Rust, serializes as `"expo"` via serde snake_case
- Produces: `'expo'` added to TypeScript `StationRole` union

---

- [ ] **Step 1: Add `Expo` to Rust `StationRole` enum**

In `kds-engine/src/types/station.rs`, change the `StationRole` enum:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationRole {
    /// Poste de préparation (cuisson, assemblage partiel).
    Preparation,
    /// Poste de maintien en température.
    Holding,
    /// Poste d'assemblage final du plateau.
    Assembly,
    /// Tableau d'affichage des commandes prêtes (client-facing).
    ReadyBoard,
    /// Poste d'expédition — 1er bump → ORB PRÊT, 2e bump → SERVI.
    Expo,
}
```

- [ ] **Step 2: Add expo serialization tests**

In `kds-engine/src/types/station.rs`, add to the existing `#[cfg(test)]` module:

```rust
#[test]
fn expo_role_serialization() {
    let json = serde_json::to_string(&StationRole::Expo).unwrap();
    assert_eq!(json, r#""expo""#);
}

#[test]
fn expo_role_deserialization() {
    let role: StationRole = serde_json::from_str(r#""expo""#).unwrap();
    assert_eq!(role, StationRole::Expo);
}
```

- [ ] **Step 3: Add 'expo' to TypeScript `StationRole`**

In `kds-app/src/types.ts`, change line 4:

```typescript
export type StationRole = 'preparation' | 'holding' | 'assembly' | 'ready_board' | 'expo'
```

- [ ] **Step 4: Run Rust tests + clippy**

```bash
cargo test -p kds-engine
cargo clippy -p kds-engine -- -D warnings
```
Expected: all tests pass, 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add kds-engine/src/types/station.rs kds-app/src/types.ts
git commit -m "feat(kds): add Expo variant to StationRole (Rust + TypeScript)"
```

---

## Task 3 — `dispatch_order`: Phase 4 auto-dispatch to assembly/expo stations

**Files:**
- Modify: `kds-engine/src/state_machine.rs`

**Interfaces:**
- Consumes: `StationRole::Assembly`, `StationRole::Expo` (from Task 2)
- Produces: assembly and expo stations receive `OrderNew` SSE event with all order lines; station_statuses in the payload contains only preparation stations (not assembly/expo/ready_board)

---

- [ ] **Step 1: Fix `station_statuses` in `build_payload`**

In `kds-engine/src/state_machine.rs`, find `build_payload` (line 264). Change the `station_statuses` line from:

```rust
    let station_statuses = all_stations
        .iter()
        .map(|s| (s.name.clone(), StationStatus::New))
        .collect();
```

to:

```rust
    let station_statuses = all_stations
        .iter()
        .filter(|s| s.role == StationRole::Preparation)
        .map(|s| (s.name.clone(), StationStatus::New))
        .collect();
```

This ensures assembly/expo only see prep station badges (not themselves).

- [ ] **Step 2: Add Phase 4 to `dispatch_order`**

In `kds-engine/src/state_machine.rs`, inside `dispatch_order`, after the Phase 3 loop (after line ~157, just before `Ok(())`), add:

```rust
    // Phase 4 — assembly et expo reçoivent la commande complète (toutes les lignes)
    let all_lines: Vec<&IncomingLine> = order.lines.iter().collect();
    for station in &stations {
        if !matches!(station.role, StationRole::Assembly | StationRole::Expo) {
            continue;
        }
        insert_kds_order(pool, order, station, &order_number_short, now_ms).await?;
        for line in &all_lines {
            insert_kds_line(pool, order, station, line).await?;
        }
        let thresholds = load_thresholds(pool, &station.id).await?;
        let payload = build_payload(
            order,
            station,
            &all_lines,
            &order_number_short,
            now_ms,
            thresholds,
            &stations,
        );
        broadcaster.send(KdsEvent::OrderNew(payload));
    }
```

Also update the existing Phase 3 print condition — currently it prints for `Preparation | Holding`. This is unchanged (printing at dispatch is only for "sans KDS" mode). No modification needed here.

- [ ] **Step 3: Run clippy + tests**

```bash
cargo clippy -p kds-engine -- -D warnings
cargo test -p kds-engine
cargo clippy -p edge-api -- -D warnings
cargo test -p edge-api
```
Expected: 0 warnings, all tests pass.

- [ ] **Step 4: Commit**

```bash
git add kds-engine/src/state_machine.rs
git commit -m "feat(kds): dispatch_order phase 4 — assembly and expo receive complete order"
```

---

## Task 4 — `acknowledge()`: role-aware behavior (print, OrderUpdated, station_statuses)

This is the core backend change. When all lines for a station are acknowledged:
- **Preparation**: print individual product ticket per line ack; on all-acked, recalculate prep station_statuses and broadcast `OrderUpdated { status: 'in_progress', stage: 'preparation' }`
- **Assembly**: on all-acked, print complete order ticket and broadcast `OrderUpdated { status: 'in_progress', stage: 'assembly' }`
- **Expo** (1st bump): on all-acked, broadcast `OrderUpdated { status: 'ready', stage: 'expo' }` — this triggers ORB PRÊT column

The `mark_served()` function (expo 2nd bump) is already correct — no changes needed.

**Files:**
- Modify: `kds-engine/src/routing.rs` — add `load_station` public helper
- Modify: `kds-engine/src/state_machine.rs` — rewrite `acknowledge()`, add private helpers

**Interfaces:**
- Consumes: `routing::load_station(pool, station_id)` → `Result<Option<Station>, KdsError>`
- Produces: `acknowledge()` with role-aware printing and OrderUpdated broadcasts

---

- [ ] **Step 1: Add `load_station` to routing.rs**

In `kds-engine/src/routing.rs`, add after `set_active_profile` (around line 29):

```rust
/// Charge une station par son identifiant.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn load_station(
    pool: &SqlitePool,
    station_id: &str,
) -> Result<Option<Station>, KdsError> {
    let row = sqlx::query_as::<_, StationRow>(
        r"SELECT id, name, role, temperature_group, output_type,
                  printer_address, printer_type, printer_mode, paper_width_mm,
                  fallback_station_id, active_in_profiles, sort_order, enabled
           FROM kds_stations WHERE id = ?",
    )
    .bind(station_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.and_then(|r| {
        let profiles: Vec<String> = serde_json::from_str(&r.active_in_profiles).ok()?;
        Some(Station {
            id: r.id,
            name: r.name,
            role: serde_json::from_str(&format!("\"{}\"", r.role)).ok()?,
            temperature_group: r.temperature_group,
            output_type: serde_json::from_str(&format!("\"{}\"", r.output_type)).ok()?,
            printer_address: r.printer_address,
            printer_type: r
                .printer_type
                .and_then(|t| serde_json::from_str(&format!("\"{t}\"")).ok()),
            printer_mode: r
                .printer_mode
                .and_then(|m| serde_json::from_str(&format!("\"{m}\"")).ok()),
            paper_width_mm: r.paper_width_mm,
            fallback_station_id: r.fallback_station_id,
            active_in_profiles: profiles,
            sort_order: r.sort_order,
            enabled: r.enabled != 0,
        })
    }))
}
```

- [ ] **Step 2: Add private helpers in state_machine.rs**

In `kds-engine/src/state_machine.rs`, add these private async helpers after the `mark_served` function (before `#[cfg(test)]`):

```rust
/// Vérifie si toutes les lignes d'une commande sont acquittées pour une station.
async fn all_lines_acked(
    pool: &SqlitePool,
    order_id: &str,
    station_id: &str,
) -> Result<bool, KdsError> {
    let unacked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kds_order_lines WHERE order_id = ? AND station_id = ? AND acknowledged = 0",
    )
    .bind(order_id)
    .bind(station_id)
    .fetch_one(pool)
    .await?;
    Ok(unacked == 0)
}

/// Charge les statuts des stations de préparation impliquées dans une commande.
/// Utilisé pour mettre à jour les badges sur les écrans assembly et expo.
async fn load_prep_station_statuses(
    pool: &SqlitePool,
    order_id: &str,
) -> Result<HashMap<String, StationStatus>, KdsError> {
    let rows: Vec<(String, String)> = sqlx::query_as(
        r"SELECT s.name,
                 CASE WHEN SUM(CASE WHEN l.acknowledged = 0 THEN 1 ELSE 0 END) = 0
                      THEN 'ready' ELSE 'new' END AS st
          FROM kds_stations s
          INNER JOIN kds_order_lines l ON l.station_id = s.id AND l.order_id = ?
          WHERE s.role = 'preparation'
          GROUP BY s.id, s.name",
    )
    .bind(order_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(name, st)| {
            let status = if st == "ready" {
                StationStatus::Ready
            } else {
                StationStatus::New
            };
            (name, status)
        })
        .collect())
}

/// Charge le numéro court d'une commande pour l'en-tête du ticket.
/// Retourne une chaîne vide si la commande n'existe pas (ne bloque pas l'impression).
async fn load_order_short(
    pool: &SqlitePool,
    order_id: &str,
    station_id: &str,
) -> Result<String, KdsError> {
    Ok(sqlx::query_scalar(
        "SELECT order_number_short FROM kds_orders WHERE order_id = ? AND station_id = ?",
    )
    .bind(order_id)
    .bind(station_id)
    .fetch_optional(pool)
    .await?
    .unwrap_or_default())
}

/// Charge les lignes d'une commande pour impression (station assembly).
async fn load_print_lines(
    pool: &SqlitePool,
    order_id: &str,
    station_id: &str,
) -> Result<Vec<PrintLine>, KdsError> {
    let rows: Vec<(String, i64, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT product_name, quantity, parent_line_id, comment
         FROM kds_order_lines WHERE order_id = ? AND station_id = ? ORDER BY rowid",
    )
    .bind(order_id)
    .bind(station_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(product_name, quantity, parent_line_id, comment)| PrintLine {
            product_name,
            quantity,
            indent: usize::from(parent_line_id.is_some()),
            comment,
        })
        .collect())
}
```

Note: the `KdsError::OrderNotFound` variant may not yet exist. Check `kds-engine/src/errors.rs` — if it's missing, add it (see Step 3).

- [ ] **Step 3: Rewrite `acknowledge()` in state_machine.rs**

Replace the entire `acknowledge` function (from the `pub async fn acknowledge` doc comment through its closing `}`) with:

```rust
/// Acknowledge un bon entier ou une ligne pour une station donnée.
/// Comportement selon le rôle :
/// - Préparation + ligne individuelle → impression du ticket produit si imprimante configurée
/// - Préparation + toutes lignes ackées → OrderUpdated (in_progress/preparation + station_statuses recalculés)
/// - Assembly + toutes lignes ackées → impression ticket complet + OrderUpdated (in_progress/assembly)
/// - Expo + toutes lignes ackées → OrderUpdated (ready/expo) → ORB passe en colonne PRÊT
///
/// # Errors
/// Retourne `KdsError::Database` si une mise à jour échoue.
pub async fn acknowledge(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order_id: &str,
    station_id: &str,
    line_id: Option<&str>,
) -> Result<(), KdsError> {
    let now_ms = now_ms();

    // Charger la station pour connaître son rôle et sa config imprimante
    let station = routing::load_station(pool, station_id).await?;

    if let Some(lid) = line_id {
        // Ack d'une ligne individuelle
        sqlx::query(
            "UPDATE kds_order_lines SET acknowledged = 1, acknowledged_at = ?
             WHERE order_id = ? AND line_id = ? AND station_id = ?",
        )
        .bind(now_ms)
        .bind(order_id)
        .bind(lid)
        .bind(station_id)
        .execute(pool)
        .await?;

        // Impression ticket produit individuel (mode "avec KDS" sur station préparation)
        if let Some(ref s) = station {
            if s.role == StationRole::Preparation {
                if let Some(ref addr) = s.printer_address {
                    let row: Option<(String, i64, Option<String>)> = sqlx::query_as(
                        "SELECT product_name, quantity, comment FROM kds_order_lines
                         WHERE order_id = ? AND line_id = ? AND station_id = ?",
                    )
                    .bind(order_id)
                    .bind(lid)
                    .bind(station_id)
                    .fetch_optional(pool)
                    .await?;

                    if let Some((product_name, quantity, comment)) = row {
                        if let Ok(short) = load_order_short(pool, order_id, station_id).await {
                            tokio::spawn(crate::printer::print_order(
                                s.clone(),
                                addr.clone(),
                                order_id.to_string(),
                                short,
                                vec![PrintLine {
                                    product_name,
                                    quantity,
                                    indent: 0,
                                    comment,
                                }],
                            ));
                        }
                    }
                }
            }
        }
    } else {
        // Ack de toutes les lignes de la station (bump en-tête)
        sqlx::query(
            "UPDATE kds_order_lines SET acknowledged = 1, acknowledged_at = ?
             WHERE order_id = ? AND station_id = ?",
        )
        .bind(now_ms)
        .bind(order_id)
        .bind(station_id)
        .execute(pool)
        .await?;

        sqlx::query(
            "UPDATE kds_orders SET status = 'ready', first_bump_at = COALESCE(first_bump_at, ?)
             WHERE order_id = ? AND station_id = ?",
        )
        .bind(now_ms)
        .bind(order_id)
        .bind(station_id)
        .execute(pool)
        .await?;
    }

    // Vérifier si toutes les lignes sont maintenant ackées (vrai pour bump-all, peut l'être après la dernière ligne individuelle)
    let acked = all_lines_acked(pool, order_id, station_id).await?;

    if acked {
        // Mettre à jour kds_orders si on est passé par la voie ligne individuelle
        if line_id.is_some() {
            sqlx::query(
                "UPDATE kds_orders SET status = 'ready', first_bump_at = COALESCE(first_bump_at, ?)
                 WHERE order_id = ? AND station_id = ?",
            )
            .bind(now_ms)
            .bind(order_id)
            .bind(station_id)
            .execute(pool)
            .await?;
        }

        match station.as_ref().map(|s| &s.role) {
            Some(StationRole::Preparation) => {
                let station_statuses = load_prep_station_statuses(pool, order_id).await?;
                broadcaster.send(KdsEvent::OrderUpdated(
                    crate::types::event::KdsOrderUpdate {
                        order_id: order_id.to_string(),
                        status: "in_progress".to_string(),
                        stage: "preparation".to_string(),
                        station_statuses,
                    },
                ));
            }
            Some(StationRole::Assembly) => {
                if let Some(ref s) = station {
                    if let Some(ref addr) = s.printer_address {
                        let lines = load_print_lines(pool, order_id, station_id).await?;
                        if let Ok(short) = load_order_short(pool, order_id, station_id).await {
                            tokio::spawn(crate::printer::print_order(
                                s.clone(),
                                addr.clone(),
                                order_id.to_string(),
                                short,
                                lines,
                            ));
                        }
                    }
                }
                // Conserver les station_statuses de préparation pour l'expo
                let station_statuses = load_prep_station_statuses(pool, order_id).await?;
                broadcaster.send(KdsEvent::OrderUpdated(
                    crate::types::event::KdsOrderUpdate {
                        order_id: order_id.to_string(),
                        status: "in_progress".to_string(),
                        stage: "assembly".to_string(),
                        station_statuses,
                    },
                ));
            }
            Some(StationRole::Expo) => {
                // À ce stade toutes les preps devraient être 'ready'
                let station_statuses = load_prep_station_statuses(pool, order_id).await?;
                broadcaster.send(KdsEvent::OrderUpdated(
                    crate::types::event::KdsOrderUpdate {
                        order_id: order_id.to_string(),
                        status: "ready".to_string(),
                        stage: "expo".to_string(),
                        station_statuses,
                    },
                ));
            }
            _ => {}
        }
    }

    // Toujours émettre OrderAcked (mise à jour des lignes dans le frontend)
    broadcaster.send(KdsEvent::OrderAcked(
        crate::types::event::KdsAckPayload {
            order_id: order_id.to_string(),
            station_id: station_id.to_string(),
            line_id: line_id.map(str::to_string),
        },
    ));

    Ok(())
}
```

- [ ] **Step 4: Verify `routing` import in state_machine.rs**

Ensure the top of `kds-engine/src/state_machine.rs` already has `use crate::routing;` — if not, add it to the existing `use crate::{...}` block.

- [ ] **Step 5: Run clippy + tests**

```bash
cargo clippy -p kds-engine -- -D warnings
cargo test -p kds-engine
cargo clippy -p edge-api -- -D warnings
cargo test -p edge-api
```
Expected: 0 warnings, all tests pass. Fix any clippy issues before continuing.

- [ ] **Step 6: Commit**

```bash
git add kds-engine/src/routing.rs kds-engine/src/state_machine.rs
git commit -m "feat(kds): role-aware acknowledge — prep prints product ticket, assembly prints full ticket, expo signals ORB"
```

---

## Task 5 — Frontend: PreparationStation filter + OrderCard expo support + ExpoStation + App routing

**Files:**
- Modify: `kds-app/src/pages/PreparationStation.tsx`
- Modify: `kds-app/src/components/OrderCard.tsx`
- Create: `kds-app/src/pages/ExpoStation.tsx`
- Modify: `kds-app/src/App.tsx`

**Interfaces:**
- Consumes: `StationRole = ... | 'expo'` (from Task 2)
- Produces:
  - `PreparationStation`: hides cards when all lines acked (works for prep and assembly roles)
  - `ExpoStation`: keeps cards until `status === 'served'`, shows 2-state button (EXPO PRÊT → / LIVRÉ ✓)
  - `App`: routes expo stationId to ExpoStation

---

- [ ] **Step 1: Fix `visibleOrders` filter in PreparationStation.tsx**

In `kds-app/src/pages/PreparationStation.tsx`, change line 33:

```tsx
  const visibleOrders = orders.filter(
    (o) => o.status !== 'served' && !o.lines.every((l) => l.acknowledged),
  )
```

- [ ] **Step 2: Update OrderCard for expo role**

In `kds-app/src/components/OrderCard.tsx`:

**2a.** Add `expo: 'EXPO'` to `STAGE_BADGE` (line ~16):
```tsx
const STAGE_BADGE: Record<string, string> = {
  preparation: 'PREP',
  holding: 'HOLD',
  assembly: 'ASSE',
  expo: 'EXPO',
}
```

**2b.** Update the station_statuses section (line ~138) to also show for expo role:
```tsx
      {(role === 'assembly' || role === 'expo') && Object.keys(order.station_statuses).length > 0 && (
```

**2c.** Update the footer buttons section. Replace the existing `role === 'assembly' ? ... : ...` block with:

```tsx
          {(role === 'assembly' || role === 'expo') ? (
            <>
              {!allAcked && (
                <button
                  onClick={handleBumpAll}
                  disabled={bumping}
                  className="text-xs bg-blue-600 hover:bg-blue-700 text-white px-2 py-1 rounded disabled:opacity-50"
                >
                  {role === 'expo' ? 'EXPO PRÊT →' : 'PRÊT →'}
                </button>
              )}
              {allAcked && (
                <button
                  onClick={handleServed}
                  disabled={bumping}
                  className="text-xs bg-green-600 hover:bg-green-700 text-white px-2 py-1 rounded disabled:opacity-50"
                >
                  {role === 'expo' ? 'LIVRÉ ✓' : 'SERVI ✓'}
                </button>
              )}
            </>
          ) : (
            <button
              onClick={handleBumpAll}
              disabled={bumping || allAcked}
              className="text-xs bg-blue-600 hover:bg-blue-700 text-white px-2 py-1 rounded disabled:opacity-50"
            >
              TOUT ✓
            </button>
          )}
```

- [ ] **Step 3: Create ExpoStation.tsx**

Create `kds-app/src/pages/ExpoStation.tsx`:

```tsx
import { useEffect, useState } from 'react'
import { getStations } from '../api'
import ConnectionBanner from '../components/ConnectionBanner'
import OrderCard from '../components/OrderCard'
import { useKdsFeed } from '../hooks/useKdsFeed'
import type { Station } from '../types'

interface Props {
  stationId: string
}

export default function ExpoStation({ stationId }: Props) {
  const { orders, connected } = useKdsFeed(stationId)
  const [station, setStation] = useState<Station | null>(null)
  const [stationError, setStationError] = useState<string | null>(null)
  const isFullscreen = new URLSearchParams(window.location.search).get('fullscreen') === 'true'

  useEffect(() => {
    getStations()
      .then((list) => {
        const found = list.find((s) => s.id === stationId) ?? null
        setStation(found)
        if (!found) setStationError(`Station "${stationId}" introuvable`)
      })
      .catch((e: unknown) => {
        setStationError(e instanceof Error ? e.message : 'Erreur API stations')
      })
  }, [stationId])

  const stationName = station?.name ?? stationId

  // Expo garde les cartes jusqu'au 2e bump (status='served')
  // allAcked=true après le 1er bump → bouton bascule sur LIVRÉ ✓
  const visibleOrders = orders.filter((o) => o.status !== 'served')

  return (
    <div className={`min-h-screen bg-gray-900 text-white flex flex-col ${isFullscreen ? '' : 'p-4'}`}>
      <ConnectionBanner connected={connected} />

      <div className="flex items-center gap-3 mb-4 px-2 pt-2">
        <h1 className="text-2xl font-bold tracking-wide">{stationName.toUpperCase()}</h1>
        <span className="text-xs bg-purple-700 rounded px-2 py-1 uppercase">EXPO</span>
        <span className="ml-auto text-sm text-gray-400">
          {visibleOrders.length} commande{visibleOrders.length !== 1 ? 's' : ''}
        </span>
      </div>

      {stationError && (
        <div className="mx-2 mb-4 p-3 bg-yellow-900 border border-yellow-600 rounded text-yellow-200 text-sm">
          ⚠ {stationError}
        </div>
      )}

      <div className="flex gap-3 overflow-x-auto pb-4 px-2 flex-1 items-start">
        {visibleOrders.length === 0 ? (
          <div className="flex-1 flex items-center justify-center text-gray-500 text-xl min-h-64">
            Aucune commande à expédier
          </div>
        ) : (
          visibleOrders.map((order) => (
            <OrderCard
              key={order.order_id}
              order={order}
              stationId={stationId}
              role="expo"
            />
          ))
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Update App.tsx to route expo stationId → ExpoStation**

Replace `kds-app/src/App.tsx` entirely:

```tsx
import { useEffect, useState } from 'react'
import { getStations } from './api'
import ConfigPage from './pages/ConfigPage'
import ExpoStation from './pages/ExpoStation'
import OrderReadyBoard from './pages/OrderReadyBoard'
import PreparationStation from './pages/PreparationStation'

function getStationId(): string {
  const segments = window.location.pathname.split('/').filter(Boolean)
  return segments[segments.length - 1] ?? ''
}

export default function App() {
  const stationId = getStationId()
  const params = new URLSearchParams(window.location.search)
  const orbType = params.get('orb') as 'client' | 'livreur' | null

  const [stationRole, setStationRole] = useState<string | null>(null)
  const [roleLoaded, setRoleLoaded] = useState(false)

  useEffect(() => {
    if (!stationId || stationId === 'config' || stationId === 'ready_board') {
      setRoleLoaded(true)
      return
    }
    getStations()
      .then((list) => {
        setStationRole(list.find((s) => s.id === stationId)?.role ?? null)
      })
      .catch(() => {})
      .finally(() => setRoleLoaded(true))
  }, [stationId])

  if (stationId === 'config') return <ConfigPage />
  if (stationId === 'ready_board') return <OrderReadyBoard orbType={orbType} />

  if (!stationId) {
    return (
      <div className="min-h-screen bg-gray-900 text-white flex items-center justify-center">
        <div className="text-center">
          <p className="text-2xl mb-4 text-gray-400">KDS — Aucune station configurée</p>
          <p className="text-sm text-gray-600">
            Accéder à{' '}
            <code className="bg-gray-800 px-2 py-1 rounded">/kds/[station_id]</code>
          </p>
        </div>
      </div>
    )
  }

  if (!roleLoaded) return null

  if (stationRole === 'expo') return <ExpoStation stationId={stationId} />
  return <PreparationStation stationId={stationId} />
}
```

- [ ] **Step 5: Run TypeScript type-check**

```bash
cd kds-app && npx tsc --noEmit
```
Expected: 0 errors.

- [ ] **Step 6: Commit**

```bash
git add kds-app/src/pages/PreparationStation.tsx kds-app/src/components/OrderCard.tsx kds-app/src/pages/ExpoStation.tsx kds-app/src/App.tsx
git commit -m "feat(kds-app): expo station component, prep filter fix, expo buttons in OrderCard"
```

---

## Task 6 — Seed data: assembly-01 and expo-01

**Files:**
- Modify: `scripts/seed_kds_test_data.py`

**Interfaces:**
- Produces: 5 stations in restaurant.db: grill-01, boissons-01, assembly-01, expo-01, ready_board
- Produces: 2 routing rules (food → grill-01, drink → boissons-01)
- Assembly/expo receive the complete order via Phase 4 in dispatch_order — no routing rules needed for them

---

- [ ] **Step 1: Update seed_kds_test_data.py**

Replace the file:

```python
#!/usr/bin/env python3
"""Seed test KDS stations and routing rules into restaurant.db."""
import sqlite3, uuid, sys

DB = sys.argv[1] if len(sys.argv) > 1 else "restaurant.db"
conn = sqlite3.connect(DB)
conn.execute("PRAGMA foreign_keys = ON")

# Stations
# Insert order: boissons-01 first (no fallback), then grill-01 (depends on boissons-01),
# then assembly-01, expo-01, ready_board
conn.executemany(
    """INSERT OR REPLACE INTO kds_stations
       (id, name, role, output_type, printer_type, printer_address,
        fallback_station_id, active_in_profiles, sort_order, enabled)
       VALUES (?,?,?,?,?,?,?,?,?,?)""",
    [
        ("boissons-01", "Boissons",  "preparation", "screen",
         "file", "/tmp/print_boissons.txt", None,          '["normal","rush"]', 2, 1),
        ("grill-01",    "Grill",     "preparation", "screen",
         "file", "/tmp/print_grill.txt",    "boissons-01", '["normal","rush"]', 1, 1),
        ("assembly-01", "Assembly",  "assembly",    "screen",
         "file", "/tmp/print_assembly.txt", None,          '["normal","rush"]', 3, 1),
        ("expo-01",     "Expo",      "expo",        "screen",
         None,   None,                      None,          '["normal","rush"]', 4, 1),
        ("ready_board", "Prêt",      "ready_board", "screen",
         None,   None,                      None,          '["normal","rush"]', 5, 1),
    ]
)

# Routing rules for profile "normal"
# assembly and expo receive all orders via Phase 4 in dispatch_order (no routing rules needed)
conn.executemany(
    """INSERT OR REPLACE INTO kds_routing_rules
       (id, profile_id, rule_type, match_value, station_ids, priority)
       VALUES (?,?,?,?,?,?)""",
    [
        (str(uuid.uuid4()), "normal", "category", "food",  '["grill-01"]',    10),
        (str(uuid.uuid4()), "normal", "category", "drink", '["boissons-01"]', 10),
    ]
)

conn.commit()
conn.close()
print(f"Seeded {DB} — 5 stations (grill-01, boissons-01, assembly-01, expo-01, ready_board), 2 routing rules.")
```

- [ ] **Step 2: Re-seed the local DB**

First ensure edge-api is stopped (or the DB is not locked). Then:

```bash
# Delete old seeded data (stations + routing rules)
sqlite3 restaurant.db "DELETE FROM kds_routing_rules; DELETE FROM kds_stations;"
python3 scripts/seed_kds_test_data.py
```

Expected output: `Seeded restaurant.db — 5 stations (grill-01, boissons-01, assembly-01, expo-01, ready_board), 2 routing rules.`

Verify:
```bash
sqlite3 restaurant.db "SELECT id, role FROM kds_stations ORDER BY sort_order;"
```
Expected:
```
grill-01|preparation
boissons-01|preparation
assembly-01|assembly
expo-01|expo
ready_board|ready_board
```

- [ ] **Step 3: Commit**

```bash
git add scripts/seed_kds_test_data.py
git commit -m "feat(kds): seed data — add assembly-01 and expo-01 stations"
```

---

## E2E Verification

After all tasks are complete, run this sequence manually to verify the full workflow.

### Setup

```bash
# Terminal 1 — edge-api
DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api

# Terminal 2 — kds-app
cd kds-app && npm run dev

# Terminal 3 — SSE monitor (optional)
curl -N http://localhost:8080/api/v1/kds/feed/ready_board
```

Open browser tabs:
- `http://localhost:5174/kds/grill-01` — Grill (prep)
- `http://localhost:5174/kds/boissons-01` — Boissons (prep)
- `http://localhost:5174/kds/assembly-01` — Assembly
- `http://localhost:5174/kds/expo-01` — Expo (purple EXPO badge)
- `http://localhost:5174/kds/ready_board` — ORB

### Scenario

```bash
# 1. Ouvrir une session
curl -s -X POST http://localhost:8080/api/v1/sessions/open \
  -H "Content-Type: application/json" -d '{"operator": "test"}'

# 2. Créer + payer une commande
ORDER_ID=$(curl -s -X POST http://localhost:8080/api/v1/orders \
  -H "Content-Type: application/json" \
  -d '{
    "order_reference": "ORD-E2E-001",
    "payment_method": "cash",
    "line_items": [
      {"amount_ttc_cents": 880, "tva_rate": "10", "label": "Burger", "category": "food", "quantity": 1},
      {"amount_ttc_cents": 220, "tva_rate": "10", "label": "Coca",   "category": "drink", "quantity": 1}
    ]
  }' | python3 -c "import sys,json; print(json.load(sys.stdin)['id'])")

curl -s -X POST http://localhost:8080/api/v1/orders/$ORDER_ID/pay \
  -H "Content-Type: application/json" \
  -d '{"payment_method": "cash", "amount_paid_cents": 1100}'

echo "Order ID: $ORDER_ID"
```

**Checkpoints:**
1. `grill-01` shows Burger card — `boissons-01` shows Coca card ✓
2. `assembly-01` shows complete order (Burger + Coca) with badges grill-01 ⬜ boissons-01 ⬜ ✓
3. `expo-01` shows complete order with same badges ✓
4. ORB shows order in "EN PRÉPARATION" ✓

```bash
# 3. Bump Burger on grill-01 (individual line ack)
BURGER_LINE=$(sqlite3 restaurant.db "SELECT line_id FROM kds_order_lines WHERE order_id='$ORDER_ID' AND station_id='grill-01' AND product_name='Burger';")
curl -s -X POST http://localhost:8080/api/v1/kds/orders/$ORDER_ID/ack \
  -H "Content-Type: application/json" \
  -d "{\"station_id\": \"grill-01\", \"line_id\": \"$BURGER_LINE\"}"
```

**Checkpoints:**
5. Burger line strikethrough on grill-01 card ✓
6. `/tmp/print_grill.txt` created with Burger ticket ✓ (check: `cat /tmp/print_grill.txt`)
7. `assembly-01` badge grill-01 → ✅ ready ✓
8. ORB still "EN PRÉPARATION" ✓

```bash
# 4. Bump Coca on boissons-01 (last line → card disappears)
COCA_LINE=$(sqlite3 restaurant.db "SELECT line_id FROM kds_order_lines WHERE order_id='$ORDER_ID' AND station_id='boissons-01' AND product_name='Coca';")
curl -s -X POST http://localhost:8080/api/v1/kds/orders/$ORDER_ID/ack \
  -H "Content-Type: application/json" \
  -d "{\"station_id\": \"boissons-01\", \"line_id\": \"$COCA_LINE\"}"
```

**Checkpoints:**
9. Boissons card disappears ✓ (all lines acked)
10. Grill card disappears ✓ (already all acked from step 5 → but wait, Burger was the only line; step 5 already had all lines acked for grill-01)

Wait — actually after step 5 (individual Burger line ack on grill-01), if grill-01 only had one line (Burger), then `all_lines_acked` returns true immediately. The OrderUpdated with `status='in_progress'` is broadcast, `applyOrderUpdated` updates the order status to 'in_progress'. PreparationStation filter: `status !== 'served' && !allLinesAcked` — `allLinesAcked` is true → card disappears. ✓

After step 6 (Coca ack on boissons-01), same thing: all lines acked, card disappears. ✓

```bash
# 5. Bump assembly-01 (PRÊT → button)
curl -s -X POST http://localhost:8080/api/v1/kds/orders/$ORDER_ID/ack \
  -H "Content-Type: application/json" \
  -d '{"station_id": "assembly-01"}'
```

**Checkpoints:**
11. Assembly card disappears ✓ (all lines acked → filter hides it)
12. `/tmp/print_assembly.txt` created with complete ticket (Burger + Coca) ✓
13. expo-01 order still visible (status='in_progress' != 'served') ✓
14. ORB still "EN PRÉPARATION" ✓

```bash
# 6. Expo 1st bump (EXPO PRÊT → button)
curl -s -X POST http://localhost:8080/api/v1/kds/orders/$ORDER_ID/ack \
  -H "Content-Type: application/json" \
  -d '{"station_id": "expo-01"}'
```

**Checkpoints:**
15. expo-01 card shows "LIVRÉ ✓" button (allAcked=true after OrderAcked event) ✓
16. ORB shows order in "PRÊT ✓" column ✓ (status='ready')

```bash
# 7. Expo 2nd bump (LIVRÉ ✓ button)
curl -s -X POST http://localhost:8080/api/v1/kds/orders/$ORDER_ID/served \
  -H "Content-Type: application/json" \
  -d '{"station_id": "expo-01"}'
```

**Checkpoints:**
17. expo-01 card disappears ✓ (status='served')
18. ORB order disappears from both columns ✓

---

## Appendix — Clippy Pedantic Common Fixes

| Pattern that triggers warning | Fix |
|---|---|
| `match option { Some(x) => ..., None => {} }` | `if let Some(x) = option { ... }` |
| `match bool { true => ..., false => {} }` | `if bool { ... }` |
| `vec.iter().map(...).collect::<HashMap>()` | Add `#[allow(clippy::...)]` only if false positive; otherwise refactor |
| unused `Result` from `tokio::spawn` | Assign to `let _` |
| `format!("\"{}\"", x)` | This is intentional (serde JSON); a `#[allow]` is acceptable here if clippy flags it |
| `.unwrap_or_default()` on `String` | Replace with `.unwrap_or_else(String::new)` if clippy pedantic flags it |

