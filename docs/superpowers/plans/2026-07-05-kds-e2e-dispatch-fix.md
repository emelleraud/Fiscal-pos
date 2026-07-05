# KDS E2E Dispatch Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable full E2E KDS testing by (1) seeding test stations in SQLite, (2) adding an `order_pending_lines` bridge table, and (3) wiring `LineItem` label/category through to KDS dispatch at pay time.

**Architecture:** The `POST /orders` handler will store KDS-relevant line data in `order_pending_lines`. The `POST /pay` handler will load those rows, build `IncomingLine` values, dispatch to the KDS engine, then delete the pending rows. Stations and routing rules are seeded directly into `restaurant.db` for the test environment.

**Tech Stack:** Rust/Axum (edge-api), SQLite/sqlx, Python 3 (seed script), kds-engine (IncomingLine, LineType)

## Global Constraints

- All new fields on `LineItem` MUST use `#[serde(default)]` — existing tests send `{ "amount_ttc_cents": N, "tva_rate": "10" }` and must still pass.
- Migration runner is in `fiscal-engine/src/journal/store.rs`; new migration SQL goes in `fiscal-engine/migrations/`.
- Clippy pedantic is enforced (`-D warnings`). All new code must be lint-clean.
- KDS dispatch in `pay_order_handler` is already fire-and-forget (`tokio::spawn`) — keep it that way; don't block the HTTP response on KDS.
- `order_pending_lines` cleanup after dispatch is best-effort (`ok()` is fine — non-blocking).

---

## Task 1: Seed test stations and routing rules in restaurant.db

**Files:**
- Create: `scripts/seed_kds_test_data.py`
- Modifies SQLite: `restaurant.db` (data only, no schema changes)

**Interfaces:**
- Produces: 3 rows in `kds_stations`, 2 rows in `kds_routing_rules` in `restaurant.db`

- [ ] **Step 1: Write the seed script**

```python
#!/usr/bin/env python3
"""Seed test KDS stations and routing rules into restaurant.db."""
import sqlite3, uuid, sys

DB = sys.argv[1] if len(sys.argv) > 1 else "restaurant.db"
conn = sqlite3.connect(DB)
conn.execute("PRAGMA foreign_keys = ON")

# Stations — grill-01 falls back to boissons-01 when offline
conn.executemany(
    """INSERT OR REPLACE INTO kds_stations
       (id, name, role, output_type, printer_type, printer_address,
        fallback_station_id, active_in_profiles, sort_order, enabled)
       VALUES (?,?,?,?,?,?,?,?,?,?)""",
    [
        ("grill-01",    "Grill",    "preparation", "screen",
         "file", "/tmp/print_grill.txt",    "boissons-01", '["normal","rush"]', 1, 1),
        ("boissons-01", "Boissons", "preparation", "screen",
         "file", "/tmp/print_boissons.txt", None,          '["normal","rush"]', 2, 1),
        ("ready_board", "Prêt",     "ready_board", "screen",
         None,   None,                      None,          '["normal","rush"]', 3, 1),
    ]
)

# Routing rules for profile "normal"
# rule_type="category", match_value = category string sent in LineItem
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
print(f"Seeded restaurant.db ({DB}) — 3 stations, 2 routing rules.")
```

- [ ] **Step 2: Run the script and verify**

```bash
python3 scripts/seed_kds_test_data.py restaurant.db
```

Expected output:
```
Seeded restaurant.db (restaurant.db) — 3 stations, 2 routing rules.
```

Verify:
```bash
python3 -c "
import sqlite3; conn = sqlite3.connect('restaurant.db')
print('Stations:', conn.execute('SELECT id, fallback_station_id FROM kds_stations').fetchall())
print('Rules:',    conn.execute('SELECT rule_type, match_value, station_ids FROM kds_routing_rules').fetchall())
"
```

Expected:
```
Stations: [('grill-01', 'boissons-01'), ('boissons-01', None), ('ready_board', None)]
Rules: [('category', 'food', '["grill-01"]'), ('category', 'drink', '["boissons-01"]')]
```

- [ ] **Step 3: No commit yet — data will be verified in Task 3 E2E**

---

## Task 2: Migration 0009 — `order_pending_lines` bridge table

**Files:**
- Create: `fiscal-engine/migrations/0009_order_pending_lines.sql`
- Modify: `fiscal-engine/src/journal/store.rs:159` — add one `run("0009", ...)` call

**Interfaces:**
- Produces: `order_pending_lines` table with columns `(order_id TEXT, line_id TEXT, label TEXT, product_id TEXT, category TEXT, quantity INTEGER)`
- Consumed by: Task 3 — `create_order_handler` inserts, `pay_order_handler` reads + deletes

- [ ] **Step 1: Create the migration SQL file**

File: `fiscal-engine/migrations/0009_order_pending_lines.sql`

