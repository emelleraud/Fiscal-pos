# CLAUDE.md — pos-fiscal

POS fiscal open-source conforme **NF525** pour une chaîne QSR en France.  
Architecture offline-first : terminal Electron → edge-api Axum → sync-client → Supabase (PostgreSQL cloud).

---

## Architecture

```
pos-app (Electron + React)
    │  HTTP LAN
    ▼
edge-api (Axum, port 8080)
    │  SQLite WAL
    ▼
fiscal-engine (Rust — NF525 : hash-chain, Z-reports)
    │
    ▼
sync-client (agent Rust — sync offline-first)
    │  HTTPS Supabase REST
    ▼
Supabase (PostgreSQL cloud)
    │
    ▼
backoffice (React Vite, port 5173 — dashboard / journal / rapports Z)
```

### Crates Rust

| Crate | Rôle |
|---|---|
| `common` | Types partagés, erreurs, constantes |
| `fiscal-engine` | Moteur NF525 : journal SQLite append-only, hash SHA-256 chaîné, signature ed25519, rapports Z |
| `edge-api` | Serveur HTTP Axum (LAN only) — routes sessions, orders, menu, health |
| `sync-client` | Agent de sync SQLite → Supabase. Ordre obligatoire : sessions → z_reports → fiscal_entries (FK cloud) |

### Frontends

| App | Tech | Port |
|---|---|---|
| `pos-app` | Electron + React 18 + Vite 5 + Zustand + TailwindCSS | 5173 |
| `backoffice` | React 19 + React Router 7 + Supabase JS + Vite | 5173 |

---

## Commandes essentielles

```bash
# Tests unitaires (tout le workspace)
cargo test --workspace

# Tests E2E sync (nécessite .env.test chargé)
source .env.test && cargo test -p sync-client --test e2e_sync -- --ignored --nocapture

# edge-api local
DATABASE_URL=sqlite:./restaurant.db EDGE_API_PORT=8080 cargo run -p edge-api

# sync-client
source .env && cargo run -p sync-client

# Back-office
cd backoffice && npm run dev

# pos-app (dev)
cd pos-app && npm run dev

# pos-app (Electron dev)
cd pos-app && npm run electron:dev

# Scripts prêts à l'emploi
./scripts/01_build.sh
./scripts/02_test.sh
./scripts/03_run_edge_api.sh
./scripts/04_run_frontend.sh
./scripts/05_run_sync_client.sh
./scripts/06_smoke_test.sh
```

---

## Variables d'environnement

### `.env` (racine — sync-client en prod)
```
DATABASE_URL=sqlite:./restaurant.db
SUPABASE_URL=https://iawyngsvqjsogvkwkrxw.supabase.co
SUPABASE_SERVICE_KEY=<jamais committée>
SITE_ID=9983f3ac-cde8-4838-9386-49ef24f57dad
```

### `.env.test` (racine — tests E2E, ignoré par git)
Mêmes clés que `.env`. **Ne jamais committer.**

### `backoffice/.env.local`
```
VITE_SUPABASE_URL=...
VITE_SUPABASE_ANON_KEY=...
```

### `pos-app/.env.local`
```
VITE_EDGE_API_URL=http://localhost:8080
```

---

## Supabase

- **Project ref** : `iawyngsvqjsogvkwkrxw`
- **Site de test** : `9983f3ac-cde8-4838-9386-49ef24f57dad`

### Migrations appliquées

| Fichier | Contenu |
|---|---|
| `001_fiscal_schema.sql` | Schéma de base : sessions, fiscal_entries, z_reports |
| `002_roles_rls.sql` | RLS + rôles |
| `003_add_tva_columns.sql` | Colonnes TVA |
| `004_fix_immutability_rules.sql` | Trigger `prevent_delete` sur fiscal_entries |
| `005_backoffice_views_anon_read.sql` | Vues SECURITY DEFINER + grant anon (dashboard, journal) |
| `006_z_reports_anon_read.sql` | Policy RLS anon + grant (rapports Z) |

### Contraintes cloud importantes
- `fiscal_entries` est **immuable** — trigger `prevent_delete` actif, aucun DELETE possible.
- Sync ordre strict : sessions → z_reports → fiscal_entries (FK constraints).
- `run_migrations` est idempotent via la table `_applied_migrations`.

---

## Conventions NF525

- Journal fiscal **append-only** — aucune ligne ne peut être modifiée ou supprimée.
- Chaque entrée est hashée avec SHA-256 et chaînée à la précédente.
- Signature ed25519 sur chaque session clôturée.
- Les rapports Z consolident et clôturent une période (journée / service).

---

## Tests

```
fiscal-engine : ~132 tests unitaires
sync-client   : ~34 tests unitaires + tests E2E (--ignored, nécessite .env.test)
pos-app       : ~24 tests Vitest
edge-api      : 0 test (à faire)
```

Les tests E2E frappent le **vrai Supabase** (pas de mock). Ils sont marqués `#[ignore]` et se lancent explicitement.

---

## CI (GitHub Actions)

`.github/workflows/ci.yml` :
1. `cargo fmt --check`
2. `cargo clippy -- -D warnings` (pedantic)
3. `cargo test --workspace`
4. `cargo build --release`

SQLite in-memory pour les tests en CI. Clippy pedantic — tout warning = échec.

---

## Backlog Sprint 1 (état 2026-05-21)

- [ ] Sécuriser `public.delete_test_data()` — renommer en `delete_test_data_dev()` ou ajouter garde d'environnement
- [ ] Câbler `ZReportScreen` dans pos-app
- [ ] Câbler `CancelScreen` dans pos-app
- [ ] Conformité multi-TVA (NF525)
- [ ] Tests edge-api

Voir `PLAN.md` pour le détail complet du sprint.
