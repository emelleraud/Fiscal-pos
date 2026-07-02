# KDS Backend — Implementation Plan (Plan 1/3)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ajouter le moteur KDS (Kitchen Display System) au backend — crate `kds-engine`, migrations SQLite, routes SSE + acknowledge, hook dans la route `/pay`, formatteur ESC/POS, dispatcher TCP/IP + file, binaire `kds-print-agent`.

**Architecture:** Nouveau crate `kds-engine` dans le workspace Rust, importé par `edge-api` comme dépendance. Un broadcaster `tokio::sync::broadcast` diffuse les événements vers les handlers SSE (< 10 ms LAN). L'impression est dispatché en parallèle via TCP/IP direct ou fichier texte (mode test).

**Tech Stack:** Rust/Axum 0.7, SQLite WAL (sqlx 0.8), tokio 1.38, `tokio-stream` (BroadcastStream), SSE via `axum::response::sse`, ESC/POS raw bytes (pas de crate tiers).

**Plans suivants :**
- Plan 2 — `kds-app` (React/Vite, SSE consumer, UI stations + ORB)
- Plan 3 — Backoffice cuisine + Supabase migrations + sync-client pull

**Spec de référence :** `docs/superpowers/specs/2026-07-02-kds-design.md`

**CI avant chaque commit :** `cargo fmt --check && cargo clippy --workspace -- -D warnings && cargo test --workspace`

---

## Fichiers créés / modifiés

```
kds-engine/                          ← nouveau crate workspace
  Cargo.toml
  src/
    lib.rs
    errors.rs
    types/
      mod.rs
      order_type.rs                  ← OrderType enum (eat_in | takeaway | …)
      station.rs                     ← Station, Role, OutputType, PrinterMode
      event.rs                       ← KdsEvent, KdsOrderPayload, KdsLine
      routing.rs                     ← RoutingRule, RoutingProfile
    routing.rs                       ← route_order() : order → Vec<station_id>
    state_machine.rs                 ← transitions new→in_progress→…→served
    broadcaster.rs                   ← KdsBroadcaster wrapping broadcast::Sender
    migrations.rs                    ← run_kds_migrations() idempotent
    formatter/
      mod.rs
      receipt.rs                     ← format_receipt() → Vec<u8> ESC/POS
      linerless.rs                   ← format_linerless() → Vec<u8>
      file.rs                        ← format_file() → String (WYSIWYG)
    printer.rs                       ← PrintDispatcher : tcpip | file | failover

common/src/lib.rs                    ← + OrderType re-export (ou dans kds-engine)

edge-api/
  Cargo.toml                         ← + kds-engine, tokio-stream, futures-util
  src/
    app.rs                           ← AppState + kds: Arc<KdsEngine>
    routes/
      mod.rs                         ← + kds module
      kds.rs                         ← routes SSE, ack, served, config
    routes/orders.rs                 ← hook pay → kds_engine.on_payment()
  migrations/
    0007_order_type.sql              ← ALTER TABLE orders ADD COLUMN order_type
    0008_kds_schema.sql              ← toutes les tables KDS

kds-print-agent/                     ← nouveau binaire workspace
  Cargo.toml
  src/
    main.rs                          ← serveur Axum port 6611, POST /print
```

---

## Task 1 : Ajout de `order_type` dans `common` + migration edge-api

**Files:**
- Modify: `common/src/lib.rs`
- Create: `edge-api/migrations/0007_order_type.sql`
- Modify: `edge-api/src/routes/orders.rs` (struct `CreateOrderRequest`)

- [ ] **Écrire la migration SQLite**

`edge-api/migrations/0007_order_type.sql` :
```sql
-- Migration 0007 : order_type pour le routage KDS
ALTER TABLE orders ADD COLUMN order_type TEXT NOT NULL DEFAULT 'eat_in'
    CHECK (order_type IN ('eat_in','takeaway','click_and_collect','delivery','drive'));
```

- [ ] **Ajouter `OrderType` dans `common/src/lib.rs`**

Ajouter après les identifiants existants :
```rust
/// Type de commande — détermine l'ORB cible et les règles de routage KDS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    #[default]
    EatIn,
    Takeaway,
    ClickAndCollect,
    Delivery,
    Drive,
}

impl OrderType {
    /// Retourne la valeur TEXT pour SQLite.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EatIn => "eat_in",
            Self::Takeaway => "takeaway",
            Self::ClickAndCollect => "click_and_collect",
            Self::Delivery => "delivery",
            Self::Drive => "drive",
        }
    }
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
```

- [ ] **Ajouter `order_type` dans `CreateOrderRequest` (orders.rs)**

Dans `edge-api/src/routes/orders.rs`, trouver `struct CreateOrderRequest` et ajouter :
```rust
#[serde(default)]
pub order_type: common::OrderType,
```

- [ ] **Appliquer la migration dans `store.rs`**

Dans `fiscal-engine/src/journal/store.rs`, dans `run_migrations()`, après le dernier bloc `run("0006", ...)`, ajouter :
```rust
run(
    "0007",
    include_str!("../../migrations/0007_order_type.sql"),
)
.await?;
```

> Note : la migration 0007 vit dans `edge-api/migrations/` mais est référencée depuis `fiscal-engine/src/journal/store.rs` via un chemin relatif. Si le chemin ne compile pas, déplacer le fichier dans `fiscal-engine/migrations/0007_order_type.sql` à la place.

- [ ] **Vérifier que les tests compilent et passent**
```bash
cargo test --workspace 2>&1 | tail -5
```
Attendu : tous les tests passent, aucun warning clippy.

- [ ] **Commit**
```bash
git add common/src/lib.rs edge-api/src/routes/orders.rs fiscal-engine/src/journal/store.rs fiscal-engine/migrations/0007_order_type.sql
git commit -m "feat(common): OrderType enum + migration 0007 order_type"
```

---

## Task 2 : Scaffold du crate `kds-engine`

**Files:**
- Create: `kds-engine/Cargo.toml`
- Create: `kds-engine/src/lib.rs`
- Create: `kds-engine/src/errors.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Ajouter `kds-engine` au workspace**

Dans `Cargo.toml` (racine), dans `[workspace] members` :
```toml
members = [
    "fiscal-engine",
    "promo-engine",
    "edge-api",
    "sync-client",
    "common",
    "kds-engine",
    "kds-print-agent",
]
```

Ajouter dans `[workspace.dependencies]` :
```toml
tokio-stream = { version = "0.1", features = ["sync"] }
futures-util = "0.3"
```

- [ ] **Créer `kds-engine/Cargo.toml`**
```toml
[package]
name = "kds-engine"
version = "0.1.0"
edition = "2021"

[dependencies]
common = { path = "../common" }
serde = { workspace = true }
serde_json = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true }
sqlx = { workspace = true }
uuid = { workspace = true }
time = { workspace = true }
tracing = { workspace = true }

[dev-dependencies]
tokio = { workspace = true }
tempfile = { workspace = true }
sqlx = { workspace = true }
```

- [ ] **Créer `kds-engine/src/errors.rs`**
```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum KdsError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("print connect error: {0}")]
    PrintConnect(std::io::Error),
    #[error("print write error: {0}")]
    PrintWrite(std::io::Error),
    #[error("print file error: {0}")]
    PrintFile(std::io::Error),
    #[error("station not found: {0}")]
    StationNotFound(String),
    #[error("no active profile configured")]
    NoActiveProfile,
}
```

- [ ] **Créer `kds-engine/src/lib.rs`**
```rust
#![deny(clippy::all, clippy::pedantic)]

pub mod broadcaster;
pub mod errors;
pub mod formatter;
pub mod migrations;
pub mod printer;
pub mod routing;
pub mod state_machine;
pub mod types;

pub use errors::KdsError;
```

- [ ] **Créer `kds-engine/src/types/mod.rs`**
```rust
pub mod event;
pub mod order_type;
pub mod routing;
pub mod station;
```

- [ ] **Vérifier que le workspace compile**
```bash
cargo build -p kds-engine 2>&1 | tail -10
```
Attendu : `Compiling kds-engine v0.1.0` sans erreur.

- [ ] **Commit**
```bash
git add kds-engine/ Cargo.toml
git commit -m "feat(kds-engine): scaffold crate — errors, lib, types modules"
```

---

## Task 3 : Types KDS (`station`, `event`, `routing`, `order_type`)

**Files:**
- Create: `kds-engine/src/types/order_type.rs`
- Create: `kds-engine/src/types/station.rs`
- Create: `kds-engine/src/types/event.rs`
- Create: `kds-engine/src/types/routing.rs`

- [ ] **Créer `kds-engine/src/types/order_type.rs`**
```rust
use serde::{Deserialize, Serialize};

/// Type de commande — détermine le routage ORB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrderType {
    #[default]
    EatIn,
    Takeaway,
    ClickAndCollect,
    Delivery,
    Drive,
}