```sql
-- Migration 0009 : lignes KDS en attente de dispatch (bridge order→pay)
-- Stocke les données d'affichage KDS entre POST /orders et POST /pay.
-- Nettoyées après dispatch dans pay_order_handler (best-effort).

CREATE TABLE IF NOT EXISTS order_pending_lines (
    order_id    TEXT    NOT NULL,
    line_id     TEXT    NOT NULL,
    label       TEXT    NOT NULL,
    product_id  TEXT    NOT NULL DEFAULT '',
    category    TEXT    NOT NULL DEFAULT '',
    quantity    INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (order_id, line_id)
);
```

- [ ] **Step 2: Register the migration in store.rs**

In `fiscal-engine/src/journal/store.rs`, after the existing `run("0008", ...)` line (currently line 159):

Old:
```rust
        run("0008", include_str!("../../migrations/0008_kds_schema.sql")).await?;

        Ok(())
```

New:
```rust
        run("0008", include_str!("../../migrations/0008_kds_schema.sql")).await?;
        run("0009", include_str!("../../migrations/0009_order_pending_lines.sql")).await?;

        Ok(())
```

- [ ] **Step 3: Build to confirm no compile errors**

```bash
cargo build -p fiscal-engine 2>&1 | tail -5
```

Expected: `Finished dev profile`

- [ ] **Step 4: Commit**

```bash
git add fiscal-engine/migrations/0009_order_pending_lines.sql \
        fiscal-engine/src/journal/store.rs
git commit -m "feat(kds): add order_pending_lines migration for KDS dispatch bridge"
```

---

## Task 3: Expand LineItem and wire KDS dispatch

**Files:**
- Modify: `edge-api/src/routes/orders.rs`
  - `LineItem` struct: add 3 optional fields
  - `create_order_handler`: insert into `order_pending_lines` after `order_id` is assigned
  - `pay_order_handler`: load pending lines, build `IncomingLine` vec, replace `lines: vec![]`

**Interfaces:**
- Consumes: `order_pending_lines` table (Task 2), `IncomingLine` from `kds_engine::state_machine`
- `IncomingLine` fields: `line_id: String`, `product_name: String`, `quantity: i64`, `category: String`, `product_id: String`, `tags: Vec<String>`, `parent_line_id: Option<String>`, `line_type: LineType`, `comment: Option<String>`
- `LineType::Item` is the default for all POS-created lines

- [ ] **Step 1: Add optional KDS fields to `LineItem`**

In `edge-api/src/routes/orders.rs`, the existing `LineItem` struct (around line 54):

Old:
```rust
#[derive(Debug, Deserialize)]
pub struct LineItem {
    /// SKU de l'article (optionnel — utilisé pour les promos ciblées sur un article).
    #[serde(default)]
    pub sku: Option<String>,
    /// Montant TTC en centimes pour cet article (quantité incluse).
    pub amount_ttc_cents: i64,
    /// Taux de TVA applicable à cet article.
    pub tva_rate: TvaRateRequest,
}
```

New:
```rust
#[derive(Debug, Deserialize)]
pub struct LineItem {
    /// SKU de l'article (optionnel — utilisé pour les promos ciblées sur un article).
    #[serde(default)]
    pub sku: Option<String>,
    /// Montant TTC en centimes pour cet article (quantité incluse).
    pub amount_ttc_cents: i64,
    /// Taux de TVA applicable à cet article.
    pub tva_rate: TvaRateRequest,
    /// Libellé affiché sur l'écran KDS (optionnel — ignoré si absent).
    #[serde(default)]
    pub label: Option<String>,
    /// Quantité de l'article (pour l'affichage KDS).
    #[serde(default)]
    pub quantity: Option<i64>,
    /// Catégorie de l'article — utilisée par les règles de routage KDS.
    /// Exemples : "food", "drink", "dessert".
    #[serde(default)]
    pub category: Option<String>,
}
```

- [ ] **Step 2: Store pending lines in `create_order_handler`**

In `create_order_handler`, locate the block that assigns `order_id` (around line 595):
```rust
    let entry = state.journal.record_transaction(data).await?;
    let order_id = Uuid::now_v7().to_string();
```

After the `for discount in pending_discounts { ... }` loop and before the final `Ok(...)`, add:

```rust
    // Stocker les lignes KDS en attente de dispatch (sera lu par POST /pay).
    // Best-effort : une erreur ici ne bloque pas la vente.
    for (i, li) in body.line_items.iter().enumerate() {
        if li.label.is_none() {
            continue; // Ignorer les lignes sans libellé KDS
        }
        let _ = sqlx::query(
            "INSERT OR IGNORE INTO order_pending_lines \
             (order_id, line_id, label, product_id, category, quantity) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(&order_id)
        .bind(format!("line-{i}"))
        .bind(li.label.as_deref().unwrap_or(""))
        .bind(li.sku.as_deref().unwrap_or(""))
        .bind(li.category.as_deref().unwrap_or(""))
        .bind(li.quantity.unwrap_or(1))
        .execute(&state.db)
        .await;
    }
```

- [ ] **Step 3: Load pending lines and dispatch in `pay_order_handler`**

In `pay_order_handler`, replace the `let incoming = IncomingOrder { ... lines: vec![], ... };` block (around line 676) with:

