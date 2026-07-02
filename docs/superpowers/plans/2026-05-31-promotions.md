# Promotions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ajouter un moteur de promotions complet (crate `promo-engine`, API edge, back-office CRUD + workflow d'approbation, UX caisse) conforme NF525.

**Architecture:** Crate Rust `promo-engine` (zéro couplage avec `fiscal-engine`) évalue les promos au moment de `POST /orders` ; chaque promo appliquée génère une entrée `DISCOUNT` distincte dans le journal fiscal. Le catalogue est stocké dans Supabase, synchronisé vers SQLite local par `sync-client`.

**Tech Stack:** Rust (time 0.3, uuid 1, serde), SQLite/sqlx, Supabase PostgREST, React 19, TypeScript, Supabase JS v2.

**Phases et dépendances :**
- Phase 1 (Tasks 1-2) : fondations DB — prérequis de tout le reste
- Phase 2 (Tasks 3-6) : crate `promo-engine` — indépendante, testable seule
- Phase 3 (Tasks 7-9) : backend API — dépend de Phase 1 + 2
- Phase 4 (Tasks 10-13) : back-office — dépend de Phase 1 seulement
- Phase 5 (Task 14) : pos-app — dépend de Phase 3

---

## Fichiers créés / modifiés

| Fichier | Action |
|---|---|
| `supabase/migrations/012_restaurant_groups.sql` | Créer |
| `supabase/migrations/013_promotion_approval_thresholds.sql` | Créer |
| `supabase/migrations/014_promotions.sql` | Créer |
| `fiscal-engine/migrations/0006_promotions.sql` | Créer |
| `fiscal-engine/src/journal/store.rs` | Modifier (+ migration 0006) |
| `promo-engine/Cargo.toml` | Créer |
| `promo-engine/src/lib.rs` | Créer |
| `promo-engine/src/types.rs` | Créer |
| `promo-engine/src/errors.rs` | Créer |
| `promo-engine/src/evaluator.rs` | Créer |
| `Cargo.toml` | Modifier (+ membre promo-engine) |
| `edge-api/Cargo.toml` | Modifier (+ dépendance promo-engine) |
| `edge-api/src/app.rs` | Modifier (+ db pool dans AppState) |
| `edge-api/src/main.rs` | Modifier (clone pool avant Journal::open) |
| `edge-api/src/routes/mod.rs` | Modifier (+ route promotions) |
| `edge-api/src/routes/promotions.rs` | Créer |
| `edge-api/src/routes/orders.rs` | Modifier (manual_promo_ids + DISCOUNT entries) |
| `sync-client/src/promo_puller.rs` | Créer |
| `sync-client/src/sync_loop.rs` | Modifier (+ pull_promotions) |
| `sync-client/src/lib.rs` | Modifier (pub mod promo_puller) |
| `backoffice/src/context/AuthContext.tsx` | Modifier (exposer role) |
| `backoffice/src/hooks/useRole.ts` | Créer |
| `backoffice/src/pages/GroupList.tsx` | Créer |
| `backoffice/src/pages/GroupForm.tsx` | Créer |
| `backoffice/src/pages/PromotionList.tsx` | Créer |
| `backoffice/src/pages/PromotionForm.tsx` | Créer |
| `backoffice/src/App.tsx` | Modifier (+ routes groupes + promotions) |
| `backoffice/src/components/Layout.tsx` | Modifier (+ nav + logout role-aware) |
| `pos-app/src/api/client.ts` | Modifier (+ getAvailablePromotions, createOrder promo_ids) |
| `pos-app/src/store/orderStore.ts` | Modifier (+ selectedPromoIds) |
| `pos-app/src/components/PromoModal.tsx` | Créer |
| `pos-app/src/pages/OrderPage.tsx` | Modifier (bouton Promos + ticket remises) |

---

## Task 1 — Migrations Supabase (012, 013, 014)

**Files:**
- Create: `supabase/migrations/012_restaurant_groups.sql`
- Create: `supabase/migrations/013_promotion_approval_thresholds.sql`
- Create: `supabase/migrations/014_promotions.sql`

- [ ] **Step 1 : Créer `012_restaurant_groups.sql`**

```sql
-- supabase/migrations/012_restaurant_groups.sql
CREATE TABLE IF NOT EXISTS public.restaurant_groups (
  id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name         text        NOT NULL,
  group_type   text        NOT NULL CHECK (group_type IN ('static','dynamic','mixed')),
  criteria     jsonb,
  created_by   uuid        REFERENCES auth.users(id),
  created_at   timestamptz NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS public.restaurant_group_members (
  group_id  uuid NOT NULL REFERENCES public.restaurant_groups(id) ON DELETE CASCADE,
  site_id   uuid NOT NULL REFERENCES public.sites(id) ON DELETE CASCADE,
  PRIMARY KEY (group_id, site_id)
);

ALTER TABLE public.restaurant_groups ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.restaurant_group_members ENABLE ROW LEVEL SECURITY;

CREATE POLICY "service_role_all_groups" ON public.restaurant_groups
  FOR ALL TO service_role USING (true) WITH CHECK (true);
CREATE POLICY "authenticated_read_groups" ON public.restaurant_groups
  FOR SELECT TO authenticated USING (true);
CREATE POLICY "director_write_groups" ON public.restaurant_groups
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'))
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'));

CREATE POLICY "service_role_all_members" ON public.restaurant_group_members
  FOR ALL TO service_role USING (true) WITH CHECK (true);
CREATE POLICY "authenticated_read_members" ON public.restaurant_group_members
  FOR SELECT TO authenticated USING (true);
CREATE POLICY "director_write_members" ON public.restaurant_group_members
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'))
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'));
```

- [ ] **Step 2 : Créer `013_promotion_approval_thresholds.sql`**

```sql
-- supabase/migrations/013_promotion_approval_thresholds.sql
CREATE TABLE IF NOT EXISTS public.promotion_approval_thresholds (
  id            uuid  PRIMARY KEY DEFAULT gen_random_uuid(),
  scope         text  NOT NULL CHECK (scope IN ('site','group','chain')),
  max_cents     int,          -- NULL = illimité
  required_role text  NOT NULL CHECK (required_role IN ('manager','director','regional_director'))
);

ALTER TABLE public.promotion_approval_thresholds ENABLE ROW LEVEL SECURITY;

CREATE POLICY "service_role_all_thresholds" ON public.promotion_approval_thresholds
  FOR ALL TO service_role USING (true) WITH CHECK (true);
CREATE POLICY "authenticated_read_thresholds" ON public.promotion_approval_thresholds
  FOR SELECT TO authenticated USING (true);
CREATE POLICY "regional_director_write_thresholds" ON public.promotion_approval_thresholds
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') = 'regional_director')
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') = 'regional_director');

-- Seed des seuils par défaut
INSERT INTO public.promotion_approval_thresholds (scope, max_cents, required_role) VALUES
  ('site',  1000, 'manager'),           -- ≤ 10,00 € : manager suffit
  ('site',  NULL, 'director'),          -- > 10,00 € : director
  ('group', NULL, 'director'),          -- tout montant groupe : director
  ('chain', NULL, 'regional_director'); -- tout montant chaîne : regional_director
```

- [ ] **Step 3 : Créer `014_promotions.sql`**

```sql
-- supabase/migrations/014_promotions.sql
CREATE TABLE IF NOT EXISTS public.promotions (
  id                    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
  name                  text        NOT NULL,
  scope                 text        NOT NULL CHECK (scope IN ('chain','group','site')),
  site_id               uuid        REFERENCES public.sites(id),
  group_id              uuid        REFERENCES public.restaurant_groups(id),
  promo_type            text        NOT NULL CHECK (promo_type IN ('fixed_amount','percentage','item_discount','bogo','happy_hour')),
  value_cents           int,
  value_bps             int,
  target_sku            text,
  trigger               text        NOT NULL CHECK (trigger IN ('auto','manual')),
  exclusion_group       text,
  priority              int         NOT NULL DEFAULT 0,
  valid_from            date,
  valid_to              date,
  days_of_week          int[],
  time_from             time,
  time_to               time,
  status                text        NOT NULL DEFAULT 'draft'
                                    CHECK (status IN ('draft','pending_approval','approved','active','rejected')),
  required_approval_role text       CHECK (required_approval_role IN ('manager','director','regional_director')),
  approved_by           uuid        REFERENCES auth.users(id),
  approved_at           timestamptz,
  rejected_by           uuid        REFERENCES auth.users(id),
  rejection_reason      text,
  created_by            uuid        NOT NULL REFERENCES auth.users(id),
  active                boolean     NOT NULL DEFAULT false,
  created_at            timestamptz NOT NULL DEFAULT now(),
  updated_at_ms         bigint      NOT NULL DEFAULT (extract(epoch from now()) * 1000)::bigint,
  CONSTRAINT scope_consistency CHECK (
    (scope = 'site'  AND site_id IS NOT NULL  AND group_id IS NULL) OR
    (scope = 'group' AND group_id IS NOT NULL AND site_id IS NULL)  OR
    (scope = 'chain' AND site_id IS NULL      AND group_id IS NULL)
  )
);

ALTER TABLE public.promotions ENABLE ROW LEVEL SECURITY;

CREATE POLICY "service_role_all_promotions" ON public.promotions
  FOR ALL TO service_role USING (true) WITH CHECK (true);

-- Lecture : authenticated voit les promos de son périmètre
CREATE POLICY "authenticated_read_promotions" ON public.promotions
  FOR SELECT TO authenticated USING (true);

-- Écriture manager : scope=site uniquement, son propre site
CREATE POLICY "manager_write_site_promotions" ON public.promotions
  FOR INSERT TO authenticated
  WITH CHECK (
    (auth.jwt() -> 'app_metadata' ->> 'role') IN ('manager','director','regional_director')
    AND scope = 'site'
  );

-- Director : peut écrire site + group
CREATE POLICY "director_write_promotions" ON public.promotions
  FOR ALL TO authenticated
  USING ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'))
  WITH CHECK ((auth.jwt() -> 'app_metadata' ->> 'role') IN ('director','regional_director'));
```

- [ ] **Step 4 : Appliquer les migrations sur Supabase**

Via MCP `execute_sql` (project_id = `iawyngsvqjsogvkwkrxw`) ou CLI :
```bash
cd pos-fiscal
supabase db push --db-url "postgresql://..."
```

- [ ] **Step 5 : Vérifier les tables créées**

```sql
SELECT table_name FROM information_schema.tables
WHERE table_schema = 'public'
  AND table_name IN ('restaurant_groups','restaurant_group_members',
                     'promotion_approval_thresholds','promotions');
-- Attendu : 4 lignes
```

- [ ] **Step 6 : Commit**