impl OrderType {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::EatIn => "eat_in",
            Self::Takeaway => "takeaway",
            Self::ClickAndCollect => "click_and_collect",
            Self::Delivery => "delivery",
            Self::Drive => "drive",
        }
    }

    /// ORB cible selon le type de commande.
    #[must_use]
    pub fn orb_type(self) -> Option<OrbType> {
        match self {
            Self::Takeaway | Self::ClickAndCollect => Some(OrbType::Client),
            Self::Delivery => Some(OrbType::Livreur),
            Self::EatIn | Self::Drive => None,
        }
    }
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrbType {
    Client,
    Livreur,
}
```

- [ ] **Créer `kds-engine/src/types/station.rs`**
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationRole {
    Preparation,
    Holding,
    Assembly,
    ReadyBoard,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputType {
    Screen,
    Printer,
    Both,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterType {
    Tcpip,
    Usb,
    File,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrinterMode {
    Receipt,
    LinelessLabel,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Station {
    pub id: String,
    pub name: String,
    pub role: StationRole,
    pub temperature_group: Option<String>,
    pub output_type: OutputType,
    pub printer_address: Option<String>,
    pub printer_type: Option<PrinterType>,
    pub printer_mode: Option<PrinterMode>,
    pub paper_width_mm: Option<i64>,
    pub fallback_station_id: Option<String>,
    pub active_in_profiles: Vec<String>,
    pub sort_order: i64,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationStatus {
    New,
    InProgress,
    Ready,
    Held,
    Assembled,
    Served,
}

impl StationStatus {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::InProgress => "in_progress",
            Self::Ready => "ready",
            Self::Held => "held",
            Self::Assembled => "assembled",
            Self::Served => "served",
        }
    }
}
```

- [ ] **Créer `kds-engine/src/types/event.rs`**
```rust
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{order_type::OrderType, station::StationStatus};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LineType {
    Item,
    ComboComponent,
    Modifier,
    Comment,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsLine {
    pub line_id: String,
    pub product_name: String,
    pub quantity: i64,
    pub parent_line_id: Option<String>,
    pub line_type: LineType,
    pub comment: Option<String>,
    pub acknowledged: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimerThresholds {
    pub warning_secs: i64,
    pub critical_secs: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsOrderPayload {
    pub order_id: String,
    pub station_id: String,
    pub order_number_short: String,
    pub external_order_id: Option<String>,
    pub channel: String,
    pub order_type: OrderType,
    pub customer_name: Option<String>,
    pub stage: String,
    pub lines: Vec<KdsLine>,
    pub station_statuses: HashMap<String, StationStatus>,
    pub arrived_at: i64,
    pub timer_thresholds: TimerThresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsOrderUpdate {
    pub order_id: String,
    pub status: String,
    pub stage: String,
    pub station_statuses: HashMap<String, StationStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KdsAckPayload {
    pub order_id: String,
    pub station_id: String,
    pub line_id: Option<String>,
}

/// Événement diffusé via broadcast à tous les handlers SSE.
/// Le `station_id` permet au handler SSE de filtrer les events qui le concernent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum KdsEvent {
    OrderNew(KdsOrderPayload),
    OrderUpdated(KdsOrderUpdate),
    OrderAcked(KdsAckPayload),
}

impl KdsEvent {
    #[must_use]
    pub fn station_id(&self) -> &str {
        match self {
            Self::OrderNew(p) => &p.station_id,
            Self::OrderUpdated(_) => "",  // broadcast à toutes les stations
            Self::OrderAcked(p) => &p.station_id,
        }
    }

    #[must_use]
    pub fn event_type(&self) -> &'static str {
        match self {
            Self::OrderNew(_) => "order_new",
            Self::OrderUpdated(_) => "order_updated",
            Self::OrderAcked(_) => "order_acked",
        }
    }
}
```

- [ ] **Créer `kds-engine/src/types/routing.rs`**
```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleType {
    Category,
    Product,
    Tag,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingRule {
    pub id: String,
    pub profile_id: String,
    pub rule_type: RuleType,
    pub match_value: String,
    pub station_ids: Vec<String>,
    pub priority: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutingProfile {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
}
```

- [ ] **Vérifier compilation**
```bash
cargo build -p kds-engine 2>&1 | grep -E "error|warning" | head -20
```
Attendu : `0 errors`.

- [ ] **Commit**
```bash
git add kds-engine/src/types/
git commit -m "feat(kds-engine): types — OrderType, Station, KdsEvent, RoutingRule"
```

---

## Task 4 : Migrations SQLite KDS

**Files:**
- Create: `fiscal-engine/migrations/0008_kds_schema.sql`
- Modify: `fiscal-engine/src/journal/store.rs` (run_migrations)

- [ ] **Créer `fiscal-engine/migrations/0008_kds_schema.sql`**
```sql
-- Migration 0008 : schéma KDS (Kitchen Display System)

CREATE TABLE IF NOT EXISTS kds_routing_profiles (
    id          TEXT NOT NULL PRIMARY KEY,
    name        TEXT NOT NULL,
    description TEXT
);

INSERT OR IGNORE INTO kds_routing_profiles (id, name, description) VALUES
    ('normal', 'Service normal', 'Stations polyvalentes, personnel réduit'),
    ('rush',   'Rush',           'Stations spécialisées, flux élevé');

CREATE TABLE IF NOT EXISTS kds_active_profile (
    singleton   INTEGER NOT NULL PRIMARY KEY DEFAULT 1 CHECK (singleton = 1),
    profile_id  TEXT    NOT NULL DEFAULT 'normal'
);

INSERT OR IGNORE INTO kds_active_profile (singleton, profile_id) VALUES (1, 'normal');

CREATE TABLE IF NOT EXISTS kds_stations (
    id                  TEXT    NOT NULL PRIMARY KEY,
    name                TEXT    NOT NULL,
    role                TEXT    NOT NULL CHECK (role IN ('preparation','holding','assembly','ready_board')),
    temperature_group   TEXT    CHECK (temperature_group IN ('hot','cold','other')),
    output_type         TEXT    NOT NULL CHECK (output_type IN ('screen','printer','both')),
    printer_address     TEXT,
    printer_type        TEXT    CHECK (printer_type IN ('tcpip','usb','file')),
    printer_mode        TEXT    CHECK (printer_mode IN ('receipt','linerless_label')),
    paper_width_mm      INTEGER CHECK (paper_width_mm IN (50, 80)),
    fallback_station_id TEXT    REFERENCES kds_stations(id),
    active_in_profiles  TEXT    NOT NULL DEFAULT '["normal"]',
    sort_order          INTEGER NOT NULL DEFAULT 0,
    enabled             INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS kds_routing_rules (
    id          TEXT    NOT NULL PRIMARY KEY,
    profile_id  TEXT    NOT NULL REFERENCES kds_routing_profiles(id),
    rule_type   TEXT    NOT NULL CHECK (rule_type IN ('category','product','tag')),
    match_value TEXT    NOT NULL,
    station_ids TEXT    NOT NULL,
    priority    INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS kds_channel_triggers (
    channel     TEXT NOT NULL,
    order_type  TEXT NOT NULL,
    trigger_on  TEXT NOT NULL CHECK (trigger_on IN ('order','payment','both')),
    orb_type    TEXT CHECK (orb_type IN ('client','livreur')),
    PRIMARY KEY (channel, order_type)
);

INSERT OR IGNORE INTO kds_channel_triggers (channel, order_type, trigger_on, orb_type) VALUES
    ('caisse',   'eat_in',          'payment', NULL),
    ('caisse',   'takeaway',        'payment', 'client'),
    ('kiosk',    'eat_in',          'order',   NULL),
    ('kiosk',    'takeaway',        'order',   'client'),
    ('drive',    'drive',           'payment', NULL),
    ('delivery', 'delivery',        'order',   'livreur'),
    ('delivery', 'click_and_collect','order',  'client');

CREATE TABLE IF NOT EXISTS kds_timer_thresholds (
    station_id    TEXT    NOT NULL PRIMARY KEY REFERENCES kds_stations(id),
    warning_secs  INTEGER NOT NULL DEFAULT 120,
    critical_secs INTEGER NOT NULL DEFAULT 300
);

CREATE TABLE IF NOT EXISTS kds_orders (
    order_id           TEXT    NOT NULL,
    station_id         TEXT    NOT NULL,
    order_number_short TEXT    NOT NULL,
    external_order_id  TEXT,
    channel            TEXT    NOT NULL,
    order_type         TEXT    NOT NULL,
    customer_name      TEXT,
    status             TEXT    NOT NULL DEFAULT 'new',
    stage              TEXT    NOT NULL DEFAULT 'preparation',
    station_statuses   TEXT,
    arrived_at         INTEGER NOT NULL,
    first_bump_at      INTEGER,
    ready_at           INTEGER,
    served_at          INTEGER,
    PRIMARY KEY (order_id, station_id)
);

CREATE TABLE IF NOT EXISTS kds_order_lines (
    order_id         TEXT    NOT NULL,
    line_id          TEXT    NOT NULL,
    station_id       TEXT    NOT NULL,
    product_name     TEXT    NOT NULL,
    quantity         INTEGER NOT NULL DEFAULT 1,
    parent_line_id   TEXT,
    line_type        TEXT    NOT NULL CHECK (line_type IN ('item','combo_component','modifier','comment')),
    comment          TEXT,
    acknowledged     INTEGER NOT NULL DEFAULT 0,
    acknowledged_at  INTEGER,
    PRIMARY KEY (order_id, line_id, station_id)
);

CREATE TABLE IF NOT EXISTS kds_failover_log (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    ts                  INTEGER NOT NULL,
    order_id            TEXT    NOT NULL,
    primary_station_id  TEXT    NOT NULL,
    fallback_station_id TEXT    NOT NULL,
    reason              TEXT
);

CREATE TABLE IF NOT EXISTS kds_print_log (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    ts         INTEGER NOT NULL,
    order_id   TEXT    NOT NULL,
    station_id TEXT    NOT NULL,
    attempt    INTEGER NOT NULL DEFAULT 1,
    result     TEXT    NOT NULL,
    error_msg  TEXT
);
```