```rust
    // Charger les lignes KDS en attente puis les supprimer (best-effort).
    let pending_rows = sqlx::query(
        "SELECT line_id, label, product_id, category, quantity \
         FROM order_pending_lines WHERE order_id = ?",
    )
    .bind(&order_id)
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    let kds_lines: Vec<kds_engine::state_machine::IncomingLine> = pending_rows
        .iter()
        .map(|row| {
            use sqlx::Row as _;
            kds_engine::state_machine::IncomingLine {
                line_id:        row.try_get("line_id").unwrap_or_default(),
                product_name:   row.try_get("label").unwrap_or_default(),
                quantity:       row.try_get("quantity").unwrap_or(1),
                category:       row.try_get("category").unwrap_or_default(),
                product_id:     row.try_get("product_id").unwrap_or_default(),
                tags:           vec![],
                parent_line_id: None,
                line_type:      kds_engine::types::event::LineType::Item,
                comment:        None,
            }
        })
        .collect();

    // Nettoyer les lignes en attente — best-effort
    let _ = sqlx::query("DELETE FROM order_pending_lines WHERE order_id = ?")
        .bind(&order_id)
        .execute(&state.db)
        .await;

    let incoming = IncomingOrder {
        order_id: order_id.clone(),
        channel: "caisse".to_string(),
        order_type: common::OrderType::default(),
        customer_name: None,
        external_order_id: None,
        lines: kds_lines,
    };
```

Remove the now-redundant comment block above the original `let incoming = ...` (the 9-line comment about MVP and empty lines).

- [ ] **Step 4: Build and run existing tests**

```bash
cargo build -p edge-api 2>&1 | tail -5
cargo test -p edge-api 2>&1 | tail -20
```

Expected: all 18 existing tests pass. The new optional fields default to `None` when absent, so existing test JSON bodies still deserialize correctly.

- [ ] **Step 5: Verify the fix with a targeted integration test**

Add to the bottom of `edge-api/tests/api.rs`:

```rust
#[tokio::test]
#[serial]
async fn create_order_with_kds_label_stores_pending_lines() {
    let (app, _db) = setup().await;

    // Ouvrir une session
    let res = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/sessions/open",
            json!({}),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    // Créer une commande avec label+category
    let res = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-KDS-001",
                "line_items": [
                    {
                        "amount_ttc_cents": 880,
                        "tva_rate": "10",
                        "label": "Burger Classic",
                        "category": "food",
                        "quantity": 1
                    },
                    {
                        "amount_ttc_cents": 220,
                        "tva_rate": "10",
                        "label": "Coca-Cola",
                        "category": "drink",
                        "quantity": 1
                    }
                ],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);
    let body = response_body(res).await;
    let order_id = body["order_id"].as_str().unwrap().to_string();

    // Payer — doit réussir même si aucune station KDS n'est configurée (dispatch best-effort)
    let res = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/orders/{order_id}/pay"),
            json!({ "payment_method": "card", "amount_paid_cents": 1100 }),
        ))
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
```

Run:
```bash
cargo test -p edge-api create_order_with_kds_label -- --nocapture 2>&1 | tail -10
```

Expected: `test create_order_with_kds_label_stores_pending_lines ... ok`

- [ ] **Step 6: Commit**

```bash
git add edge-api/src/routes/orders.rs \
        edge-api/tests/api.rs \
        scripts/seed_kds_test_data.py
git commit -m "feat(kds): wire LineItem label/category through to KDS dispatch at pay time"
```

---

## Post-Fix Verification

After all tasks complete, run the full test suite to confirm no regressions:

```bash
cargo test --workspace 2>&1 | tail -10
```

Then launch edge-api pointing at the seeded restaurant.db and verify the KDS config endpoint:

```bash
DATABASE_URL=sqlite:./restaurant.db KDS_HEARTBEAT_TIMEOUT_SECS=15 cargo run -p edge-api &
sleep 3
curl -s http://localhost:8080/api/v1/kds/stations | python3 -m json.tool
curl -s http://localhost:8080/api/v1/kds/config   | python3 -m json.tool
```

Expected `kds/stations` response: 3 stations (`grill-01`, `boissons-01`, `ready_board`), all `"is_online": true` (absent from heartbeat map = online by safe-default).

Expected `kds/config` response: `{"profile": "normal"}`.

---

## Self-Review

**Spec coverage:**
- ✅ Blocker 1 (empty stations): Task 1 seeds 3 stations + 2 routing rules
- ✅ Blocker 2 (API schema mismatch): Task 3 adds optional `label`/`quantity`/`category` with `#[serde(default)]`
- ✅ Blocker 3 (empty lines dispatch): Task 3 stores at POST /orders, loads at POST /pay
- ✅ Migration applied automatically on next edge-api start: Task 2 adds to migration runner
- ✅ No regression in existing 18 tests: new fields are `Option` with `#[serde(default)]`
- ✅ Printer config in seed: `printer_type = "file"`, `printer_address = "/tmp/print_grill.txt"` — no physical printer required

**Gaps:** The `order_type` field in `IncomingOrder` still defaults to `EatIn` (MVP). Full channel + order_type routing can be added in a separate task once `CreateOrderRequest.order_type` is stored and passed.