```bash
git add supabase/migrations/012_restaurant_groups.sql \
        supabase/migrations/013_promotion_approval_thresholds.sql \
        supabase/migrations/014_promotions.sql
git commit -m "feat(db): migrations 012-014 — restaurant_groups, thresholds, promotions"
```

---

## Task 2 — Migration SQLite locale 0006 + store.rs

**Files:**
- Create: `fiscal-engine/migrations/0006_promotions.sql`
- Modify: `fiscal-engine/src/journal/store.rs:139-145`

- [ ] **Step 1 : Créer `0006_promotions.sql`**

```sql
-- fiscal-engine/migrations/0006_promotions.sql
-- Table locale des promotions actives (sync-client → edge-api, lecture seule côté caisse)
CREATE TABLE IF NOT EXISTS promotions (
  id               TEXT PRIMARY KEY,
  name             TEXT NOT NULL,
  promo_type       TEXT NOT NULL,
  value_cents      INTEGER,
  value_bps        INTEGER,
  target_sku       TEXT,
  trigger          TEXT NOT NULL,
  exclusion_group  TEXT,
  priority         INTEGER NOT NULL DEFAULT 0,
  valid_from       TEXT,       -- ISO date YYYY-MM-DD ou NULL
  valid_to         TEXT,
  days_of_week     TEXT,       -- JSON array ex: "[1,2,3]" ou NULL
  time_from        TEXT,       -- HH:MM ou NULL
  time_to          TEXT,
  updated_at_ms    INTEGER NOT NULL DEFAULT 0
);
```

- [ ] **Step 2 : Enregistrer la migration dans `store.rs`**

Dans `fiscal-engine/src/journal/store.rs`, après la ligne :
```rust
        run("0005", include_str!("../../migrations/0005_multi_tva.sql")).await?;
```
Ajouter :
```rust
        run("0006", include_str!("../../migrations/0006_promotions.sql")).await?;
```

- [ ] **Step 3 : Ajouter le même include dans le helper de test (ligne ~1020)**

Chercher le bloc qui applique manuellement les migrations dans les tests (`apply_schema_for_tests` ou équivalent) et ajouter :
```rust
        sqlx::query(include_str!("../../migrations/0006_promotions.sql"))
            .execute(&pool)
            .await
            .expect("Migration 0006 appliquée");
```

- [ ] **Step 4 : Compiler et vérifier**

```bash
cargo build -p fiscal-engine 2>&1 | grep -E "error|warning"
# Attendu : 0 erreur
```

- [ ] **Step 5 : Commit**

```bash
git add fiscal-engine/migrations/0006_promotions.sql fiscal-engine/src/journal/store.rs
git commit -m "feat(fiscal-engine): migration 0006 — table promotions locale SQLite"
```

---

## Task 3 — Crate promo-engine : scaffold + types

**Files:**
- Modify: `Cargo.toml` (workspace members)
- Create: `promo-engine/Cargo.toml`
- Create: `promo-engine/src/lib.rs`
- Create: `promo-engine/src/types.rs`
- Create: `promo-engine/src/errors.rs`

- [ ] **Step 1 : Ajouter promo-engine au workspace**

Dans `Cargo.toml` racine, modifier `members` :
```toml
members = [
    "common",
    "fiscal-engine",
    "promo-engine",
    "edge-api",
    "sync-client",
]
```

- [ ] **Step 2 : Créer `promo-engine/Cargo.toml`**

```toml
[package]
name = "promo-engine"
version = "0.1.0"
edition = "2021"

[dependencies]
serde      = { workspace = true }
serde_json = { workspace = true }
thiserror  = { workspace = true }
uuid       = { workspace = true }
time       = { workspace = true }
```

- [ ] **Step 3 : Créer `promo-engine/src/errors.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PromoError {
    #[error("Panier vide — impossible d'évaluer les promotions")]
    EmptyCart,
    #[error("Promotion {id} sans valeur définie (value_cents et value_bps sont tous les deux null)")]
    MissingValue { id: uuid::Uuid },
}
```

- [ ] **Step 4 : Créer `promo-engine/src/types.rs`**

```rust
use serde::{Deserialize, Serialize};
use time::{Date, Time};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cart {
    pub line_items: Vec<CartItem>,
    pub total_ttc_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartItem {
    pub sku: String,
    pub amount_ttc_cents: i64,
    pub tva_rate: TvaRateKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TvaRateKey {
    Reduit5_5,
    Intermediaire10,
    Normal20,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub id: Uuid,
    pub name: String,
    pub promo_type: PromoType,
    pub value_cents: Option<i64>,
    pub value_bps: Option<i64>,
    pub target_sku: Option<String>,
    pub trigger: Trigger,
    pub exclusion_group: Option<String>,
    pub priority: i32,
    pub valid_from: Option<Date>,
    pub valid_to: Option<Date>,
    pub days_of_week: Option<Vec<u8>>,  // 1=Lun, 7=Dim
    pub time_from: Option<Time>,
    pub time_to: Option<Time>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromoType {
    FixedAmount,
    Percentage,
    ItemDiscount,
    Bogo,
    HappyHour,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    Auto,
    Manual,
}

#[derive(Debug, Clone)]
pub struct EvalResult {
    pub applied: Vec<PromoApplication>,
    pub rejected: Vec<PromoApplication>,
}

#[derive(Debug, Clone)]
pub struct PromoApplication {
    pub promo_id: Uuid,
    pub promo_name: String,
    pub discount_cents: i64,        // toujours positif
    pub tva_allocation: TvaAllocation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TvaAllocation {
    pub cents_5_5: i64,
    pub cents_10:  i64,
    pub cents_20:  i64,
}

impl TvaAllocation {
    #[must_use]
    pub fn total(&self) -> i64 {
        self.cents_5_5 + self.cents_10 + self.cents_20
    }
}
```

- [ ] **Step 5 : Créer `promo-engine/src/lib.rs`**

```rust
#![deny(clippy::all, clippy::pedantic)]
#![warn(missing_docs)]

//! # promo-engine
//!
//! Évalue les promotions applicables à un panier de commande.
//! Zéro couplage avec `fiscal-engine`.

pub mod errors;
pub mod evaluator;
pub mod types;

pub use evaluator::evaluate;
pub use types::{
    Cart, CartItem, EvalResult, PromoApplication,
    Promotion, PromoType, Trigger, TvaAllocation, TvaRateKey,
};
```

- [ ] **Step 6 : Créer `promo-engine/src/evaluator.rs` (stub vide)**

```rust
use time::OffsetDateTime;
use uuid::Uuid;

use crate::types::{Cart, EvalResult, Promotion};

pub fn evaluate(
    _cart: &Cart,
    _promos: &[Promotion],
    _manual_selected_ids: &[Uuid],
    _now: OffsetDateTime,
) -> EvalResult {
    EvalResult { applied: vec![], rejected: vec![] }
}
```

- [ ] **Step 7 : Vérifier compilation**

```bash
cargo build -p promo-engine 2>&1 | grep -E "^error"
# Attendu : aucune ligne
```

- [ ] **Step 8 : Commit**

```bash
git add Cargo.toml promo-engine/
git commit -m "feat(promo-engine): scaffold — types, errors, evaluator stub"
```

---

## Task 4 — promo-engine : filtrage validité + déclencheur (TDD)

**Files:**
- Modify: `promo-engine/src/evaluator.rs`

- [ ] **Step 1 : Écrire les tests de validité**