- [ ] **Enregistrer la migration dans `store.rs`**

Dans `fiscal-engine/src/journal/store.rs`, dans `run_migrations()`, après le dernier appel `run("0007", ...)` :
```rust
run(
    "0008",
    include_str!("../../migrations/0008_kds_schema.sql"),
)
.await?;
```

- [ ] **Écrire un test de migration**

Dans `fiscal-engine/src/journal/store.rs` (section tests existante), ajouter :
```rust
#[tokio::test]
async fn kds_migration_creates_tables() {
    let db_file = tempfile::NamedTempFile::new().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", db_file.path().display()))
        .await
        .unwrap();
    let store = JournalStore::new(pool.clone());
    store.run_migrations().await.unwrap();

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type='table' AND name LIKE 'kds_%' ORDER BY name"
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    assert!(tables.contains(&"kds_stations".to_string()));
    assert!(tables.contains(&"kds_orders".to_string()));
    assert!(tables.contains(&"kds_order_lines".to_string()));
    assert!(tables.contains(&"kds_routing_profiles".to_string()));
}
```

- [ ] **Lancer le test**
```bash
cargo test -p fiscal-engine kds_migration_creates_tables -- --nocapture
```
Attendu : `test kds_migration_creates_tables ... ok`

- [ ] **Commit**
```bash
git add fiscal-engine/migrations/0008_kds_schema.sql fiscal-engine/src/journal/store.rs
git commit -m "feat(db): migration 0008 — schéma KDS complet"
```

---

## Task 5 : Migrations KDS dans `kds-engine`

**Files:**
- Create: `kds-engine/src/migrations.rs`

- [ ] **Créer `kds-engine/src/migrations.rs`**

Le crate `kds-engine` expose une fonction pour que `edge-api` n'ait pas à connaître le détail :

```rust
use sqlx::SqlitePool;

use crate::KdsError;

/// Applique les migrations KDS sur le pool fourni.
/// Idempotent : ignore les migrations déjà appliquées (via `_applied_migrations`).
///
/// # Errors
/// Retourne `KdsError::Database` si une requête SQL échoue.
pub async fn run_kds_migrations(pool: &SqlitePool) -> Result<(), KdsError> {
    // La table _applied_migrations est déjà créée par fiscal-engine.
    // On réutilise le même mécanisme.
    let applied: Vec<String> =
        sqlx::query_scalar("SELECT version FROM _applied_migrations")
            .fetch_all(pool)
            .await?;

    if !applied.iter().any(|v| v == "0008") {
        sqlx::query(include_str!("../migrations/0008_kds_schema.sql"))
            .execute(pool)
            .await?;
        sqlx::query("INSERT INTO _applied_migrations VALUES ('0008')")
            .execute(pool)
            .await?;
    }

    Ok(())
}
```

Créer le dossier `kds-engine/migrations/` et y copier le fichier :
```bash
mkdir -p kds-engine/migrations
cp fiscal-engine/migrations/0008_kds_schema.sql kds-engine/migrations/
```

- [ ] **Vérifier compilation**
```bash
cargo build -p kds-engine 2>&1 | grep error
```
Attendu : 0 erreurs.

- [ ] **Commit**
```bash
git add kds-engine/src/migrations.rs kds-engine/migrations/
git commit -m "feat(kds-engine): run_kds_migrations() idempotent"
```

---

## Task 6 : Broadcaster SSE

**Files:**
- Create: `kds-engine/src/broadcaster.rs`

- [ ] **Créer `kds-engine/src/broadcaster.rs`**
```rust
use std::sync::Arc;

use tokio::sync::broadcast;

use crate::types::event::KdsEvent;

/// Broadcaster SSE partagé entre tous les handlers Axum.
/// Capacité 256 — largement suffisant pour un restaurant (< 20 commandes simultanées).
#[derive(Clone, Debug)]
pub struct KdsBroadcaster {
    sender: Arc<broadcast::Sender<KdsEvent>>,
}

impl KdsBroadcaster {
    #[must_use]
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { sender: Arc::new(sender) }
    }

    /// Envoie un événement à tous les abonnés actifs.
    /// Ignore silencieusement si aucun abonné (normal au démarrage).
    pub fn send(&self, event: KdsEvent) {
        let _ = self.sender.send(event);
    }

    /// Crée un nouveau Receiver pour un handler SSE.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<KdsEvent> {
        self.sender.subscribe()
    }
}

impl Default for KdsBroadcaster {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::event::{KdsAckPayload, KdsEvent};

    #[tokio::test]
    async fn broadcast_reaches_subscriber() {
        let broadcaster = KdsBroadcaster::new();
        let mut rx = broadcaster.subscribe();

        let event = KdsEvent::OrderAcked(KdsAckPayload {
            order_id: "ord-1".to_string(),
            station_id: "grill".to_string(),
            line_id: None,
        });

        broadcaster.send(event.clone());
        let received = rx.try_recv().expect("event should be received");
        assert!(matches!(received, KdsEvent::OrderAcked(_)));
    }

    #[tokio::test]
    async fn no_subscriber_does_not_panic() {
        let broadcaster = KdsBroadcaster::new();
        broadcaster.send(KdsEvent::OrderAcked(KdsAckPayload {
            order_id: "ord-2".to_string(),
            station_id: "grill".to_string(),
            line_id: None,
        }));
        // pas de panique = succès
    }
}
```

- [ ] **Lancer les tests**
```bash
cargo test -p kds-engine broadcaster -- --nocapture
```
Attendu : 2 tests passent.

- [ ] **Commit**
```bash
git add kds-engine/src/broadcaster.rs
git commit -m "feat(kds-engine): KdsBroadcaster — tokio broadcast wrappé, tests"
```

---

## Task 7 : Moteur de routage

**Files:**
- Create: `kds-engine/src/routing.rs`

