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
fiscal-engine (Rust — NF525 : hash-chain, Z-reports, archives)
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
| `fiscal-engine` | Moteur NF525 : journal SQLite append-only, hash SHA-256 chaîné, signature ed25519, rapports Z, archives annuelles |
| `edge-api` | Serveur HTTP Axum (LAN only) — routes sessions, orders, menu, health, archive |
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
DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api

# sync-client
source .env && cargo run -p sync-client

# Back-office
cd backoffice && npm run dev

# pos-app (dev React)
cd pos-app && npm run dev

# pos-app (Electron dev — compile TS puis démarre)
cd pos-app && npm run electron:dev

# pos-app (build Electron distributable)
cd pos-app && npm run electron:build

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

### Variables edge-api (optionnelles)
```
DATA_DIR=./data                  # répertoire menu.json + archives CSV (défaut: ./data)
EDGE_API_PORT=8080               # port d'écoute (défaut: 8080)
FISCAL_SIGNING_KEY_HEX=<64chars> # clé Ed25519 privée pour archives (64 hex chars)
SITE_ID=<uuid>                   # identifiant du site (pour la signature d'archive)
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
| `007_secure_delete_test_data.sql` | REVOKE EXECUTE delete_test_data → service_role only |
| `008_site_configs.sql` | Table site_configs + RLS + seed menu de démo |

### Contraintes cloud importantes
- `fiscal_entries` est **immuable** — trigger `prevent_delete` actif, aucun DELETE possible.
- Sync ordre strict : sessions → z_reports → fiscal_entries (FK constraints).
- `run_migrations` est idempotent via la table `_applied_migrations`.

---

## Routes edge-api

| Méthode | Chemin | Description |
|---|---|---|
| GET | `/api/v1/health` | Healthcheck |
| GET | `/api/v1/menu` | Carte active (lit `{DATA_DIR}/menu.json`) |
| GET | `/api/v1/sessions/current` | Session active |
| POST | `/api/v1/sessions/open` | Ouvrir une session |
| POST | `/api/v1/sessions/close` | Clôturer (génère rapport Z) |
| POST | `/api/v1/orders` | Créer une vente (`line_items[]` multi-TVA) |
| GET | `/api/v1/orders/:id` | Consulter une commande |
| POST | `/api/v1/orders/:id/pay` | Valider le paiement |
| POST | `/api/v1/orders/:id/cancel` | Annuler (motif obligatoire) |
| POST | `/api/v1/archive/:year` | Générer l'archive annuelle NF525 §7 |

---

## Conventions NF525

- Journal fiscal **append-only** — aucune ligne ne peut être modifiée ou supprimée.
- Chaque entrée est hashée avec SHA-256 et chaînée à la précédente.
- **Hash figé pour certification LNE** — ne jamais modifier `HashInput` (tva_rate_byte + ht_cents + tva_cents).
- Multi-TVA : `tva_5_5_breakdown`, `tva_10_breakdown`, `tva_20_breakdown` stockés additivement. Le champ `tva_breakdown` est le dominant (pour le hash) et la somme totale.
- Signature ed25519 sur chaque session clôturée.
- Les rapports Z consolident et clôturent une période (journée / service).
- Archives annuelles CSV UTF-8 BOM, séparateur `;`, signées Ed25519 (`FISCAL_SIGNING_KEY_HEX`).

---

## Tests

```
fiscal-engine : ~132 tests unitaires
sync-client   : ~34 tests unitaires + 2 tests E2E (--ignored, nécessite .env.test)
pos-app       : ~24 tests Vitest
edge-api      : 18 tests d'intégration (Axum oneshot, SQLite tempfile)
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

## Electron (pos-app)

- `electron/main.ts` — process principal, fenêtre kiosk en prod, HTTP vers edge-api
- `electron/preload.ts` — contextBridge : `getApiUrl()`, `printText(text)`
- `tsconfig.electron.json` — compilation CommonJS in-place (→ `electron/*.js`, ignorés par git)
- `npm run electron:dev` — compile TS Electron, puis lance Vite + Electron en parallèle
- `npm run electron:build` — compile TS, build Vite, package via electron-builder

---

## Génération de la clé de signature archive

```bash
# Via les tests unitaires fiscal-engine (affiche les bytes)
cargo test -p fiscal-engine generate_keypair -- --nocapture

# Ou en Rust :
use fiscal_engine::archive_engine::generate_signing_keypair;
let (priv_bytes, pub_bytes) = generate_signing_keypair();
// hex-encoder priv_bytes → FISCAL_SIGNING_KEY_HEX
```

La clé privée (32 octets = 64 hex chars) est stockée dans `FISCAL_SIGNING_KEY_HEX`.  
La clé publique (32 octets = 64 hex chars) est stockée avec chaque archive pour vérification LNE.  
**Ne jamais committer ces clés.**