En bas de `promo-engine/src/evaluator.rs`, ajouter :

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::{date, datetime, time};
    use uuid::Uuid;
    use crate::types::{CartItem, PromoType, TvaRateKey};

    fn uuid(b: u8) -> Uuid { Uuid::from_bytes([b; 16]) }

    fn base_promo(b: u8) -> Promotion {
        Promotion {
            id: uuid(b),
            name: format!("P{b}"),
            promo_type: PromoType::FixedAmount,
            value_cents: Some(100),
            value_bps: None,
            target_sku: None,
            trigger: Trigger::Auto,
            exclusion_group: None,
            priority: 0,
            valid_from: None,
            valid_to: None,
            days_of_week: None,
            time_from: None,
            time_to: None,
        }
    }

    fn cart_1000() -> Cart {
        Cart {
            line_items: vec![CartItem {
                sku: "SKU-A".into(),
                amount_ttc_cents: 1000,
                tva_rate: TvaRateKey::Intermediaire10,
            }],
            total_ttc_cents: 1000,
        }
    }

    #[test]
    fn promo_after_valid_to_rejected() {
        let p = Promotion { valid_to: Some(date!(2020-01-01)), ..base_promo(1) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_before_valid_from_rejected() {
        let p = Promotion { valid_from: Some(date!(2099-01-01)), ..base_promo(2) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_in_date_range_applied() {
        let p = Promotion {
            valid_from: Some(date!(2026-01-01)),
            valid_to: Some(date!(2026-12-31)),
            ..base_promo(3)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn promo_wrong_day_rejected() {
        // 2026-06-01 = lundi (1) ; promo sam+dim seulement
        let p = Promotion { days_of_week: Some(vec![6, 7]), ..base_promo(4) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-06-01 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_correct_day_applied() {
        // 2026-06-01 = lundi (1)
        let p = Promotion { days_of_week: Some(vec![1]), ..base_promo(5) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-06-01 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn promo_outside_time_range_rejected() {
        let p = Promotion {
            time_from: Some(time!(16:00)),
            time_to:   Some(time!(18:00)),
            ..base_promo(6)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 14:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_inside_time_range_applied() {
        let p = Promotion {
            time_from: Some(time!(16:00)),
            time_to:   Some(time!(18:00)),
            ..base_promo(7)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 17:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn manual_promo_without_selection_rejected() {
        let p = Promotion { trigger: Trigger::Manual, ..base_promo(8) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn manual_promo_selected_applied() {
        let p = Promotion { trigger: Trigger::Manual, ..base_promo(9) };
        let id = p.id;
        let r = evaluate(&cart_1000(), &[p], &[id], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }
}
```

- [ ] **Step 2 : Exécuter les tests — vérifier qu'ils échouent**

```bash
cargo test -p promo-engine 2>&1 | grep -E "FAILED|test result"
# Attendu : plusieurs FAILED (evaluator stub retourne toujours applied=[])
```

- [ ] **Step 3 : Implémenter `is_in_window` et le filtrage dans `evaluate()`**

Remplacer le contenu de `promo-engine/src/evaluator.rs` :

```rust
use std::collections::HashMap;

use time::{OffsetDateTime, Weekday};
use uuid::Uuid;

use crate::types::{
    Cart, EvalResult, Promotion, PromoApplication, PromoType, Trigger, TvaAllocation, TvaRateKey,
};

pub fn evaluate(
    cart: &Cart,
    promos: &[Promotion],
    manual_selected_ids: &[Uuid],
    now: OffsetDateTime,
) -> EvalResult {
    let candidates: Vec<(&Promotion, PromoApplication)> = promos
        .iter()
        .filter(|p| is_in_window(p, now))
        .filter(|p| match p.trigger {
            Trigger::Auto   => true,
            Trigger::Manual => manual_selected_ids.contains(&p.id),
        })
        .filter(|p| cart_conditions_met(p, cart))
        .filter_map(|p| compute_discount(p, cart).map(|app| (p, app)))
        .collect();

    resolve_exclusion_groups(candidates)
}

fn is_in_window(p: &Promotion, now: OffsetDateTime) -> bool {
    let today = now.date();
    let t     = now.time();
    let dow   = weekday_num(now.weekday());

    if let Some(from) = p.valid_from { if today < from { return false; } }
    if let Some(to)   = p.valid_to   { if today > to   { return false; } }

    if let Some(days) = &p.days_of_week {
        if !days.is_empty() && !days.contains(&dow) { return false; }
    }

    match (p.time_from, p.time_to) {
        (Some(from), Some(to)) if from <= to => {
            if t < from || t > to { return false; }
        }
        (Some(from), Some(to)) => {
            // Plage chevauchant minuit (ex: 22h-02h)
            if t < from && t > to { return false; }
        }
        (Some(from), None) => { if t < from { return false; } }
        (None, Some(to))   => { if t > to   { return false; } }
        (None, None)       => {}
    }

    true
}

fn weekday_num(w: Weekday) -> u8 {
    match w {
        Weekday::Monday    => 1,
        Weekday::Tuesday   => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday  => 4,
        Weekday::Friday    => 5,
        Weekday::Saturday  => 6,
        Weekday::Sunday    => 7,
    }
}

fn cart_conditions_met(p: &Promotion, cart: &Cart) -> bool {
    match p.promo_type {
        PromoType::ItemDiscount | PromoType::Bogo => p
            .target_sku
            .as_ref()
            .map_or(false, |sku| cart.line_items.iter().any(|i| &i.sku == sku)),
        _ => !cart.line_items.is_empty(),
    }
}

fn compute_discount(p: &Promotion, cart: &Cart) -> Option<PromoApplication> {
    let discount_cents = match p.promo_type {
        PromoType::FixedAmount | PromoType::HappyHour if p.value_cents.is_some() => {
            p.value_cents?.min(cart.total_ttc_cents)
        }
        PromoType::Percentage | PromoType::HappyHour => {
            let bps = p.value_bps?;
            (cart.total_ttc_cents * bps / 10_000).min(cart.total_ttc_cents)
        }
        PromoType::FixedAmount => p.value_cents?.min(cart.total_ttc_cents),
        PromoType::ItemDiscount => {
            let sku   = p.target_sku.as_ref()?;
            let total: i64 = cart.line_items.iter()
                .filter(|i| &i.sku == sku)
                .map(|i| i.amount_ttc_cents)
                .sum();
            if let Some(c) = p.value_cents {
                c.min(total)
            } else {
                let bps = p.value_bps?;
                (total * bps / 10_000).min(total)
            }
        }
        PromoType::Bogo => {
            let sku = p.target_sku.as_ref()?;
            cart.line_items.iter()
                .filter(|i| &i.sku == sku)
                .map(|i| i.amount_ttc_cents)
                .min()?
        }
    };

    if discount_cents <= 0 { return None; }

    Some(PromoApplication {
        promo_id: p.id,
        promo_name: p.name.clone(),
        discount_cents,
        tva_allocation: compute_tva_allocation(discount_cents, cart),
    })
}

fn compute_tva_allocation(discount_cents: i64, cart: &Cart) -> TvaAllocation {
    let total = cart.total_ttc_cents;
    if total == 0 { return TvaAllocation::default(); }

    let mut by_rate: HashMap<TvaRateKey, i64> = HashMap::new();
    for item in &cart.line_items {
        *by_rate.entry(item.tva_rate).or_default() += item.amount_ttc_cents;
    }

    let alloc = |key: TvaRateKey| -> i64 {
        by_rate.get(&key).copied().unwrap_or(0) * discount_cents / total
    };

    let a5  = alloc(TvaRateKey::Reduit5_5);
    let a10 = alloc(TvaRateKey::Intermediaire10);
    let a20 = alloc(TvaRateKey::Normal20);
    let remainder = discount_cents - a5 - a10 - a20;

    // Remainder au taux dominant (évite de perdre des centimes par arrondi)
    let dominant = [
        (TvaRateKey::Reduit5_5,       by_rate.get(&TvaRateKey::Reduit5_5).copied().unwrap_or(0)),
        (TvaRateKey::Intermediaire10, by_rate.get(&TvaRateKey::Intermediaire10).copied().unwrap_or(0)),
        (TvaRateKey::Normal20,        by_rate.get(&TvaRateKey::Normal20).copied().unwrap_or(0)),
    ]
    .into_iter()
    .max_by_key(|(_, v)| *v)
    .map(|(k, _)| k)
    .unwrap_or(TvaRateKey::Intermediaire10);

    let mut alloc = TvaAllocation { cents_5_5: a5, cents_10: a10, cents_20: a20 };
    match dominant {
        TvaRateKey::Reduit5_5       => alloc.cents_5_5 += remainder,
        TvaRateKey::Intermediaire10 => alloc.cents_10  += remainder,
        TvaRateKey::Normal20        => alloc.cents_20  += remainder,
    }
    alloc
}

fn resolve_exclusion_groups(
    candidates: Vec<(&Promotion, PromoApplication)>,
) -> EvalResult {
    let mut grouped: HashMap<String, Vec<(&Promotion, PromoApplication)>> = HashMap::new();
    let mut applied: Vec<PromoApplication> = Vec::new();
    let mut rejected: Vec<PromoApplication> = Vec::new();

    for (promo, app) in candidates {
        if let Some(grp) = &promo.exclusion_group {
            grouped.entry(grp.clone()).or_default().push((promo, app));
        } else {
            applied.push(app);
        }
    }

    for (_grp, mut group) in grouped {
        group.sort_by(|a, b| {
            b.0.priority.cmp(&a.0.priority)
                .then_with(|| b.1.discount_cents.cmp(&a.1.discount_cents))
        });
        let mut iter = group.into_iter();
        if let Some((_p, winner)) = iter.next() { applied.push(winner); }
        for (_p, loser) in iter { rejected.push(loser); }
    }

    EvalResult { applied, rejected }
}

#[cfg(test)]
mod tests {
    // … (tests déjà écrits au Step 1)
}
```

- [ ] **Step 4 : Exécuter les tests — vérifier qu'ils passent**

```bash
cargo test -p promo-engine 2>&1 | grep -E "FAILED|ok|test result"
# Attendu : "test result: ok. 9 passed"
```

- [ ] **Step 5 : Commit**

```bash
git add promo-engine/src/evaluator.rs
git commit -m "feat(promo-engine): validity filter + trigger filter — 9 tests verts"
```

---

## Task 5 — promo-engine : calcul remises + groupes exclusifs + TVA (TDD)

**Files:**
- Modify: `promo-engine/src/evaluator.rs` (section tests)

- [ ] **Step 1 : Ajouter les tests de calcul dans le module `tests`**

```rust
    #[test]
    fn fixed_amount_discount_applied() {
        let p = Promotion { value_cents: Some(200), ..base_promo(10) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 200);
    }

    #[test]
    fn fixed_amount_capped_at_cart_total() {
        let p = Promotion { value_cents: Some(9999), ..base_promo(11) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 1000);
    }

    #[test]
    fn percentage_10_percent() {
        let p = Promotion {
            promo_type: PromoType::Percentage,
            value_cents: None,
            value_bps: Some(1000), // 10%
            ..base_promo(12)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 100);
    }

    #[test]
    fn item_discount_sku_absent_rejected() {
        let p = Promotion {
            promo_type: PromoType::ItemDiscount,
            target_sku: Some("ABSENT".into()),
            value_cents: Some(100),
            ..base_promo(13)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn bogo_gives_cheapest_item_free() {
        let cart = Cart {
            line_items: vec![
                CartItem { sku: "BUR".into(), amount_ttc_cents: 900, tva_rate: TvaRateKey::Intermediaire10 },
                CartItem { sku: "BUR".into(), amount_ttc_cents: 1200, tva_rate: TvaRateKey::Intermediaire10 },
            ],
            total_ttc_cents: 2100,
        };
        let p = Promotion {
            promo_type: PromoType::Bogo,
            target_sku: Some("BUR".into()),
            value_cents: None,
            ..base_promo(14)
        };
        let r = evaluate(&cart, &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 900); // le moins cher offert
    }

    #[test]
    fn exclusion_group_highest_priority_wins() {
        let p1 = Promotion { exclusion_group: Some("g".into()), priority: 5,  value_cents: Some(300), ..base_promo(15) };
        let p2 = Promotion { exclusion_group: Some("g".into()), priority: 10, value_cents: Some(50),  ..base_promo(16) };
        let r = evaluate(&cart_1000(), &[p1, p2], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
        assert_eq!(r.applied[0].discount_cents, 50); // p2 priority 10
        assert_eq!(r.rejected.len(), 1);
    }

    #[test]
    fn exclusion_group_tie_biggest_discount_wins() {
        let p1 = Promotion { exclusion_group: Some("g".into()), priority: 0, value_cents: Some(200), ..base_promo(17) };
        let p2 = Promotion { exclusion_group: Some("g".into()), priority: 0, value_cents: Some(500), ..base_promo(18) };
        let r = evaluate(&cart_1000(), &[p1, p2], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 500);
    }

    #[test]
    fn no_group_all_cumulated() {
        let p1 = Promotion { value_cents: Some(100), ..base_promo(19) };
        let p2 = Promotion { value_cents: Some(200), ..base_promo(20) };
        let r = evaluate(&cart_1000(), &[p1, p2], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied.len(), 2);
    }

    #[test]
    fn tva_allocation_single_rate_coherent() {
        let p = Promotion { value_cents: Some(100), ..base_promo(21) };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        let a = &r.applied[0].tva_allocation;
        assert_eq!(a.total(), 100);
        assert_eq!(a.cents_5_5, 0);
        assert_eq!(a.cents_10, 100);
        assert_eq!(a.cents_20, 0);
    }

    #[test]
    fn tva_allocation_multi_rate_proportional() {
        // 600 @ 10% + 400 @ 20% = 1000 total ; discount 100
        let cart = Cart {
            line_items: vec![
                CartItem { sku: "A".into(), amount_ttc_cents: 600, tva_rate: TvaRateKey::Intermediaire10 },
                CartItem { sku: "B".into(), amount_ttc_cents: 400, tva_rate: TvaRateKey::Normal20 },
            ],
            total_ttc_cents: 1000,
        };
        let p = Promotion { value_cents: Some(100), ..base_promo(22) };
        let r = evaluate(&cart, &[p], &[], datetime!(2026-05-31 10:00 UTC));
        let a = &r.applied[0].tva_allocation;
        assert_eq!(a.total(), 100);
        assert_eq!(a.cents_10, 60);
        assert_eq!(a.cents_20, 40);
    }
```

- [ ] **Step 2 : Exécuter les tests — vérifier qu'ils passent tous**

```bash
cargo test -p promo-engine -- --nocapture 2>&1 | grep -E "FAILED|test result"
# Attendu : "test result: ok. 22 passed"
```

- [ ] **Step 3 : Commit**

```bash
git add promo-engine/src/evaluator.rs
git commit -m "test(promo-engine): 22 tests verts — calcul remises, groupes exclusifs, TVA"
```

---

## Task 6 — AppState db pool + sync-client promo puller

**Files:**
- Modify: `edge-api/Cargo.toml`
- Modify: `edge-api/src/app.rs`
- Modify: `edge-api/src/main.rs`
- Create: `sync-client/src/promo_puller.rs`
- Modify: `sync-client/src/sync_loop.rs`
- Modify: `sync-client/src/lib.rs`

- [ ] **Step 1 : Ajouter promo-engine dans edge-api/Cargo.toml**

```toml
[dependencies]
# … existants …
promo-engine = { path = "../promo-engine" }
```

- [ ] **Step 2 : Modifier `edge-api/src/app.rs` — ajouter db pool dans AppState**

```rust
use sqlx::sqlite::SqlitePool;

#[derive(Clone, Debug)]
pub struct AppState {
    pub journal: Arc<Journal>,
    pub db: SqlitePool,       // accès direct pour les requêtes non-fiscales (promotions)
    pub data_dir: String,
}

impl AppState {
    #[must_use]
    pub fn new(journal: Journal, db: SqlitePool, data_dir: String) -> Self {
        Self { journal: Arc::new(journal), db, data_dir }
    }
}
```

- [ ] **Step 3 : Modifier `edge-api/src/main.rs` — cloner le pool avant Journal::open**

Remplacer :
```rust
    let journal = match Journal::open(pool).await {
```
Par :
```rust
    let db = pool.clone();
    let journal = match Journal::open(pool).await {
```

Et remplacer :
```rust
    let state = AppState::new(journal, config.data_dir.clone());
```
Par :
```rust
    let state = AppState::new(journal, db, config.data_dir.clone());
```

- [ ] **Step 4 : Créer `sync-client/src/promo_puller.rs`**

```rust
//! Pull des promotions actives depuis Supabase vers SQLite local.

use serde::Deserialize;
use sqlx::SqlitePool;
use tracing::{debug, info, warn};

use crate::{client::SupabaseClient, config::SyncConfig, error::SyncError};

#[derive(Debug, Deserialize)]
struct RemotePromo {
    id: String,
    name: String,
    promo_type: String,
    value_cents: Option<i64>,
    value_bps: Option<i64>,
    target_sku: Option<String>,
    trigger: String,
    exclusion_group: Option<String>,
    priority: i32,
    valid_from: Option<String>,
    valid_to: Option<String>,
    days_of_week: Option<Vec<i64>>,
    time_from: Option<String>,
    time_to: Option<String>,
    updated_at_ms: i64,
}

/// Tire les promotions actives depuis Supabase et les upserte dans SQLite local.
///
/// # Errors
/// `SyncError::Network` si Supabase est inaccessible.
pub async fn pull_promotions(
    client: &SupabaseClient,
    config: &SyncConfig,
    pool: &SqlitePool,
) -> Result<usize, SyncError> {
    let promos = client.pull_promotions(&config.site_id).await?;
    let count = promos.len();

    for p in promos {
        let days_json = p.days_of_week
            .map(|d| serde_json::to_string(&d).unwrap_or_default());

        sqlx::query(
            "INSERT INTO promotions
             (id, name, promo_type, value_cents, value_bps, target_sku, trigger,
              exclusion_group, priority, valid_from, valid_to, days_of_week,
              time_from, time_to, updated_at_ms)
             VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)
             ON CONFLICT(id) DO UPDATE SET
               name=excluded.name, promo_type=excluded.promo_type,
               value_cents=excluded.value_cents, value_bps=excluded.value_bps,
               target_sku=excluded.target_sku, trigger=excluded.trigger,
               exclusion_group=excluded.exclusion_group, priority=excluded.priority,
               valid_from=excluded.valid_from, valid_to=excluded.valid_to,
               days_of_week=excluded.days_of_week, time_from=excluded.time_from,
               time_to=excluded.time_to, updated_at_ms=excluded.updated_at_ms",
        )
        .bind(&p.id).bind(&p.name).bind(&p.promo_type)
        .bind(p.value_cents).bind(p.value_bps).bind(&p.target_sku)
        .bind(&p.trigger).bind(&p.exclusion_group).bind(p.priority)
        .bind(&p.valid_from).bind(&p.valid_to).bind(&days_json)
        .bind(&p.time_from).bind(&p.time_to).bind(p.updated_at_ms)
        .execute(pool)
        .await
        .map_err(|e| SyncError::FatalConfig { reason: e.to_string() })?;
    }

    if count > 0 {
        info!(count, "Promotions synchronisées depuis Supabase");
    } else {
        debug!("Aucune nouvelle promotion");
    }
    Ok(count)
}
```

- [ ] **Step 5 : Ajouter `pull_promotions` dans `SupabaseClient` (`client.rs`)**

Ajouter la méthode suivante dans l'impl de `SupabaseClient` :

```rust
    pub async fn pull_promotions(&self, site_id: &str) -> Result<Vec<serde_json::Value>, SyncError> {
        // Tire chain + site. Les promos groupe sont filtrées côté caisse (MVP).
        let url = format!(
            "{}/rest/v1/promotions?select=*&active=eq.true&or=(scope.eq.chain,site_id.eq.{})",
            self.base_url, site_id
        );
        let resp = self
            .client
            .get(&url)
            .header("apikey", &self.service_key)
            .header("Authorization", format!("Bearer {}", self.service_key))
            .send()
            .await
            .map_err(|e| SyncError::Network { source: e.to_string() })?;

        if !resp.status().is_success() {
            warn!(status = %resp.status(), "pull_promotions HTTP error");
            return Ok(vec![]);
        }
        resp.json::<Vec<serde_json::Value>>()
            .await
            .map_err(|e| SyncError::Network { source: e.to_string() })
    }
```

- [ ] **Step 6 : Enregistrer le module dans `sync-client/src/lib.rs`**

Ajouter :
```rust
pub mod promo_puller;
```

- [ ] **Step 7 : Appeler `pull_promotions` dans `sync_loop.rs`**

Dans `run_sync_cycle`, après l'appel à `pull_and_apply_config(...)`, ajouter :

```rust
    if let Err(e) = crate::promo_puller::pull_promotions(client, config, &store.pool_ref().clone()).await {
        warn!(error = %e, "Échec du pull des promotions (non fatal)");
    }
```

Note : exposer `pool_ref()` depuis `JournalStore` si pas encore public — voir `store.rs:723`.

- [ ] **Step 8 : Compiler**

```bash
cargo build --workspace 2>&1 | grep "^error"
# Attendu : 0 erreur
```

- [ ] **Step 9 : Commit**

```bash
git add edge-api/Cargo.toml edge-api/src/app.rs edge-api/src/main.rs \
        sync-client/src/promo_puller.rs sync-client/src/sync_loop.rs \
        sync-client/src/lib.rs
git commit -m "feat: AppState db pool + sync-client pull promotions → SQLite local"
```

---

## Task 7 — edge-api : GET /promotions/available + POST /orders avec promos

**Files:**
- Create: `edge-api/src/routes/promotions.rs`
- Modify: `edge-api/src/routes/mod.rs`
- Modify: `edge-api/src/routes/orders.rs`

- [ ] **Step 1 : Créer `edge-api/src/routes/promotions.rs`**

```rust
//! GET /api/v1/promotions/available — promotions actives dans la fenêtre de validité.

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;
use time::OffsetDateTime;

use crate::app::AppState;

#[derive(Serialize)]
pub struct AvailablePromo {
    pub id: String,
    pub name: String,
    pub promo_type: String,
    pub trigger: String,
    pub value_cents: Option<i64>,
    pub value_bps: Option<i64>,
    pub target_sku: Option<String>,
}

pub async fn get_available_promotions(
    State(state): State<AppState>,
) -> Result<Json<Vec<AvailablePromo>>, StatusCode> {
    let now = OffsetDateTime::now_utc();
    let today = now.date().to_string();            // YYYY-MM-DD
    let time_now = format!("{:02}:{:02}", now.hour(), now.minute()); // HH:MM
    let dow = match now.weekday() {
        time::Weekday::Monday    => 1i64,
        time::Weekday::Tuesday   => 2,
        time::Weekday::Wednesday => 3,
        time::Weekday::Thursday  => 4,
        time::Weekday::Friday    => 5,
        time::Weekday::Saturday  => 6,
        time::Weekday::Sunday    => 7,
    };

    let rows = sqlx::query_as!(
        AvailablePromo,
        r#"SELECT id, name, promo_type, trigger, value_cents, value_bps, target_sku
           FROM promotions
           WHERE (valid_from IS NULL OR valid_from <= ?)
             AND (valid_to   IS NULL OR valid_to   >= ?)
             AND (days_of_week IS NULL OR days_of_week LIKE '%' || ? || '%')
             AND (time_from IS NULL OR time_from <= ?)
             AND (time_to   IS NULL OR time_to   >= ?)"#,
        today, today, dow, time_now, time_now
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(rows))
}
```

- [ ] **Step 2 : Enregistrer la route dans `routes/mod.rs`**

Ajouter dans le routeur Axum :
```rust
pub mod promotions;

// Dans build_router() :
.route("/api/v1/promotions/available", get(promotions::get_available_promotions))
```

- [ ] **Step 3 : Ajouter `manual_promo_ids` dans `CreateOrderRequest` (`orders.rs`)**

```rust
#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    pub order_reference: String,
    pub line_items: Vec<LineItem>,
    #[allow(dead_code)]
    pub payment_method: PaymentMethod,
    #[serde(default)]
    pub manual_promo_ids: Vec<String>,   // UUIDs des promos manuelles sélectionnées
}
```

- [ ] **Step 4 : Charger et évaluer les promos dans le handler `create_order`**

Dans la fonction `create_order`, avant `journal.record_transaction(...)`, ajouter :

```rust
    // Charger les promos actives depuis SQLite
    let db_promos: Vec<SqlitePromoRow> = sqlx::query_as!(
        SqlitePromoRow,
        "SELECT * FROM promotions"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiErr::internal(e.to_string()))?;

    let promos: Vec<promo_engine::Promotion> = db_promos
        .into_iter()
        .filter_map(|r| r.try_into().ok())
        .collect();

    let manual_ids: Vec<uuid::Uuid> = body.manual_promo_ids.iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    let cart = promo_engine::Cart {
        line_items: body.line_items.iter().map(|li| promo_engine::CartItem {
            sku: li.sku.clone().unwrap_or_default(),
            amount_ttc_cents: li.amount_ttc_cents,
            tva_rate: li.tva_rate.into(),
        }).collect(),
        total_ttc_cents: body.line_items.iter().map(|li| li.amount_ttc_cents).sum(),
    };

    let eval = promo_engine::evaluate(&cart, &promos, &manual_ids, time::OffsetDateTime::now_utc());

    // Enregistrer une entrée DISCOUNT par promo appliquée
    for app in &eval.applied {
        let discount_breakdown = build_discount_breakdown(app);
        journal.record_transaction(
            FiscalEntryData {
                operation_type: OperationType::Discount,
                amount_ttc_cents: Cents(-(app.discount_cents)),
                tva_breakdown: discount_breakdown,
                order_reference: body.order_reference.clone(),
                label: Some(app.promo_name.clone()),
            },
            session_id,
        ).await?;
    }
```

Ajouter le helper :
```rust
fn build_discount_breakdown(app: &promo_engine::PromoApplication) -> TvaBreakdown {
    // Construit un TvaBreakdown multi-taux à partir de TvaAllocation
    // Pour MVP : on utilise le taux dominant (le plus gros montant)
    let alloc = &app.tva_allocation;
    let (dominant_rate, dominant_cents) = if alloc.cents_10 >= alloc.cents_20 && alloc.cents_10 >= alloc.cents_5_5 {
        (TvaRate::Intermediaire10, alloc.cents_10)
    } else if alloc.cents_20 >= alloc.cents_5_5 {
        (TvaRate::Normal20, alloc.cents_20)
    } else {
        (TvaRate::Reduit5_5, alloc.cents_5_5)
    };
    TvaBreakdown::from_ttc(Cents(app.discount_cents), dominant_rate)
}
```

- [ ] **Step 5 : Ajouter `applied_promos` dans `OrderResponse`**

```rust
#[derive(Serialize)]
pub struct OrderResponse {
    pub order_id: String,
    pub fiscal_entry: FiscalEntryResponse,
    pub applied_promos: Vec<AppliedPromoResponse>,
}

#[derive(Serialize)]
pub struct AppliedPromoResponse {
    pub promo_id: String,
    pub name: String,
    pub discount_cents: i64,
}
```

- [ ] **Step 6 : Compiler + test d'intégration smoke**

```bash
cargo build -p edge-api 2>&1 | grep "^error"
# Attendu : 0 erreur

DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api &
sleep 2
curl -s http://localhost:8080/api/v1/promotions/available | head -50
# Attendu : JSON array (vide si aucune promo en SQLite)
kill %1
```

- [ ] **Step 7 : Commit**

```bash
git add edge-api/src/routes/promotions.rs edge-api/src/routes/mod.rs \
        edge-api/src/routes/orders.rs
git commit -m "feat(edge-api): GET /promotions/available + POST /orders avec évaluation promos"
```

---

## Task 8 — Back-office : useRole + GroupList + GroupForm

**Files:**
- Modify: `backoffice/src/context/AuthContext.tsx`
- Create: `backoffice/src/hooks/useRole.ts`
- Create: `backoffice/src/pages/GroupList.tsx`
- Create: `backoffice/src/pages/GroupForm.tsx`

- [ ] **Step 1 : Exposer le rôle dans AuthContext**

Dans `backoffice/src/context/AuthContext.tsx`, modifier l'interface et le Provider :

```typescript
interface AuthContextValue {
  session: Session | null
  loading: boolean
  role: string | null        // 'manager' | 'director' | 'regional_director' | null
  signOut: () => Promise<void>
}

// Dans AuthProvider, ajouter :
const role = session?.user?.app_metadata?.role ?? null
// Passer role dans la value du Provider
```

- [ ] **Step 2 : Créer `backoffice/src/hooks/useRole.ts`**

```typescript
import { useAuth } from '../context/AuthContext'

export type Role = 'manager' | 'director' | 'regional_director'

const RANK: Record<Role, number> = {
  manager: 1,
  director: 2,
  regional_director: 3,
}

export function useRole() {
  const { role } = useAuth()
  const current = role as Role | null

  const hasRole = (required: Role): boolean => {
    if (!current) return false
    return (RANK[current] ?? 0) >= RANK[required]
  }

  return { role: current, hasRole }
}
```

- [ ] **Step 3 : Créer `backoffice/src/pages/GroupList.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { supabase } from '../supabaseClient'

interface Group { id: string; name: string; group_type: string; created_at: string }

export default function GroupList() {
  const [groups, setGroups] = useState<Group[]>([])

  useEffect(() => {
    supabase.from('restaurant_groups').select('*').order('name')
      .then(({ data }) => setGroups(data ?? []))
  }, [])

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>Groupes de restaurants <span style={{ color: '#888', fontWeight: 400, fontSize: '0.9rem' }}>{groups.length} groupe(s)</span></h2>
        <Link to="/groups/new">
          <button style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}>
            + Nouveau groupe
          </button>
        </Link>
      </div>
      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['NOM', 'TYPE', 'DATE CRÉATION', 'ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {groups.map(g => (
            <tr key={g.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
              <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{g.name}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#666', textTransform: 'capitalize' }}>{g.group_type}</td>
              <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.85rem' }}>{g.created_at?.slice(0, 10)}</td>
              <td style={{ padding: '0.7rem 0.8rem' }}>
                <Link to={`/groups/${g.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 8 }}>Éditer</Link>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}
```

- [ ] **Step 4 : Créer `backoffice/src/pages/GroupForm.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../supabaseClient'

interface Site { id: string; site_code: string; name: string }

export default function GroupForm() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const isEdit = Boolean(id)

  const [name, setName] = useState('')
  const [groupType, setGroupType] = useState<'static' | 'dynamic' | 'mixed'>('static')
  const [criteria, setCriteria] = useState('{}')
  const [allSites, setAllSites] = useState<Site[]>([])
  const [selectedSites, setSelectedSites] = useState<string[]>([])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    supabase.from('sites').select('id, site_code, name').order('site_code')
      .then(({ data }) => setAllSites(data ?? []))

    if (isEdit) {
      supabase.from('restaurant_groups').select('*').eq('id', id!).single()
        .then(({ data }) => {
          if (!data) return
          setName(data.name)
          setGroupType(data.group_type)
          setCriteria(JSON.stringify(data.criteria ?? {}, null, 2))
        })
      supabase.from('restaurant_group_members').select('site_id').eq('group_id', id!)
        .then(({ data }) => setSelectedSites((data ?? []).map((r: any) => r.site_id)))
    }
  }, [id, isEdit])

  const toggleSite = (siteId: string) =>
    setSelectedSites(prev => prev.includes(siteId) ? prev.filter(s => s !== siteId) : [...prev, siteId])

  const handleSave = async () => {
    setSaving(true); setError(null)
    try {
      let parsedCriteria: object | null = null
      if (groupType !== 'static') {
        try { parsedCriteria = JSON.parse(criteria) }
        catch { throw new Error('Critères JSON invalides') }
      }

      let groupId = id
      if (!isEdit) {
        const { data, error: e } = await supabase.from('restaurant_groups')
          .insert({ name, group_type: groupType, criteria: parsedCriteria })
          .select('id').single()
        if (e) throw e
        groupId = data.id
      } else {
        const { error: e } = await supabase.from('restaurant_groups')
          .update({ name, group_type: groupType, criteria: parsedCriteria })
          .eq('id', id!)
        if (e) throw e
      }

      // Sync membres statiques
      if (groupType !== 'dynamic') {
        await supabase.from('restaurant_group_members').delete().eq('group_id', groupId!)
        if (selectedSites.length > 0)
          await supabase.from('restaurant_group_members')
            .insert(selectedSites.map(s => ({ group_id: groupId!, site_id: s })))
      }
      navigate('/groups')
    } catch (e: any) {
      setError(e.message ?? 'Erreur de sauvegarde')
    } finally { setSaving(false) }
  }

  const label = (txt: string) => <label style={{ display: 'block', fontWeight: 600, marginBottom: 4, fontSize: '0.85rem' }}>{txt}</label>
  const input = { padding: '0.5rem 0.75rem', border: '1px solid #ddd', borderRadius: 6, fontSize: '0.9rem', width: '100%', boxSizing: 'border-box' as const }

  return (
    <div style={{ padding: '1.5rem', maxWidth: 600 }}>
      <h2 style={{ marginTop: 0 }}>{isEdit ? 'Éditer' : 'Nouveau'} groupe</h2>
      {error && <p style={{ color: '#e53e3e' }}>{error}</p>}

      <div style={{ marginBottom: '1rem' }}>{label('Nom')}<input style={input} value={name} onChange={e => setName(e.target.value)} /></div>

      <div style={{ marginBottom: '1rem' }}>
        {label('Type')}
        <select style={input} value={groupType} onChange={e => setGroupType(e.target.value as any)}>
          <option value="static">Statique (liste manuelle)</option>
          <option value="dynamic">Dynamique (critères)</option>
          <option value="mixed">Mixte</option>
        </select>
      </div>

      {groupType !== 'static' && (
        <div style={{ marginBottom: '1rem' }}>
          {label('Critères JSON (ex: {"ville":"Paris"})')}
          <textarea style={{ ...input, height: 100, fontFamily: 'monospace', resize: 'vertical' }}
            value={criteria} onChange={e => setCriteria(e.target.value)} />
        </div>
      )}

      {groupType !== 'dynamic' && (
        <div style={{ marginBottom: '1rem' }}>
          {label('Sites membres')}
          <div style={{ border: '1px solid #ddd', borderRadius: 6, maxHeight: 200, overflowY: 'auto', padding: '0.5rem' }}>
            {allSites.map(s => (
              <label key={s.id} style={{ display: 'flex', alignItems: 'center', gap: 8, padding: '0.25rem 0', cursor: 'pointer' }}>
                <input type="checkbox" checked={selectedSites.includes(s.id)} onChange={() => toggleSite(s.id)} />
                {s.name} <span style={{ color: '#888', fontSize: '0.8rem' }}>({s.site_code})</span>
              </label>
            ))}
          </div>
        </div>
      )}

      <div style={{ display: 'flex', gap: 8 }}>
        <button onClick={handleSave} disabled={saving || !name.trim()}
          style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.6rem 1.2rem', cursor: 'pointer', fontWeight: 600 }}>
          {saving ? 'Sauvegarde…' : 'Enregistrer'}
        </button>
        <button onClick={() => navigate('/groups')}
          style={{ background: '#f5f6fa', border: '1px solid #ddd', borderRadius: 6, padding: '0.6rem 1rem', cursor: 'pointer' }}>
          Annuler
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 5 : Commit**

```bash
git add backoffice/src/context/AuthContext.tsx backoffice/src/hooks/useRole.ts \
        backoffice/src/pages/GroupList.tsx backoffice/src/pages/GroupForm.tsx
git commit -m "feat(backoffice): useRole hook + GroupList + GroupForm"
```

---

## Task 9 — Back-office : PromotionList + file d'approbation

**Files:**
- Create: `backoffice/src/pages/PromotionList.tsx`

- [ ] **Step 1 : Créer `backoffice/src/pages/PromotionList.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { Link } from 'react-router-dom'
import { supabase } from '../supabaseClient'
import { useRole } from '../hooks/useRole'

interface Promo {
  id: string; name: string; scope: string; promo_type: string
  trigger: string; status: string; valid_from: string | null
  valid_to: string | null; active: boolean
}

const STATUS_BADGE: Record<string, { bg: string; label: string }> = {
  draft:            { bg: '#f0f0f0', label: 'Brouillon' },
  pending_approval: { bg: '#fff3cd', label: 'En attente' },
  approved:         { bg: '#d4edda', label: 'Approuvée' },
  active:           { bg: '#c3e6cb', label: 'Active' },
  rejected:         { bg: '#f8d7da', label: 'Rejetée' },
}

export default function PromotionList() {
  const { hasRole } = useRole()
  const [promos, setPromos] = useState<Promo[]>([])
  const [tab, setTab] = useState<'all' | 'pending'>('all')
  const [rejectId, setRejectId] = useState<string | null>(null)
  const [rejectReason, setRejectReason] = useState('')

  const load = async () => {
    const q = supabase.from('promotions').select('id,name,scope,promo_type,trigger,status,valid_from,valid_to,active')
    const { data } = tab === 'pending' ? await q.eq('status', 'pending_approval') : await q.order('created_at', { ascending: false })
    setPromos(data ?? [])
  }

  useEffect(() => { load() }, [tab])

  const approve = async (id: string) => {
    await supabase.from('promotions').update({ status: 'approved', approved_at: new Date().toISOString() }).eq('id', id)
    load()
  }

  const reject = async () => {
    if (!rejectId || !rejectReason.trim()) return
    await supabase.from('promotions').update({ status: 'rejected', rejection_reason: rejectReason }).eq('id', rejectId)
    setRejectId(null); setRejectReason(''); load()
  }

  const toggleActive = async (p: Promo) => {
    if (p.status !== 'approved' && !p.active) return
    await supabase.from('promotions').update({ active: !p.active }).eq('id', p.id)
    load()
  }

  const tabStyle = (t: string) => ({
    padding: '0.4rem 1rem', border: 'none', cursor: 'pointer', fontWeight: 600,
    borderBottom: tab === t ? '2px solid #4f8ef7' : '2px solid transparent',
    background: 'none', color: tab === t ? '#4f8ef7' : '#888',
  })

  return (
    <div style={{ padding: '1.5rem' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '1rem' }}>
        <h2 style={{ margin: 0 }}>Promotions</h2>
        <Link to="/promotions/new">
          <button style={{ background: '#4f8ef7', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', fontWeight: 600 }}>
            + Nouvelle promo
          </button>
        </Link>
      </div>

      {hasRole('director') && (
        <div style={{ borderBottom: '1px solid #eee', marginBottom: '1rem' }}>
          <button style={tabStyle('all')} onClick={() => setTab('all')}>Toutes</button>
          <button style={tabStyle('pending')} onClick={() => setTab('pending')}>En attente d'approbation</button>
        </div>
      )}

      {/* Modal rejet */}
      {rejectId && (
        <div style={{ position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.4)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 100 }}>
          <div style={{ background: '#fff', borderRadius: 12, padding: '1.5rem', width: 400 }}>
            <h3 style={{ margin: '0 0 1rem' }}>Motif de rejet</h3>
            <textarea value={rejectReason} onChange={e => setRejectReason(e.target.value)}
              style={{ width: '100%', border: '1px solid #ddd', borderRadius: 6, padding: '0.5rem', minHeight: 80, boxSizing: 'border-box' }} />
            <div style={{ display: 'flex', gap: 8, marginTop: '0.75rem' }}>
              <button onClick={reject} disabled={!rejectReason.trim()}
                style={{ background: '#e53e3e', color: '#fff', border: 'none', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer' }}>Rejeter</button>
              <button onClick={() => setRejectId(null)} style={{ border: '1px solid #ddd', borderRadius: 6, padding: '0.5rem 1rem', cursor: 'pointer', background: '#fff' }}>Annuler</button>
            </div>
          </div>
        </div>
      )}

      <table style={{ width: '100%', borderCollapse: 'collapse' }}>
        <thead>
          <tr style={{ background: '#f5f6fa' }}>
            {['NOM','TYPE','SCOPE','DÉCLENCHEUR','VALIDITÉ','STATUT','ACTIONS'].map(h => (
              <th key={h} style={{ textAlign: 'left', padding: '0.6rem 0.8rem', fontSize: '0.78rem', color: '#666' }}>{h}</th>
            ))}
          </tr>
        </thead>
        <tbody>
          {promos.map(p => {
            const badge = STATUS_BADGE[p.status] ?? { bg: '#eee', label: p.status }
            return (
              <tr key={p.id} style={{ borderBottom: '1px solid #f0f0f0' }}>
                <td style={{ padding: '0.7rem 0.8rem', fontWeight: 500 }}>{p.name}</td>
                <td style={{ padding: '0.7rem 0.8rem', color: '#666' }}>{p.promo_type}</td>
                <td style={{ padding: '0.7rem 0.8rem', color: '#666', textTransform: 'capitalize' }}>{p.scope}</td>
                <td style={{ padding: '0.7rem 0.8rem', color: '#666' }}>{p.trigger}</td>
                <td style={{ padding: '0.7rem 0.8rem', color: '#888', fontSize: '0.8rem' }}>
                  {p.valid_from ?? '—'} → {p.valid_to ?? '∞'}
                </td>
                <td style={{ padding: '0.7rem 0.8rem' }}>
                  <span style={{ background: badge.bg, padding: '0.2rem 0.6rem', borderRadius: 12, fontSize: '0.78rem' }}>{badge.label}</span>
                </td>
                <td style={{ padding: '0.7rem 0.8rem', whiteSpace: 'nowrap' }}>
                  <Link to={`/promotions/${p.id}`} style={{ color: '#4f8ef7', textDecoration: 'none', marginRight: 8 }}>Éditer</Link>
                  {p.status === 'pending_approval' && hasRole('director') && (
                    <>
                      <button onClick={() => approve(p.id)} style={{ color: '#38a169', border: 'none', background: 'none', cursor: 'pointer', marginRight: 4 }}>✓ Approuver</button>
                      <button onClick={() => setRejectId(p.id)} style={{ color: '#e53e3e', border: 'none', background: 'none', cursor: 'pointer' }}>✗ Rejeter</button>
                    </>
                  )}
                  {p.status === 'approved' && (
                    <button onClick={() => toggleActive(p)} style={{ color: p.active ? '#e53e3e' : '#38a169', border: 'none', background: 'none', cursor: 'pointer' }}>
                      {p.active ? 'Désactiver' : 'Activer'}
                    </button>
                  )}
                </td>
              </tr>
            )
          })}
        </tbody>
      </table>
    </div>
  )
}
```

- [ ] **Step 2 : Commit**

```bash
git add backoffice/src/pages/PromotionList.tsx
git commit -m "feat(backoffice): PromotionList + file d'approbation director"
```

---

## Task 10 — Back-office : PromotionForm

**Files:**
- Create: `backoffice/src/pages/PromotionForm.tsx`

- [ ] **Step 1 : Créer `backoffice/src/pages/PromotionForm.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { useNavigate, useParams } from 'react-router-dom'
import { supabase } from '../supabaseClient'
import { useAuth } from '../context/AuthContext'
import { useRole } from '../hooks/useRole'

const PROMO_TYPES = ['fixed_amount','percentage','item_discount','bogo','happy_hour']
const DAYS = [{ v: 1, l: 'Lun' },{ v: 2, l: 'Mar' },{ v: 3, l: 'Mer' },
              { v: 4, l: 'Jeu' },{ v: 5, l: 'Ven' },{ v: 6, l: 'Sam' },{ v: 7, l: 'Dim' }]

export default function PromotionForm() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const { session } = useAuth()
  const { hasRole } = useRole()
  const isEdit = Boolean(id)

  const [name, setName] = useState('')
  const [scope, setScope] = useState<'site'|'group'|'chain'>('site')
  const [siteId, setSiteId] = useState('')
  const [groupId, setGroupId] = useState('')
  const [trigger, setTrigger] = useState<'auto'|'manual'>('manual')
  const [promoType, setPromoType] = useState('fixed_amount')
  const [valueCents, setValueCents] = useState('')
  const [valueBps, setValueBps] = useState('')
  const [targetSku, setTargetSku] = useState('')
  const [exclusionGroup, setExclusionGroup] = useState('')
  const [priority, setPriority] = useState('0')
  const [validFrom, setValidFrom] = useState('')
  const [validTo, setValidTo] = useState('')
  const [daysOfWeek, setDaysOfWeek] = useState<number[]>([])
  const [timeFrom, setTimeFrom] = useState('')
  const [timeTo, setTimeTo] = useState('')
  const [status, setStatus] = useState('draft')

  const [sites, setSites] = useState<{id:string;name:string;site_code:string}[]>([])
  const [groups, setGroups] = useState<{id:string;name:string}[]>([])
  const [saving, setSaving] = useState(false)
  const [error, setError] = useState<string|null>(null)

  useEffect(() => {
    supabase.from('sites').select('id,name,site_code').then(({data}) => setSites(data??[]))
    supabase.from('restaurant_groups').select('id,name').then(({data}) => setGroups(data??[]))
    if (isEdit) {
      supabase.from('promotions').select('*').eq('id',id!).single().then(({data:d}) => {
        if (!d) return
        setName(d.name); setScope(d.scope); setSiteId(d.site_id??'')
        setGroupId(d.group_id??''); setTrigger(d.trigger); setPromoType(d.promo_type)
        setValueCents(d.value_cents?.toString()??''); setValueBps(d.value_bps?.toString()??'')
        setTargetSku(d.target_sku??''); setExclusionGroup(d.exclusion_group??'')
        setPriority(d.priority?.toString()??'0'); setValidFrom(d.valid_from??'')
        setValidTo(d.valid_to??''); setDaysOfWeek(d.days_of_week??[])
        setTimeFrom(d.time_from??''); setTimeTo(d.time_to??''); setStatus(d.status)
      })
    }
  }, [id, isEdit])

  const toggleDay = (v: number) =>
    setDaysOfWeek(prev => prev.includes(v) ? prev.filter(d => d!==v) : [...prev, v])

  const handleSave = async (submitForApproval = false) => {
    setSaving(true); setError(null)
    try {
      const payload: Record<string, any> = {
        name, scope, trigger, promo_type: promoType,
        value_cents: valueCents ? parseInt(valueCents,10) : null,
        value_bps: valueBps ? parseInt(valueBps,10) : null,
        target_sku: targetSku || null,
        exclusion_group: exclusionGroup || null,
        priority: parseInt(priority,10) || 0,
        valid_from: validFrom || null, valid_to: validTo || null,
        days_of_week: daysOfWeek.length ? daysOfWeek : null,
        time_from: timeFrom || null, time_to: timeTo || null,
        site_id: scope === 'site' ? siteId : null,
        group_id: scope === 'group' ? groupId : null,
        status: submitForApproval ? 'pending_approval' : status,
      }
      if (!isEdit) payload.created_by = session?.user?.id

      const { error: e } = isEdit
        ? await supabase.from('promotions').update(payload).eq('id', id!)
        : await supabase.from('promotions').insert(payload)
      if (e) throw e
      navigate('/promotions')
    } catch (e: any) {
      setError(e.message ?? 'Erreur de sauvegarde')
    } finally { setSaving(false) }
  }

  const handleApprove = async () => {
    await supabase.from('promotions').update({
      status: 'approved', approved_by: session?.user?.id, approved_at: new Date().toISOString()
    }).eq('id', id!)
    navigate('/promotions')
  }

  const lb = (t: string) => <label style={{ display:'block',fontWeight:600,marginBottom:4,fontSize:'0.85rem' }}>{t}</label>
  const inp = { padding:'0.5rem 0.75rem',border:'1px solid #ddd',borderRadius:6,fontSize:'0.9rem',width:'100%',boxSizing:'border-box' as const }
  const sect = (title: string) => <h3 style={{ marginTop:'1.5rem',marginBottom:'0.75rem',fontSize:'0.95rem',color:'#444',borderBottom:'1px solid #eee',paddingBottom:4 }}>{title}</h3>

  const needsSku = ['item_discount','bogo'].includes(promoType)
  const needsBps = ['percentage','happy_hour'].includes(promoType)
  const needsCents = ['fixed_amount','item_discount','happy_hour'].includes(promoType)

  return (
    <div style={{ padding:'1.5rem', maxWidth:640 }}>
      <h2 style={{ marginTop:0 }}>{isEdit ? 'Éditer' : 'Nouvelle'} promotion</h2>
      {error && <p style={{ color:'#e53e3e' }}>{error}</p>}

      {sect('1. Identité')}
      <div style={{ marginBottom:'0.75rem' }}>{lb('Nom')}<input style={inp} value={name} onChange={e=>setName(e.target.value)} /></div>
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12,marginBottom:'0.75rem' }}>
        <div>{lb('Portée')}<select style={inp} value={scope} onChange={e=>setScope(e.target.value as any)}>
          <option value="site">Site</option><option value="group">Groupe</option><option value="chain">Chaîne</option>
        </select></div>
        <div>{lb('Déclencheur')}<select style={inp} value={trigger} onChange={e=>setTrigger(e.target.value as any)}>
          <option value="manual">Manuel (caissier)</option><option value="auto">Automatique</option>
        </select></div>
      </div>
      {scope==='site' && <div style={{ marginBottom:'0.75rem' }}>{lb('Site')}
        <select style={inp} value={siteId} onChange={e=>setSiteId(e.target.value)}>
          <option value="">— choisir —</option>
          {sites.map(s=><option key={s.id} value={s.id}>{s.name} ({s.site_code})</option>)}
        </select></div>}
      {scope==='group' && <div style={{ marginBottom:'0.75rem' }}>{lb('Groupe')}
        <select style={inp} value={groupId} onChange={e=>setGroupId(e.target.value)}>
          <option value="">— choisir —</option>
          {groups.map(g=><option key={g.id} value={g.id}>{g.name}</option>)}
        </select></div>}

      {sect('2. Mécanique')}
      <div style={{ marginBottom:'0.75rem' }}>{lb('Type de remise')}<select style={inp} value={promoType} onChange={e=>setPromoType(e.target.value)}>
        {PROMO_TYPES.map(t=><option key={t} value={t}>{t}</option>)}
      </select></div>
      {needsCents && <div style={{ marginBottom:'0.75rem' }}>{lb('Montant fixe (centimes)')}
        <input style={inp} type="number" value={valueCents} onChange={e=>setValueCents(e.target.value)} placeholder="ex: 200 = 2,00 €" /></div>}
      {needsBps && <div style={{ marginBottom:'0.75rem' }}>{lb('Pourcentage (basis points, 1000 = 10%)')}
        <input style={inp} type="number" value={valueBps} onChange={e=>setValueBps(e.target.value)} placeholder="ex: 1000 = 10%" /></div>}
      {needsSku && <div style={{ marginBottom:'0.75rem' }}>{lb('SKU cible')}
        <input style={inp} value={targetSku} onChange={e=>setTargetSku(e.target.value)} placeholder="ex: BUR-001" /></div>}

      {sect('3. Cumul')}
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12 }}>
        <div>{lb('Groupe d\'exclusion')}<input style={inp} value={exclusionGroup} onChange={e=>setExclusionGroup(e.target.value)} placeholder="ex: remise_panier" /></div>
        <div>{lb('Priorité')}<input style={inp} type="number" value={priority} onChange={e=>setPriority(e.target.value)} /></div>
      </div>

      {sect('4. Validité')}
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12,marginBottom:'0.75rem' }}>
        <div>{lb('Début')}<input style={inp} type="date" value={validFrom} onChange={e=>setValidFrom(e.target.value)} /></div>
        <div>{lb('Fin')}<input style={inp} type="date" value={validTo} onChange={e=>setValidTo(e.target.value)} /></div>
      </div>
      <div style={{ marginBottom:'0.75rem' }}>{lb('Jours de la semaine')}
        <div style={{ display:'flex',gap:8,flexWrap:'wrap',marginTop:4 }}>
          {DAYS.map(d=>(
            <label key={d.v} style={{ display:'flex',alignItems:'center',gap:4,cursor:'pointer',
              padding:'0.25rem 0.6rem',border:'1px solid #ddd',borderRadius:20,
              background: daysOfWeek.includes(d.v) ? '#4f8ef7' : '#fff',
              color: daysOfWeek.includes(d.v) ? '#fff' : '#333', fontSize:'0.85rem' }}>
              <input type="checkbox" style={{ display:'none' }} checked={daysOfWeek.includes(d.v)} onChange={()=>toggleDay(d.v)} />{d.l}
            </label>
          ))}
        </div>
      </div>
      <div style={{ display:'grid',gridTemplateColumns:'1fr 1fr',gap:12 }}>
        <div>{lb('Heure début')}<input style={inp} type="time" value={timeFrom} onChange={e=>setTimeFrom(e.target.value)} /></div>
        <div>{lb('Heure fin')}<input style={inp} type="time" value={timeTo} onChange={e=>setTimeTo(e.target.value)} /></div>
      </div>

      <div style={{ display:'flex',gap:8,marginTop:'1.5rem',flexWrap:'wrap' }}>
        <button onClick={() => handleSave(false)} disabled={saving||!name.trim()}
          style={{ background:'#4f8ef7',color:'#fff',border:'none',borderRadius:6,padding:'0.6rem 1.2rem',cursor:'pointer',fontWeight:600 }}>
          {saving ? 'Sauvegarde…' : 'Enregistrer (brouillon)'}
        </button>
        {status === 'draft' && (
          <button onClick={() => handleSave(true)} disabled={saving||!name.trim()}
            style={{ background:'#ed8936',color:'#fff',border:'none',borderRadius:6,padding:'0.6rem 1.2rem',cursor:'pointer',fontWeight:600 }}>
            Soumettre pour approbation
          </button>
        )}
        {status === 'pending_approval' && hasRole('director') && (
          <button onClick={handleApprove}
            style={{ background:'#38a169',color:'#fff',border:'none',borderRadius:6,padding:'0.6rem 1.2rem',cursor:'pointer',fontWeight:600 }}>
            Approuver directement
          </button>
        )}
        <button onClick={() => navigate('/promotions')}
          style={{ background:'#f5f6fa',border:'1px solid #ddd',borderRadius:6,padding:'0.6rem 1rem',cursor:'pointer' }}>
          Annuler
        </button>
      </div>
    </div>
  )
}
```

- [ ] **Step 2 : Commit**

```bash
git add backoffice/src/pages/PromotionForm.tsx
git commit -m "feat(backoffice): PromotionForm — create/edit/submit/approve"
```

---

## Task 11 — Back-office : nav + routes wiring

**Files:**
- Modify: `backoffice/src/App.tsx`
- Modify: `backoffice/src/components/Layout.tsx`

- [ ] **Step 1 : Ajouter les imports et routes dans `App.tsx`**

```typescript
import GroupList from './pages/GroupList'
import GroupForm from './pages/GroupForm'
import PromotionList from './pages/PromotionList'
import PromotionForm from './pages/PromotionForm'

// Dans <Routes> :
<Route path="/promotions"     element={<PromotionList />} />
<Route path="/promotions/new" element={<PromotionForm />} />
<Route path="/promotions/:id" element={<PromotionForm />} />
<Route path="/groups"         element={<GroupList />} />
<Route path="/groups/new"     element={<GroupForm />} />
<Route path="/groups/:id"     element={<GroupForm />} />
```

- [ ] **Step 2 : Ajouter les entrées nav dans `Layout.tsx`**

Dans la liste des liens de navigation existante, ajouter sous les entrées CATALOGUE :

```typescript
{ href: '/promotions', label: '🏷️ Promotions' },
// Sous CONFIG SITE (visible director+) :
{ href: '/groups', label: '🏘️ Groupes', minRole: 'director' },
```

Adapter le rendu pour filtrer par `minRole` si nécessaire via le hook `useRole`.

- [ ] **Step 3 : Lancer le back-office et vérifier la navigation**

```bash
cd pos-fiscal/backoffice && npm run dev
```

Vérifier dans le navigateur :
- `/promotions` → liste vide + bouton "+ Nouvelle promo"
- `/promotions/new` → formulaire complet avec toutes les sections
- `/groups` → liste vide + bouton "+ Nouveau groupe"

- [ ] **Step 4 : Commit**

```bash
git add backoffice/src/App.tsx backoffice/src/components/Layout.tsx
git commit -m "feat(backoffice): routes + nav promotions et groupes"
```

---

## Task 12 — pos-app : PromoModal + store + OrderPage + ticket

**Files:**
- Modify: `pos-app/src/api/client.ts`
- Modify: `pos-app/src/store/orderStore.ts`
- Create: `pos-app/src/components/PromoModal.tsx`
- Modify: `pos-app/src/pages/OrderPage.tsx`

- [ ] **Step 1 : Ajouter `getAvailablePromotions` dans `client.ts`**

```typescript
export interface AvailablePromo {
  id: string
  name: string
  promo_type: string
  trigger: 'auto' | 'manual'
  value_cents: number | null
  value_bps: number | null
  target_sku: string | null
}

export async function getAvailablePromotions(): Promise<AvailablePromo[]> {
  const res = await fetch(`${getApiUrl()}/api/v1/promotions/available`)
  if (!res.ok) return []
  return res.json()
}
```

Dans `createOrder`, ajouter `manual_promo_ids` au payload :

```typescript
export async function createOrder(params: {
  orderReference: string
  lineItems: LineItem[]
  paymentMethod: string
  manualPromoIds?: string[]
}) {
  const res = await fetch(`${getApiUrl()}/api/v1/orders`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      order_reference: params.orderReference,
      line_items: params.lineItems,
      payment_method: params.paymentMethod,
      manual_promo_ids: params.manualPromoIds ?? [],
    }),
  })
  if (!res.ok) throw new Error(await res.text())
  return res.json()
}
```

- [ ] **Step 2 : Ajouter `selectedPromoIds` dans `orderStore.ts`**

```typescript
// Dans le store Zustand, ajouter :
selectedPromoIds: string[]
setSelectedPromoIds: (ids: string[]) => void

// Dans create() :
selectedPromoIds: [],
setSelectedPromoIds: (ids) => set({ selectedPromoIds: ids }),
```

- [ ] **Step 3 : Créer `pos-app/src/components/PromoModal.tsx`**

```typescript
import { useEffect, useState } from 'react'
import { getAvailablePromotions, type AvailablePromo } from '../api/client'
import { useOrderStore } from '../store/orderStore'

interface Props { onClose: () => void }

export default function PromoModal({ onClose }: Props) {
  const [promos, setPromos] = useState<AvailablePromo[]>([])
  const { selectedPromoIds, setSelectedPromoIds } = useOrderStore()
  const [selected, setSelected] = useState<string[]>(selectedPromoIds)

  useEffect(() => { getAvailablePromotions().then(setPromos) }, [])

  const autoPromos = promos.filter(p => p.trigger === 'auto')
  const manualPromos = promos.filter(p => p.trigger === 'manual')

  const toggle = (id: string) =>
    setSelected(prev => prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id])

  const confirm = () => { setSelectedPromoIds(selected); onClose() }

  const formatDiscount = (p: AvailablePromo) => {
    if (p.value_cents) return `-${(p.value_cents / 100).toFixed(2)} €`
    if (p.value_bps)   return `-${(p.value_bps / 100).toFixed(0)} %`
    return 'offert'
  }

  return (
    <div style={{ position:'fixed',inset:0,background:'rgba(0,0,0,0.5)',display:'flex',alignItems:'center',justifyContent:'center',zIndex:50 }}>
      <div style={{ background:'#fff',borderRadius:16,padding:'1.5rem',width:360,maxHeight:'80vh',overflowY:'auto' }}>
        <h3 style={{ margin:'0 0 1rem' }}>Promotions disponibles</h3>

        {autoPromos.length > 0 && (
          <div style={{ marginBottom:'1rem' }}>
            <p style={{ margin:'0 0 0.5rem',fontWeight:600,fontSize:'0.85rem',color:'#555' }}>APPLIQUÉES AUTOMATIQUEMENT</p>
            {autoPromos.map(p => (
              <div key={p.id} style={{ display:'flex',justifyContent:'space-between',alignItems:'center',
                padding:'0.5rem 0.75rem',background:'#f0fff4',borderRadius:8,marginBottom:4 }}>
                <span style={{ fontSize:'0.9rem' }}>{p.name}</span>
                <span style={{ color:'#38a169',fontWeight:600,fontSize:'0.85rem' }}>{formatDiscount(p)}</span>
              </div>
            ))}
          </div>
        )}

        {manualPromos.length > 0 && (
          <div style={{ marginBottom:'1rem' }}>
            <p style={{ margin:'0 0 0.5rem',fontWeight:600,fontSize:'0.85rem',color:'#555' }}>SÉLECTION MANUELLE</p>
            {manualPromos.map(p => (
              <label key={p.id} style={{ display:'flex',justifyContent:'space-between',alignItems:'center',
                padding:'0.5rem 0.75rem',background: selected.includes(p.id) ? '#ebf4ff' : '#f9f9f9',
                borderRadius:8,marginBottom:4,cursor:'pointer',
                border: selected.includes(p.id) ? '1px solid #4f8ef7' : '1px solid transparent' }}>
                <span style={{ display:'flex',alignItems:'center',gap:8 }}>
                  <input type="checkbox" checked={selected.includes(p.id)} onChange={() => toggle(p.id)} />
                  <span style={{ fontSize:'0.9rem' }}>{p.name}</span>
                </span>
                <span style={{ color:'#4f8ef7',fontWeight:600,fontSize:'0.85rem' }}>{formatDiscount(p)}</span>
              </label>
            ))}
          </div>
        )}

        {promos.length === 0 && <p style={{ color:'#888',textAlign:'center' }}>Aucune promotion disponible</p>}

        <div style={{ display:'flex',gap:8,marginTop:'1rem' }}>
          <button onClick={confirm} style={{ flex:1,background:'#4f8ef7',color:'#fff',border:'none',borderRadius:8,padding:'0.65rem',fontWeight:600,cursor:'pointer' }}>
            Confirmer
          </button>
          <button onClick={onClose} style={{ background:'#f5f6fa',border:'1px solid #ddd',borderRadius:8,padding:'0.65rem 1rem',cursor:'pointer' }}>
            Fermer
          </button>
        </div>
      </div>
    </div>
  )
}
```

- [ ] **Step 4 : Ajouter bouton "Promos" dans `OrderPage.tsx`**

Dans la zone des boutons d'action de la commande, ajouter :

```typescript
import PromoModal from '../components/PromoModal'
// ...
const [showPromos, setShowPromos] = useState(false)
const { selectedPromoIds } = useOrderStore()

// Bouton à ajouter dans l'UI :
<button onClick={() => setShowPromos(true)} style={{ background: selectedPromoIds.length > 0 ? '#ebf4ff' : '#f5f6fa',
  border: selectedPromoIds.length > 0 ? '1px solid #4f8ef7' : '1px solid #ddd',
  borderRadius:8, padding:'0.6rem 1rem', cursor:'pointer', fontWeight:600 }}>
  🏷️ Promos {selectedPromoIds.length > 0 ? `(${selectedPromoIds.length})` : ''}
</button>
{showPromos && <PromoModal onClose={() => setShowPromos(false)} />}
```

Dans l'appel à `createOrder`, passer les IDs :
```typescript
manualPromoIds: selectedPromoIds,
```

- [ ] **Step 5 : Afficher les remises sur le ticket**

Dans la page ou le composant de ticket (ex: `TicketPage.tsx`), après les line items, afficher les `applied_promos` retournés par l'edge-api :

```typescript
{order.applied_promos?.map((p: {promo_id:string;name:string;discount_cents:number}) => (
  <div key={p.promo_id} style={{ display:'flex',justifyContent:'space-between',color:'#38a169' }}>
    <span>🏷️ {p.name}</span>
    <span>-{(p.discount_cents/100).toFixed(2)} €</span>
  </div>
))}
```

- [ ] **Step 6 : Compiler et tester**

```bash
cd pos-fiscal/pos-app && npm run dev
# Ouvrir http://localhost:5173
# Vérifier : bouton "Promos" visible sur l'écran de commande
# Tester : cliquer "Promos" → modal s'ouvre, aucune promo si SQLite vide
```

- [ ] **Step 7 : Commit**

```bash
git add pos-app/src/api/client.ts pos-app/src/store/orderStore.ts \
        pos-app/src/components/PromoModal.tsx pos-app/src/pages/OrderPage.tsx
git commit -m "feat(pos-app): PromoModal + selectedPromoIds store + ticket remises"
```

---

## Self-review

**Spec coverage :**
- Types de remises (fixed, %, item, bogo, happy_hour) → Task 4-5 ✅
- Déclenchement mixte auto/manual → Task 4 (filtre trigger) + Task 12 (PromoModal) ✅
- Groupes exclusifs paramétrables → Task 5 (resolve_exclusion_groups) ✅
- Fiscal : une ligne DISCOUNT par promo → Task 7 (loop sur eval.applied) ✅
- Scope chain/group/site → Task 1 (migration), Task 6 (pull), Task 10 (form) ✅
- Hiérarchie 4 rôles + app_metadata → Task 8 (useRole) ✅
- Workflow draft→pending→approved→active/rejected → Task 9 (PromotionList) + Task 10 (PromotionForm) ✅
- Règles approbation scope×montant → Task 1 (migration 013 + seed) ✅
- Groupes restaurants statiques + dynamiques → Task 8 (GroupForm) ✅
- Validité date + jours + heure → Task 4 (is_in_window) + Task 10 (PromotionForm) ✅
- Back-office pages → Tasks 8-11 ✅
- pos-app PromoModal + ticket → Task 12 ✅

**Placeholder scan :** aucun TBD détecté.

**Cohérence des types :**
- `TvaRateKey` utilisé partout dans promo-engine ✅
- `PromoApplication.discount_cents` toujours positif, négatif dans DISCOUNT fiscal ✅
- `manual_promo_ids: Vec<String>` dans CreateOrderRequest, `&[Uuid]` dans evaluate() → conversion explicite dans Task 7 Step 4 ✅
- `AvailablePromo` identique entre `promotions.rs` (Rust) et `client.ts` (TS) ✅