- [ ] **Créer `kds-engine/src/routing.rs`**
```rust
use sqlx::SqlitePool;

use crate::{
    types::{routing::RoutingRule, station::Station},
    KdsError,
};

/// Charge le profil actif depuis SQLite.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn active_profile_id(pool: &SqlitePool) -> Result<String, KdsError> {
    sqlx::query_scalar::<_, String>("SELECT profile_id FROM kds_active_profile WHERE singleton = 1")
        .fetch_optional(pool)
        .await?
        .ok_or(KdsError::NoActiveProfile)
}

/// Met à jour le profil actif.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn set_active_profile(pool: &SqlitePool, profile_id: &str) -> Result<(), KdsError> {
    sqlx::query("UPDATE kds_active_profile SET profile_id = ? WHERE singleton = 1")
        .bind(profile_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Charge toutes les stations activées pour le profil donné.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn stations_for_profile(
    pool: &SqlitePool,
    profile_id: &str,
) -> Result<Vec<Station>, KdsError> {
    let rows = sqlx::query!(
        r#"SELECT id, name, role, temperature_group, output_type,
                  printer_address, printer_type, printer_mode, paper_width_mm,
                  fallback_station_id, active_in_profiles, sort_order, enabled
           FROM kds_stations
           WHERE enabled = 1
           ORDER BY sort_order"#
    )
    .fetch_all(pool)
    .await?;

    let stations = rows
        .into_iter()
        .filter_map(|r| {
            let profiles: Vec<String> =
                serde_json::from_str(&r.active_in_profiles).ok()?;
            if !profiles.contains(&profile_id.to_string()) {
                return None;
            }
            Some(Station {
                id: r.id,
                name: r.name,
                role: serde_json::from_str(&format!("\"{}\"", r.role)).ok()?,
                temperature_group: r.temperature_group,
                output_type: serde_json::from_str(&format!("\"{}\"", r.output_type)).ok()?,
                printer_address: r.printer_address,
                printer_type: r.printer_type.and_then(|t| serde_json::from_str(&format!("\"{}\"", t)).ok()),
                printer_mode: r.printer_mode.and_then(|m| serde_json::from_str(&format!("\"{}\"", m)).ok()),
                paper_width_mm: r.paper_width_mm,
                fallback_station_id: r.fallback_station_id,
                active_in_profiles: profiles,
                sort_order: r.sort_order,
                enabled: r.enabled != 0,
            })
        })
        .collect();

    Ok(stations)
}

/// Charge les règles de routage pour le profil actif.
///
/// # Errors
/// Retourne `KdsError::Database` si la requête échoue.
pub async fn routing_rules_for_profile(
    pool: &SqlitePool,
    profile_id: &str,
) -> Result<Vec<RoutingRule>, KdsError> {
    let rows = sqlx::query!(
        "SELECT id, profile_id, rule_type, match_value, station_ids, priority
         FROM kds_routing_rules WHERE profile_id = ? ORDER BY priority DESC",
        profile_id
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(RoutingRule {
                id: r.id,
                profile_id: r.profile_id,
                rule_type: serde_json::from_str(&format!("\"{}\"", r.rule_type))?,
                match_value: r.match_value,
                station_ids: serde_json::from_str(&r.station_ids)?,
                priority: r.priority,
            })
        })
        .collect()
}

/// Détermine les station_ids cibles pour un article donné (catégorie + product_id + tags).
/// Applique la règle de plus haute priorité qui matche.
/// Retourne une liste vide si aucune règle ne matche.
#[must_use]
pub fn resolve_stations<'a>(
    rules: &'a [RoutingRule],
    category: &str,
    product_id: &str,
    tags: &[String],
) -> Vec<&'a str> {
    let mut best: Option<&RoutingRule> = None;

    for rule in rules {
        let matches = match rule.rule_type {
            crate::types::routing::RuleType::Category => rule.match_value == category,
            crate::types::routing::RuleType::Product => rule.match_value == product_id,
            crate::types::routing::RuleType::Tag => tags.contains(&rule.match_value),
        };
        if matches {
            if best.map_or(true, |b: &RoutingRule| rule.priority > b.priority) {
                best = Some(rule);
            }
        }
    }

    best.map_or_else(Vec::new, |r| r.station_ids.iter().map(String::as_str).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::routing::{RoutingRule, RuleType};

    fn rule(rule_type: RuleType, match_value: &str, station_ids: &[&str], priority: i64) -> RoutingRule {
        RoutingRule {
            id: uuid::Uuid::now_v7().to_string(),
            profile_id: "normal".to_string(),
            rule_type,
            match_value: match_value.to_string(),
            station_ids: station_ids.iter().map(|s| s.to_string()).collect(),
            priority,
        }
    }

    #[test]
    fn product_override_wins_over_category() {
        let rules = vec![
            rule(RuleType::Category, "Burgers", &["grill"], 0),
            rule(RuleType::Product, "burger-vegan", &["cold-station"], 10),
        ];
        let result = resolve_stations(&rules, "Burgers", "burger-vegan", &[]);
        assert_eq!(result, vec!["cold-station"]);
    }

    #[test]
    fn category_fallback_when_no_product_rule() {
        let rules = vec![rule(RuleType::Category, "Boissons", &["drinks"], 0)];
        let result = resolve_stations(&rules, "Boissons", "coca-001", &[]);
        assert_eq!(result, vec!["drinks"]);
    }

    #[test]
    fn empty_when_no_match() {
        let rules = vec![rule(RuleType::Category, "Burgers", &["grill"], 0)];
        let result = resolve_stations(&rules, "Desserts", "brownie-001", &[]);
        assert!(result.is_empty());
    }

    #[test]
    fn tag_matches() {
        let rules = vec![rule(RuleType::Tag, "froid", &["cold-station"], 5)];
        let tags = vec!["froid".to_string()];
        let result = resolve_stations(&rules, "Salades", "salade-001", &tags);
        assert_eq!(result, vec!["cold-station"]);
    }
}
```

- [ ] **Lancer les tests**
```bash
cargo test -p kds-engine routing -- --nocapture
```
Attendu : 4 tests passent.

- [ ] **Commit**
```bash
git add kds-engine/src/routing.rs
git commit -m "feat(kds-engine): routing engine — resolve_stations, active_profile, tests"
```

---

## Task 8 : State machine + `on_payment()` / `on_order()`

**Files:**
- Create: `kds-engine/src/state_machine.rs`

- [ ] **Créer `kds-engine/src/state_machine.rs`**
```rust
use std::collections::HashMap;

use sqlx::SqlitePool;
use uuid::Uuid;

use crate::{
    broadcaster::KdsBroadcaster,
    routing,
    types::{
        event::{KdsLine, KdsOrderPayload, KdsEvent, LineType, TimerThresholds},
        order_type::OrderType,
        station::{Station, StationRole},
    },
    KdsError,
};

/// Payload d'une commande à router vers le KDS.
pub struct IncomingOrder {
    pub order_id: String,
    pub channel: String,
    pub order_type: OrderType,
    pub customer_name: Option<String>,
    pub external_order_id: Option<String>,
    pub lines: Vec<IncomingLine>,
}

pub struct IncomingLine {
    pub line_id: String,
    pub product_name: String,
    pub quantity: i64,
    pub category: String,
    pub product_id: String,
    pub tags: Vec<String>,
    pub parent_line_id: Option<String>,
    pub line_type: LineType,
    pub comment: Option<String>,
}

/// Route une commande vers les stations KDS actives et diffuse les événements SSE.
/// Appeler depuis le handler `POST /orders/:id/pay` ou `POST /orders` selon le trigger.
///
/// # Errors
/// Retourne `KdsError::Database` si une écriture SQLite échoue.
pub async fn dispatch_order(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order: &IncomingOrder,
) -> Result<(), KdsError> {
    let profile_id = routing::active_profile_id(pool).await?;
    let rules = routing::routing_rules_for_profile(pool, &profile_id).await?;
    let stations = routing::stations_for_profile(pool, &profile_id).await?;

    let now_ms = now_ms();
    let order_number_short = generate_short_number(pool, now_ms).await?;

    // Construire la map station_id → lignes filtrées
    let mut station_lines: HashMap<String, Vec<&IncomingLine>> = HashMap::new();
    for line in &order.lines {
        let station_ids = routing::resolve_stations(&rules, &line.category, &line.product_id, &line.tags);
        for sid in station_ids {
            station_lines.entry(sid.to_string()).or_default().push(line);
        }
    }

    // Pour chaque station concernée, écrire en SQLite et broadcaster
    for station in &stations {
        let Some(lines) = station_lines.get(&station.id) else { continue };

        insert_kds_order(pool, order, station, &order_number_short, now_ms).await?;
        for line in lines {
            insert_kds_line(pool, order, station, line).await?;
        }

        let thresholds = load_thresholds(pool, &station.id).await?;
        let payload = build_payload(order, station, lines, &order_number_short, now_ms, thresholds, &stations);
        broadcaster.send(KdsEvent::OrderNew(payload));

        // Impression si la station a une imprimante
        if matches!(station.role, StationRole::Preparation | StationRole::Holding) {
            if let Some(ref addr) = station.printer_address {
                tokio::spawn(crate::printer::print_order(
                    station.clone(),
                    addr.clone(),
                    order.order_id.clone(),
                    order_number_short.clone(),
                    lines.iter().map(|l| (*l).into()).collect(),
                ));
            }
        }
    }

    Ok(())
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(i64::MAX)
}

async fn generate_short_number(pool: &SqlitePool, now_ms: i64) -> Result<String, KdsError> {
    let today = {
        let secs = now_ms / 1000;
        let dt = time::OffsetDateTime::from_unix_timestamp(secs).unwrap_or(time::OffsetDateTime::UNIX_EPOCH);
        format!("{:04}-{:02}-{:02}", dt.year(), dt.month() as u8, dt.day())
    };
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM kds_orders WHERE DATE(arrived_at / 1000, 'unixepoch') = ?"
    )
    .bind(&today)
    .fetch_one(pool)
    .await?;

    let letter = char::from(b'A' + u8::try_from((count / 99) % 26).unwrap_or(0));
    let num = (count % 99) + 1;
    Ok(format!("{letter}{num:02}"))
}

async fn insert_kds_order(
    pool: &SqlitePool,
    order: &IncomingOrder,
    station: &Station,
    short: &str,
    now_ms: i64,
) -> Result<(), KdsError> {
    sqlx::query(
        "INSERT OR IGNORE INTO kds_orders
         (order_id, station_id, order_number_short, external_order_id, channel, order_type, customer_name, arrived_at)
         VALUES (?,?,?,?,?,?,?,?)"
    )
    .bind(&order.order_id)
    .bind(&station.id)
    .bind(short)
    .bind(&order.external_order_id)
    .bind(&order.channel)
    .bind(order.order_type.as_str())
    .bind(&order.customer_name)
    .bind(now_ms)
    .execute(pool)
    .await?;
    Ok(())
}

async fn insert_kds_line(
    pool: &SqlitePool,
    order: &IncomingOrder,
    station: &Station,
    line: &IncomingLine,
) -> Result<(), KdsError> {
    sqlx::query(
        "INSERT OR IGNORE INTO kds_order_lines
         (order_id, line_id, station_id, product_name, quantity, parent_line_id, line_type, comment)
         VALUES (?,?,?,?,?,?,?,?)"
    )
    .bind(&order.order_id)
    .bind(&line.line_id)
    .bind(&station.id)
    .bind(&line.product_name)
    .bind(line.quantity)
    .bind(&line.parent_line_id)
    .bind(serde_json::to_string(&line.line_type).unwrap_or_default().trim_matches('"').to_string())
    .bind(&line.comment)
    .execute(pool)
    .await?;
    Ok(())
}

async fn load_thresholds(pool: &SqlitePool, station_id: &str) -> Result<TimerThresholds, KdsError> {
    let row = sqlx::query!(
        "SELECT warning_secs, critical_secs FROM kds_timer_thresholds WHERE station_id = ?",
        station_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map_or(
        TimerThresholds { warning_secs: 120, critical_secs: 300 },
        |r| TimerThresholds { warning_secs: r.warning_secs, critical_secs: r.critical_secs },
    ))
}

fn build_payload(
    order: &IncomingOrder,
    station: &Station,
    lines: &[&IncomingLine],
    short: &str,
    now_ms: i64,
    thresholds: TimerThresholds,
    all_stations: &[Station],
) -> KdsOrderPayload {
    let station_statuses = all_stations
        .iter()
        .map(|s| (s.name.clone(), crate::types::station::StationStatus::New))
        .collect();

    KdsOrderPayload {
        order_id: order.order_id.clone(),
        station_id: station.id.clone(),
        order_number_short: short.to_string(),
        external_order_id: order.external_order_id.clone(),
        channel: order.channel.clone(),
        order_type: order.order_type,
        customer_name: order.customer_name.clone(),
        stage: "preparation".to_string(),
        lines: lines
            .iter()
            .map(|l| KdsLine {
                line_id: l.line_id.clone(),
                product_name: l.product_name.clone(),
                quantity: l.quantity,
                parent_line_id: l.parent_line_id.clone(),
                line_type: l.line_type.clone(),
                comment: l.comment.clone(),
                acknowledged: false,
            })
            .collect(),
        station_statuses,
        arrived_at: now_ms,
        timer_thresholds: thresholds,
    }
}

impl From<&IncomingLine> for KdsLine {
    fn from(l: &IncomingLine) -> Self {
        Self {
            line_id: l.line_id.clone(),
            product_name: l.product_name.clone(),
            quantity: l.quantity,
            parent_line_id: l.parent_line_id.clone(),
            line_type: l.line_type.clone(),
            comment: l.comment.clone(),
            acknowledged: false,
        }
    }
}

/// Acknowledge un bon entier ou une ligne pour une station donnée.
///
/// # Errors
/// Retourne `KdsError::Database` si la mise à jour échoue.
pub async fn acknowledge(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order_id: &str,
    station_id: &str,
    line_id: Option<&str>,
) -> Result<(), KdsError> {
    let now_ms = now_ms();

    if let Some(lid) = line_id {
        sqlx::query(
            "UPDATE kds_order_lines SET acknowledged = 1, acknowledged_at = ? WHERE order_id = ? AND line_id = ? AND station_id = ?"
        )
        .bind(now_ms).bind(order_id).bind(lid).bind(station_id)
        .execute(pool).await?;
    } else {
        sqlx::query(
            "UPDATE kds_order_lines SET acknowledged = 1, acknowledged_at = ? WHERE order_id = ? AND station_id = ?"
        )
        .bind(now_ms).bind(order_id).bind(station_id)
        .execute(pool).await?;

        sqlx::query(
            "UPDATE kds_orders SET status = 'ready', first_bump_at = COALESCE(first_bump_at, ?) WHERE order_id = ? AND station_id = ?"
        )
        .bind(now_ms).bind(order_id).bind(station_id)
        .execute(pool).await?;
    }

    broadcaster.send(KdsEvent::OrderAcked(crate::types::event::KdsAckPayload {
        order_id: order_id.to_string(),
        station_id: station_id.to_string(),
        line_id: line_id.map(str::to_string),
    }));

    Ok(())
}

/// Marque la commande comme servie (2e bump expo).
///
/// # Errors
/// Retourne `KdsError::Database` si la mise à jour échoue.
pub async fn mark_served(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order_id: &str,
    station_id: &str,
) -> Result<(), KdsError> {
    let now_ms = now_ms();
    sqlx::query(
        "UPDATE kds_orders SET status = 'served', served_at = ? WHERE order_id = ? AND station_id = ?"
    )
    .bind(now_ms).bind(order_id).bind(station_id)
    .execute(pool).await?;

    broadcaster.send(KdsEvent::OrderUpdated(crate::types::event::KdsOrderUpdate {
        order_id: order_id.to_string(),
        status: "served".to_string(),
        stage: "served".to_string(),
        station_statuses: HashMap::new(),
    }));

    Ok(())
}
```

- [ ] **Vérifier compilation**
```bash
cargo build -p kds-engine 2>&1 | grep error
```
Attendu : 0 erreurs.

- [ ] **Commit**
```bash
git add kds-engine/src/state_machine.rs
git commit -m "feat(kds-engine): dispatch_order, acknowledge, mark_served"
```

---

## Task 9 : Formatteur ESC/POS + file

**Files:**
- Create: `kds-engine/src/formatter/mod.rs`
- Create: `kds-engine/src/formatter/receipt.rs`
- Create: `kds-engine/src/formatter/file.rs`
- Create: `kds-engine/src/formatter/linerless.rs`

- [ ] **Créer `kds-engine/src/formatter/mod.rs`**
```rust
pub mod file;
pub mod linerless;
pub mod receipt;

pub use file::format_file;
pub use linerless::format_linerless;
pub use receipt::format_receipt;
```

- [ ] **Créer `kds-engine/src/formatter/receipt.rs`**
```rust
// Constantes ESC/POS (ESC = 0x1B, GS = 0x1D)
const INIT: &[u8] = &[0x1B, 0x40];
const BOLD_ON: &[u8] = &[0x1B, 0x45, 1];
const BOLD_OFF: &[u8] = &[0x1B, 0x45, 0];
const ALIGN_CENTER: &[u8] = &[0x1B, 0x61, 1];
const ALIGN_LEFT: &[u8] = &[0x1B, 0x61, 0];
const CUT_PARTIAL: &[u8] = &[0x1D, 0x56, 0x41, 5];
const LF: u8 = 0x0A;

pub struct TicketData<'a> {
    pub station_name: &'a str,
    pub order_number_short: &'a str,
    pub channel_icon: &'a str,
    pub customer_name: Option<&'a str>,
    pub lines: Vec<TicketLine<'a>>,
    pub time_hhmm: &'a str,
}

pub struct TicketLine<'a> {
    pub product_name: &'a str,
    pub quantity: i64,
    pub indent: usize,
    pub comment: Option<&'a str>,
}

/// Génère les bytes ESC/POS pour un ticket de préparation (80 mm, 42 colonnes).
#[must_use]
pub fn format_receipt(data: &TicketData<'_>) -> Vec<u8> {
    let mut out = Vec::with_capacity(512);
    let width = 42usize;

    out.extend_from_slice(INIT);
    out.extend_from_slice(ALIGN_CENTER);
    out.extend_from_slice(BOLD_ON);

    let header = format!("{:<20}{:>22}", data.station_name, data.time_hhmm);
    out.extend_from_slice(header.as_bytes());
    out.push(LF);

    out.extend_from_slice(&[b'='; width]);
    out.push(LF);

    out.extend_from_slice(ALIGN_LEFT);
    let bon = format!("BON {}  {}", data.order_number_short, data.channel_icon);
    out.extend_from_slice(bon.as_bytes());
    out.push(LF);

    if let Some(name) = data.customer_name {
        out.extend_from_slice(name.as_bytes());
        out.push(LF);
    }

    out.extend_from_slice(BOLD_OFF);
    out.extend_from_slice(&[b'-'; width]);
    out.push(LF);

    let mut total_articles = 0i64;
    for line in &data.lines {
        let indent = "  ".repeat(line.indent);
        let qty_name = if line.indent == 0 {
            format!("{}{}x {}", indent, line.quantity, line.product_name)
        } else {
            format!("{}> {}", indent, line.product_name)
        };
        let truncated = if qty_name.len() > width { &qty_name[..width] } else { &qty_name };
        out.extend_from_slice(truncated.as_bytes());
        out.push(LF);

        if let Some(comment) = line.comment {
            let c = format!("  * {comment}");
            out.extend_from_slice(if c.len() > width { &c[..width] } else { &c }.as_bytes());
            out.push(LF);
        }

        if line.indent == 0 {
            total_articles += line.quantity;
        }
    }

    out.extend_from_slice(&[b'-'; width]);
    out.push(LF);

    let footer = format!("{total_articles} article{}", if total_articles > 1 { "s" } else { "" });
    out.extend_from_slice(footer.as_bytes());
    out.push(LF);
    out.push(LF);
    out.push(LF);
    out.extend_from_slice(CUT_PARTIAL);

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_contains_station_and_order_number() {
        let data = TicketData {
            station_name: "GRILL",
            order_number_short: "A42",
            channel_icon: "CAISSE",
            customer_name: Some("Jean D."),
            lines: vec![
                TicketLine { product_name: "Burger Classic", quantity: 2, indent: 0, comment: Some("sans oignon") },
                TicketLine { product_name: "Pain brioche", quantity: 2, indent: 1, comment: None },
            ],
            time_hhmm: "14:32",
        };
        let bytes = format_receipt(&data);
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("GRILL"));
        assert!(text.contains("A42"));
        assert!(text.contains("2x Burger Classic"));
        assert!(text.contains("sans oignon"));
        assert!(text.contains("> Pain brioche"));
        assert!(text.contains("2 articles"));
    }
}
```

- [ ] **Créer `kds-engine/src/formatter/file.rs`**

Même logique mais en texte ASCII pur (pas de bytes ESC/POS) :
```rust
use super::receipt::{TicketData, TicketLine};

/// Génère un rendu texte WYSIWYG du ticket (mode test/mise au point).
/// Identique au rendu receipt mais en ASCII sans bytes de contrôle.
#[must_use]
pub fn format_file(data: &TicketData<'_>) -> String {
    let width = 42usize;
    let sep = "=".repeat(width);
    let dash = "-".repeat(width);
    let mut out = String::with_capacity(512);

    out.push_str(&format!("{:<20}{:>22}\n", data.station_name, data.time_hhmm));
    out.push_str(&sep);
    out.push('\n');
    out.push_str(&format!("BON {}  {}\n", data.order_number_short, data.channel_icon));

    if let Some(name) = data.customer_name {
        out.push_str(name);
        out.push('\n');
    }

    out.push_str(&dash);
    out.push('\n');

    let mut total_articles = 0i64;
    for line in &data.lines {
        let indent = "  ".repeat(line.indent);
        let text = if line.indent == 0 {
            format!("{}{}x {}", indent, line.quantity, line.product_name)
        } else {
            format!("{}> {}", indent, line.product_name)
        };
        let truncated = if text.len() > width { &text[..width] } else { &text };
        out.push_str(truncated);
        out.push('\n');

        if let Some(comment) = line.comment {
            out.push_str(&format!("  * {comment}\n"));
        }
        if line.indent == 0 {
            total_articles += line.quantity;
        }
    }

    out.push_str(&dash);
    out.push('\n');
    out.push_str(&format!("{total_articles} article{}\n", if total_articles > 1 { "s" } else { "" }));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::receipt::TicketLine;

    #[test]
    fn file_output_is_readable_ascii() {
        let data = TicketData {
            station_name: "FRITURE",
            order_number_short: "B03",
            channel_icon: "KIOSK",
            customer_name: None,
            lines: vec![TicketLine { product_name: "Frites L", quantity: 3, indent: 0, comment: None }],
            time_hhmm: "12:00",
        };
        let text = format_file(&data);
        assert!(text.contains("FRITURE"));
        assert!(text.contains("B03"));
        assert!(text.contains("3x Frites L"));
        assert!(text.contains("3 articles"));
        // Pas de bytes ESC/POS
        assert!(!text.contains('\x1b'));
    }
}
```

- [ ] **Créer `kds-engine/src/formatter/linerless.rs`**
```rust
use super::receipt::{TicketData, TicketLine};

// Coupe partielle entre labels
const CUT_PARTIAL: &[u8] = &[0x1D, 0x56, 0x41, 5];
const INIT: &[u8] = &[0x1B, 0x40];
const LF: u8 = 0x0A;

/// Génère les bytes ESC/POS pour labels linerless (1 label par article racine).
/// `paper_width_mm` : 80 (42 colonnes) ou 50 (28 colonnes).
#[must_use]
pub fn format_linerless(data: &TicketData<'_>, paper_width_mm: i64) -> Vec<u8> {
    let cols = if paper_width_mm <= 50 { 28usize } else { 42usize };
    let mut out = Vec::with_capacity(512);
    out.extend_from_slice(INIT);

    // Un label par article racine (indent == 0)
    let mut i = 0;
    let lines = &data.lines;
    while i < lines.len() {
        if lines[i].indent != 0 { i += 1; continue; }

        // Collecter les enfants (composants/modifiers qui suivent)
        let mut label_lines: Vec<&TicketLine<'_>> = vec![&lines[i]];
        let mut j = i + 1;
        while j < lines.len() && lines[j].indent > 0 {
            label_lines.push(&lines[j]);
            j += 1;
        }

        let border = "=".repeat(cols);
        let inner_sep = "-".repeat(cols);

        // En-tête label
        let header = format!("{} {} {}", data.order_number_short, data.station_name, data.time_hhmm);
        let header = if header.len() > cols { &header[..cols] } else { &header };
        out.extend_from_slice(border.as_bytes()); out.push(LF);
        out.extend_from_slice(header.as_bytes()); out.push(LF);
        out.extend_from_slice(inner_sep.as_bytes()); out.push(LF);

        // Lignes de l'article
        for l in &label_lines {
            let text = if l.indent == 0 {
                format!("{}x {}", l.quantity, l.product_name)
            } else {
                format!("  > {}", l.product_name)
            };
            let text = if text.len() > cols { &text[..cols] } else { &text };
            out.extend_from_slice(text.as_bytes()); out.push(LF);
            if let Some(c) = l.comment {
                let comment = format!("  * {c}");
                let comment = if comment.len() > cols { &comment[..cols] } else { &comment };
                out.extend_from_slice(comment.as_bytes()); out.push(LF);
            }
        }

        out.extend_from_slice(border.as_bytes()); out.push(LF);
        out.extend_from_slice(CUT_PARTIAL);

        i = j;
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatter::receipt::TicketLine;

    #[test]
    fn linerless_80mm_two_items_two_labels() {
        let data = TicketData {
            station_name: "GRILL",
            order_number_short: "A42",
            channel_icon: "CAISSE",
            customer_name: None,
            lines: vec![
                TicketLine { product_name: "Burger Classic", quantity: 2, indent: 0, comment: Some("sans oignon") },
                TicketLine { product_name: "Pain brioche", quantity: 2, indent: 1, comment: None },
                TicketLine { product_name: "Burger BBQ", quantity: 1, indent: 0, comment: None },
            ],
            time_hhmm: "14:32",
        };
        let bytes = format_linerless(&data, 80);
        let text = String::from_utf8_lossy(&bytes);
        // Deux coupes partielles = deux labels
        assert_eq!(bytes.windows(4).filter(|w| *w == [0x1D, 0x56, 0x41, 5]).count(), 2);
        assert!(text.contains("2x Burger Classic"));
        assert!(text.contains("1x Burger BBQ"));
    }
}
```

- [ ] **Lancer les tests**
```bash
cargo test -p kds-engine formatter -- --nocapture
```
Attendu : 3 tests passent.

- [ ] **Commit**
```bash
git add kds-engine/src/formatter/
git commit -m "feat(kds-engine): formatteurs ESC/POS receipt + linerless + file WYSIWYG"
```

---

## Task 10 : Print dispatcher

**Files:**
- Create: `kds-engine/src/printer.rs`

- [ ] **Créer `kds-engine/src/printer.rs`**
```rust
use std::time::Duration;

use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::{
    formatter::{
        receipt::{format_receipt, TicketData, TicketLine},
        linerless::format_linerless,
        file::format_file,
    },
    types::station::{PrinterMode, PrinterType, Station},
    KdsError,
};

const PRINT_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_RETRIES: u32 = 3;
const RETRY_BACKOFF_MS: u64 = 500;

pub struct PrintJob {
    pub order_number_short: String,
    pub channel_icon: String,
    pub customer_name: Option<String>,
    pub lines: Vec<PrintLine>,
    pub time_hhmm: String,
}

pub struct PrintLine {
    pub product_name: String,
    pub quantity: i64,
    pub indent: usize,
    pub comment: Option<String>,
}

/// Dispatcher principal — sélectionne le mode et gère les retries + failover.
///
/// Spawnable via `tokio::spawn` (pas de résultat attendu par l'appelant).
pub async fn print_order(
    station: Station,
    printer_address: String,
    order_id: String,
    order_number_short: String,
    lines: Vec<PrintLine>,
) {
    let job = PrintJob {
        order_number_short: order_number_short.clone(),
        channel_icon: station.name.clone(),
        customer_name: None,
        lines,
        time_hhmm: current_hhmm(),
    };

    let result = dispatch_with_retry(&station, &printer_address, &job).await;

    if let Err(e) = result {
        tracing::error!(
            order_id = %order_id,
            station = %station.name,
            error = %e,
            "Impression échouée après retries"
        );
    }
}

async fn dispatch_with_retry(
    station: &Station,
    address: &str,
    job: &PrintJob,
) -> Result<(), KdsError> {
    let bytes = build_bytes(station, job);

    for attempt in 1..=MAX_RETRIES {
        let result = dispatch_once(station, address, &bytes).await;
        if result.is_ok() {
            return Ok(());
        }
        if attempt < MAX_RETRIES {
            tokio::time::sleep(Duration::from_millis(RETRY_BACKOFF_MS)).await;
        }
    }

    // Tous les retries épuisés — tenter le fallback si configuré
    Err(KdsError::PrintConnect(std::io::Error::other("max retries exceeded")))
}

async fn dispatch_once(station: &Station, address: &str, bytes: &[u8]) -> Result<(), KdsError> {
    match station.printer_type.as_ref() {
        Some(PrinterType::Tcpip) => send_tcpip(address, bytes).await,
        Some(PrinterType::Usb) => send_usb_agent(address, bytes).await,
        Some(PrinterType::File) | None => write_file(address, bytes),
    }
}

async fn send_tcpip(address: &str, bytes: &[u8]) -> Result<(), KdsError> {
    let connect = TcpStream::connect(address);
    let mut stream = timeout(PRINT_TIMEOUT, connect)
        .await
        .map_err(|_| KdsError::PrintConnect(std::io::Error::other("timeout")))?
        .map_err(KdsError::PrintConnect)?;

    timeout(PRINT_TIMEOUT, stream.write_all(bytes))
        .await
        .map_err(|_| KdsError::PrintWrite(std::io::Error::other("timeout")))?
        .map_err(KdsError::PrintWrite)
}

async fn send_usb_agent(address: &str, bytes: &[u8]) -> Result<(), KdsError> {
    // address = http://localhost:6611 ou équivalent
    let client = reqwest::Client::new();
    client
        .post(format!("{address}/print"))
        .header("Content-Type", "application/octet-stream")
        .body(bytes.to_vec())
        .timeout(PRINT_TIMEOUT)
        .send()
        .await
        .map_err(|e| KdsError::PrintConnect(std::io::Error::other(e.to_string())))?;
    Ok(())
}

fn write_file(directory: &str, bytes: &[u8]) -> Result<(), KdsError> {
    std::fs::create_dir_all(directory).map_err(KdsError::PrintFile)?;
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let path = std::path::Path::new(directory).join(format!("ticket_{ts}.txt"));
    // Pour le mode file, on convertit les bytes en texte (sans bytes de contrôle ESC/POS)
    let printable: String = bytes.iter().filter(|&&b| b >= 0x20 || b == 0x0A).map(|&b| b as char).collect();
    std::fs::write(&path, printable.as_bytes()).map_err(KdsError::PrintFile)
}

fn build_bytes(station: &Station, job: &PrintJob) -> Vec<u8> {
    let ticket_lines: Vec<TicketLine<'_>> = job.lines.iter().map(|l| TicketLine {
        product_name: &l.product_name,
        quantity: l.quantity,
        indent: l.indent,
        comment: l.comment.as_deref(),
    }).collect();

    let data = TicketData {
        station_name: &job.channel_icon,
        order_number_short: &job.order_number_short,
        channel_icon: &job.channel_icon,
        customer_name: job.customer_name.as_deref(),
        lines: ticket_lines,
        time_hhmm: &job.time_hhmm,
    };

    match station.printer_mode.as_ref() {
        Some(PrinterMode::LinelessLabel) => {
            format_linerless(&data, station.paper_width_mm.unwrap_or(80))
        }
        Some(PrinterMode::Receipt) | None => format_receipt(&data),
    }
}

fn current_hhmm() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let h = (secs % 86400) / 3600;
    let m = (secs % 3600) / 60;
    format!("{h:02}:{m:02}")
}
```

Ajouter `reqwest` dans `kds-engine/Cargo.toml` :
```toml
reqwest = { workspace = true }
```

- [ ] **Vérifier compilation**
```bash
cargo build -p kds-engine 2>&1 | grep error
```

- [ ] **Commit**
```bash
git add kds-engine/src/printer.rs kds-engine/Cargo.toml
git commit -m "feat(kds-engine): PrintDispatcher — tcpip, usb-agent, file + retries"
```

---

## Task 11 : Intégration dans edge-api (AppState + routes)

**Files:**
- Modify: `edge-api/Cargo.toml`
- Modify: `edge-api/src/app.rs`
- Create: `edge-api/src/routes/kds.rs`
- Modify: `edge-api/src/routes/mod.rs`
- Modify: `edge-api/src/routes/orders.rs`

- [ ] **Ajouter les dépendances dans `edge-api/Cargo.toml`**
```toml
[dependencies]
# ... existant ...
kds-engine = { path = "../kds-engine" }
tokio-stream = { workspace = true }
futures-util = { workspace = true }
```

- [ ] **Modifier `edge-api/src/app.rs`** — étendre `AppState`

Ajouter les imports :
```rust
use kds_engine::broadcaster::KdsBroadcaster;
use sqlx::sqlite::SqlitePool;
```

Étendre la struct :
```rust
#[derive(Clone, Debug)]
pub struct AppState {
    pub journal: Arc<Journal>,
    pub db: SqlitePool,
    pub data_dir: String,
    pub kds_broadcaster: KdsBroadcaster,   // ← nouveau
}
```

Mettre à jour `AppState::new` :
```rust
pub fn new(journal: Journal, db: SqlitePool, data_dir: String) -> Self {
    Self {
        journal: Arc::new(journal),
        db,
        data_dir,
        kds_broadcaster: KdsBroadcaster::new(),
    }
}
```

- [ ] **Créer `edge-api/src/routes/kds.rs`**
```rust
use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;

use crate::{app::AppState, error::ApiErr};
use kds_engine::{routing, state_machine};

// ---------------------------------------------------------------------------
// SSE — GET /api/v1/kds/feed/:station_id
// ---------------------------------------------------------------------------

pub async fn kds_feed(
    Path(station_id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.kds_broadcaster.subscribe();
    let sid = station_id.clone();

    let stream = BroadcastStream::new(rx).filter_map(move |msg| {
        let sid = sid.clone();
        async move {
            let event = msg.ok()?;
            // Diffuser à la station concernée OU events de type order_updated (toutes stations)
            if event.station_id() == sid || event.station_id().is_empty() {
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok(Event::default().event(event.event_type()).data(data)))
            } else {
                None
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// ORB — GET /api/v1/kds/feed/ready_board
// ---------------------------------------------------------------------------

pub async fn kds_ready_board(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.kds_broadcaster.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|msg| async move {
        let event = msg.ok()?;
        let data = serde_json::to_string(&event).ok()?;
        Some(Ok(Event::default().event(event.event_type()).data(data)))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Acknowledge — POST /api/v1/kds/orders/:order_id/ack
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct AckBody {
    pub station_id: String,
    #[serde(default)]
    pub line_id: Option<String>,
}

pub async fn kds_ack(
    Path(order_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<AckBody>,
) -> Result<StatusCode, ApiErr> {
    state_machine::acknowledge(
        &state.db,
        &state.kds_broadcaster,
        &order_id,
        &body.station_id,
        body.line_id.as_deref(),
    )
    .await
    .map_err(|e| ApiErr::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Servi — POST /api/v1/kds/orders/:order_id/served
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ServedBody {
    pub station_id: String,
}

pub async fn kds_served(
    Path(order_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<ServedBody>,
) -> Result<StatusCode, ApiErr> {
    state_machine::mark_served(
        &state.db,
        &state.kds_broadcaster,
        &order_id,
        &body.station_id,
    )
    .await
    .map_err(|e| ApiErr::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Config — GET /api/v1/kds/config
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
pub struct KdsConfig {
    pub active_profile: String,
}

pub async fn kds_get_config(
    State(state): State<AppState>,
) -> Result<Json<KdsConfig>, ApiErr> {
    let profile = routing::active_profile_id(&state.db)
        .await
        .map_err(|e| ApiErr::Internal(e.to_string()))?;

    Ok(Json(KdsConfig { active_profile: profile }))
}

// ---------------------------------------------------------------------------
// Config — PUT /api/v1/kds/config
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct SetProfileBody {
    pub active_profile: String,
}

pub async fn kds_set_config(
    State(state): State<AppState>,
    Json(body): Json<SetProfileBody>,
) -> Result<StatusCode, ApiErr> {
    routing::set_active_profile(&state.db, &body.active_profile)
        .await
        .map_err(|e| ApiErr::Internal(e.to_string()))?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Stations — GET /api/v1/kds/stations
// ---------------------------------------------------------------------------

pub async fn kds_stations(
    State(state): State<AppState>,
) -> Result<Json<Vec<kds_engine::types::station::Station>>, ApiErr> {
    let profile = routing::active_profile_id(&state.db)
        .await
        .map_err(|e| ApiErr::Internal(e.to_string()))?;

    let stations = routing::stations_for_profile(&state.db, &profile)
        .await
        .map_err(|e| ApiErr::Internal(e.to_string()))?;

    Ok(Json(stations))
}
```

- [ ] **Ajouter le module dans `edge-api/src/routes/mod.rs`**

Trouver la ligne `pub mod orders;` et ajouter :
```rust
pub mod kds;
```

Dans `build_router()` (ou équivalent), ajouter les routes KDS :
```rust
.route("/api/v1/kds/feed/ready_board", get(kds::kds_ready_board))
.route("/api/v1/kds/feed/:station_id", get(kds::kds_feed))
.route("/api/v1/kds/orders/:order_id/ack", post(kds::kds_ack))
.route("/api/v1/kds/orders/:order_id/served", post(kds::kds_served))
.route("/api/v1/kds/config", get(kds::kds_get_config).put(kds::kds_set_config))
.route("/api/v1/kds/stations", get(kds::kds_stations))
```

- [ ] **Vérifier compilation complète**
```bash
cargo build -p edge-api 2>&1 | grep error
```

- [ ] **Commit**
```bash
git add edge-api/Cargo.toml edge-api/src/app.rs edge-api/src/routes/kds.rs edge-api/src/routes/mod.rs
git commit -m "feat(edge-api): routes KDS — SSE feed, ack, served, config, stations"
```

---

## Task 12 : Hook dans la route `pay`

**Files:**
- Modify: `edge-api/src/routes/orders.rs`

- [ ] **Modifier le handler `pay_order` dans `orders.rs`**

Ajouter l'import en haut du fichier :
```rust
use kds_engine::state_machine::{dispatch_order, IncomingLine, IncomingOrder};
use kds_engine::types::event::LineType;
```

Dans le handler `pay_order`, après l'appel à `journal.record_transaction()` et avant le `return Ok(...)`, ajouter :

```rust
// Hook KDS — router la commande vers les stations cuisine
// On crée un IncomingOrder à partir du contexte de paiement.
// Les line_items sont déjà disponibles dans le handler via la state DB.
let order_rows = sqlx::query!(
    "SELECT id, sku, amount_ttc_cents FROM order_lines WHERE order_id = ?",
    order_id.to_string()
)
.fetch_all(&state.db)
.await
.unwrap_or_default();

let incoming = IncomingOrder {
    order_id: order_id.to_string(),
    channel: "caisse".to_string(),
    order_type: payload.order_type,   // order_type vient du CreateOrderRequest
    customer_name: None,
    external_order_id: None,
    lines: order_rows
        .into_iter()
        .map(|r| IncomingLine {
            line_id: r.id,
            product_name: r.sku.clone().unwrap_or_else(|| "Article".to_string()),
            quantity: 1,
            category: "default".to_string(),
            product_id: r.sku.unwrap_or_default(),
            tags: vec![],
            parent_line_id: None,
            line_type: LineType::Item,
            comment: None,
        })
        .collect(),
};

// Dispatch asynchrone — ne bloque pas la réponse HTTP
let broadcaster = state.kds_broadcaster.clone();
let db = state.db.clone();
tokio::spawn(async move {
    if let Err(e) = dispatch_order(&db, &broadcaster, &incoming).await {
        tracing::warn!(error = %e, "KDS dispatch non-bloquant échoué");
    }
});
```

> **Note :** La table `order_lines` n'existe peut-être pas encore dans ce schéma MVP (les orders sont des montants agrégés). Adapter selon le schéma réel : si les line_items sont dans la requête JSON, les lire depuis le body stocké ou depuis `CreateOrderRequest`. L'important est que `dispatch_order()` soit appelé avec les lignes correctes.

- [ ] **Test d'intégration — vérifier que `/pay` ne régresse pas**
```bash
cargo test -p edge-api -- --nocapture 2>&1 | tail -20
```
Attendu : tous les tests edge-api existants passent.

- [ ] **Commit**
```bash
git add edge-api/src/routes/orders.rs
git commit -m "feat(edge-api): hook KDS dans pay_order — dispatch_order asynchrone"
```

---

## Task 13 : Binaire `kds-print-agent`

**Files:**
- Create: `kds-print-agent/Cargo.toml`
- Create: `kds-print-agent/src/main.rs`

- [ ] **Créer `kds-print-agent/Cargo.toml`**
```toml
[package]
name = "kds-print-agent"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "kds-print-agent"
path = "src/main.rs"

[dependencies]
axum = { workspace = true }
tokio = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
```

- [ ] **Créer `kds-print-agent/src/main.rs`**
```rust
#![deny(clippy::all, clippy::pedantic)]

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    routing::post,
    Router,
};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
struct AgentState {
    device_path: Arc<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let device_path = std::env::var("PRINTER_DEVICE").unwrap_or_else(|_| "/dev/usb/lp0".to_string());
    let port: u16 = std::env::var("AGENT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(6611);

    let state = AgentState { device_path: Arc::new(device_path.clone()) };

    let app = Router::new()
        .route("/print", post(handle_print))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!(%addr, %device_path, "kds-print-agent démarré");

    let listener = tokio::net::TcpListener::bind(addr).await.expect("bind");
    axum::serve(listener, app).await.expect("serve");
}

async fn handle_print(
    State(state): State<AgentState>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    write_to_device(&state.device_path, &body)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn write_to_device(path: &str, data: &[u8]) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .await?;
    file.write_all(data).await?;
    file.flush().await
}
```

- [ ] **Vérifier compilation**
```bash
cargo build -p kds-print-agent 2>&1 | grep error
```

- [ ] **Test manuel : lancer l'agent et envoyer une requête de test**
```bash
# Terminal 1 : démarrer l'agent en mode fichier
PRINTER_DEVICE=/tmp/test-printer.raw cargo run -p kds-print-agent &
sleep 1

# Terminal 2 : envoyer des bytes de test
curl -s -X POST http://127.0.0.1:6611/print \
  -H "Content-Type: application/octet-stream" \
  --data-binary $'\x1b\x40Hello KDS\x0a' \
  -w "%{http_code}"
# Attendu : 204

# Vérifier le fichier
xxd /tmp/test-printer.raw | head -3
# Attendu : ESC @ Hello KDS LF
```

- [ ] **Arrêter l'agent**
```bash
kill %1 2>/dev/null || true
```

- [ ] **Commit**
```bash
git add kds-print-agent/
git commit -m "feat(kds-print-agent): agent USB — Axum POST /print → device"
```

---

## Task 14 : CI verte + self-review finale

- [ ] **Lancer la CI complète**
```bash
cargo fmt --check && \
cargo clippy --workspace -- -D warnings && \
cargo test --workspace && \
cargo build --release
```
Attendu : aucune erreur, aucun warning.

- [ ] **Corriger les éventuels warnings clippy pedantic**

Patterns courants pour ce projet :
- `cast_possible_wrap` → `.cast_signed()`
- `cast_sign_loss` → `.cast_unsigned()`
- `too_many_lines` → `#[allow(clippy::too_many_lines)]` sur les handlers longs
- `items_after_statements` → déplacer les structs au niveau module
- `must_use` manquant → ajouter `#[must_use]` sur les fns pures

- [ ] **Test SSE manuel**
```bash
# Démarrer edge-api
DATABASE_URL=sqlite:./test.db DATA_DIR=./data cargo run -p edge-api &
sleep 2

# Écouter le feed d'une station (terminal 1)
curl -N http://127.0.0.1:8080/api/v1/kds/feed/grill-01

# Tester la config (terminal 2)
curl http://127.0.0.1:8080/api/v1/kds/config
# {"active_profile":"normal"}

curl -X PUT http://127.0.0.1:8080/api/v1/kds/config \
  -H "Content-Type: application/json" \
  -d '{"active_profile":"rush"}'
# 204

curl http://127.0.0.1:8080/api/v1/kds/config
# {"active_profile":"rush"}

kill %1
rm test.db
```

- [ ] **Commit final**
```bash
git add .
git commit -m "chore(kds): CI verte — Plan 1 KDS Backend complet"
```

---

## Résumé des livrables Plan 1

| Livrable | Statut cible |
|---|---|
| `kds-engine` crate (types, routing, state machine, broadcaster) | ✅ |
| Migrations SQLite 0007 + 0008 | ✅ |
| Routes edge-api SSE + ack + served + config + stations | ✅ |
| Hook `pay` → `dispatch_order` | ✅ |
| Formatteurs ESC/POS (receipt + linerless + file WYSIWYG) | ✅ |
| Print dispatcher (TCP/IP + USB agent + file) | ✅ |
| `kds-print-agent` binaire | ✅ |
| CI verte (`fmt` + `clippy -D warnings` + `test`) | ✅ |

**Prochaines étapes :**
- **Plan 2** — `kds-app` (React/Vite : vues préparation, expo, ORB, SSE consumer)
- **Plan 3** — Backoffice cuisine (pages stations/routing/triggers) + Supabase migrations + sync-client pull
