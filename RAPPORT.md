# Rapport de projet — pos-fiscal
_Mis à jour le 2026-07-04_

---

## 1. Objectif

Développer un **système de caisse (POS) open-source conforme NF525** pour une chaîne de restauration rapide (QSR) en France.

La conformité NF525 est une exigence légale française (certification LNE) imposant :
- Un journal fiscal **append-only** avec hash SHA-256 chaîné
- Une signature Ed25519 sur chaque session clôturée
- Des rapports Z de clôture de période
- Des archives annuelles CSV signées pour contrôle fiscal

L'architecture est **offline-first** : les caisses fonctionnent sans réseau et synchronisent vers le cloud en arrière-plan.

---

## 2. Architecture générale

```
pos-app (Electron + React 18 + Vite + Zustand + Tailwind)
    │  HTTP LAN (port 8080)
    ▼
edge-api (Rust / Axum — moteur fiscal local, SQLite WAL)
    │
    ├── fiscal-engine (NF525 : hash-chain, Z-reports, archives CSV)
    ├── kds-engine   (KDS : routing, SSE broadcaster, state machine, print)
    └── promo-engine (évaluateur de promotions multi-TVA)
    │
    ▼
sync-client (agent Rust — sync offline-first SQLite → Supabase)
    │  HTTPS
    ▼
Supabase (PostgreSQL cloud — journal centralisé, config, users)
    │
    ▼
backoffice (React 19 + React Router 7 — dashboard / admin / cuisine)

kds-app (React 19 + Vite — affichages cuisine, standalone)
kds-print-agent (Rust — proxy HTTP → device USB imprimante)
```

---

## 3. Plan de sprints

| Sprint | Thème | Statut |
|--------|-------|--------|
| Sprint 1 | Moteur fiscal NF525 (fiscal-engine + SQLite) | ✅ Terminé |
| Sprint 2 | API edge + sync cloud (edge-api + sync-client) | ✅ Terminé |
| Sprint 3 | Application caisse (pos-app + promotions + impression + E2E) | ✅ Terminé |
| Sprint 4 | Back-office : auth + multi-sites + admin réseau | ✅ Terminé |
| Sprint 5 | KDS — Kitchen Display System (3 plans, ~30 commits) | ✅ Terminé |
| Sprint 6 | KDS Failover — heartbeat + reroutage automatique | ✅ Terminé |

---

## 4. Ce qui est fait et testé

### Sprints 1-2 — Moteur fiscal + Backend Rust

#### `fiscal-engine` — ✅ Complet (133 tests)
- Journal SQLite append-only avec trigger `prevent_delete`
- Hash SHA-256 chaîné (`HashInput` = tva_rate_byte + ht_cents + tva_cents — figé pour LNE)
- Support multi-TVA : 5,5 % / 10 % / 20 % avec breakdown additif par commande
- Signature Ed25519 sur chaque session clôturée
- Rapports Z de clôture (consolidation et archivage de période)
- Archives annuelles CSV UTF-8 BOM, séparateur `;`, 13 colonnes, signées Ed25519

#### `edge-api` (Axum) — ✅ Complet (20 tests d'intégration)
| Route | Description |
|-------|-------------|
| `GET /api/v1/health` | Healthcheck |
| `GET /api/v1/menu` | Carte active (lit `{DATA_DIR}/menu.json`) |
| `POST /api/v1/sessions/open` | Ouvrir une session |
| `POST /api/v1/sessions/close` | Clôturer (génère rapport Z) |
| `POST /api/v1/orders` | Créer une vente (multi-TVA) |
| `POST /api/v1/orders/:id/pay` | Valider le paiement + dispatch KDS |
| `POST /api/v1/orders/:id/cancel` | Annuler (motif obligatoire) |
| `POST /api/v1/archive/:year` | Générer l'archive annuelle NF525 §7 |
| `GET /api/v1/kds/feed/:station_id` | SSE temps réel par station |
| `POST /api/v1/kds/ack` | Acquitter ligne ou commande |
| `POST /api/v1/kds/served` | Marquer commande servie |
| `GET /api/v1/kds/config` | Profil actif (normal/rush) |
| `PUT /api/v1/kds/config` | Changer de profil |
| `GET /api/v1/kds/stations` | Liste stations du profil actif |
| `POST /api/v1/kds/heartbeat/:station_id` | Heartbeat de présence écran KDS → 204 |

#### `sync-client` — ✅ Complet (38 tests + 2 E2E Supabase réel)
- Sync sessions → z_reports → fiscal_entries (ordre strict pour FK cloud)
- Idempotence complète (re-sync sans doublons)
- Pull config + promotions + **config KDS (5 tables SQLite)** depuis Supabase
- Pull KDS non-fatal : `warn!` et continue si Supabase injoignable — jamais bloquant
- Fix critique (2026-06-08) : réduction egress Supabase **de 13 GB → < 1 GB/mois**

#### `promo-engine` — ✅ Complet
- Types : buy-X-get-Y, réduction %, montant fixe, combo
- Fenêtres temporelles et validité par site
- Allocation TVA proportionnelle sur les remises

---

### Sprint 3 — Application caisse (pos-app) + E2E

#### `pos-app` (Electron + React) — ✅ Complet (~24 tests Vitest)
- Écrans : OrderScreen, PaymentScreen, TicketScreen, ZReportScreen
- Annulation commande avec motif obligatoire
- Module `printer.ts` — `formatTicket()` + `printViaElectron()` via IPC Electron
- Tests E2E Playwright (3 flows : commande, annulation, rapport Z)

---

### Sprint 4 — Admin multi-restaurant

#### Back-office admin — ✅ Complet
- Auth : LoginPage, AuthContext, ProtectedRoute
- Multi-sites : SiteContext, dropdown, filtres `.eq('site_id')`
- CRUD : SiteList/Form, UserList/Form, GroupList/Form
- TechnicalConfigForm (edge_api_port, sync_interval, clé Ed25519)
- PermissionsMatrix (lock/unlock par scope réseau/groupe/site)
- Edge Functions : `user-admin`, `config-provision`
- Rôles : `pos_admin` (rang 4), `regional_director`, `manager`, `employee`

#### Migrations Supabase — 21 appliquées
| Migrations | Contenu |
|-----------|---------|
| 001-008 | Schéma fiscal, RLS, TVA, trigger prevent_delete, vues, site_configs |
| 009-011 | Policies backoffice, products, combos |
| 012-015 | Restaurant groups, promotions, approval thresholds |
| 016-020 | site_technical_configs, network_permissions, device_type, fix list_admin_users |
| **021** | **Tables KDS cloud (5 tables) + RLS site-scoped + seeds profils/triggers** |

---

### Sprint 5 — KDS Kitchen Display System ✅ (2026-07-03)

#### `kds-engine` (crate Rust) — ✅ Complet
- Types : `OrderType`, `KdsEvent`, `Station`, `RoutingRule`, `StationStatus`
- Routing engine : `resolve_stations()` par profil actif, channel, order_type
- `KdsBroadcaster` : tokio broadcast wrappé, channel par station
- State machine : `acknowledge`, `mark_served`, `dispatch_order`
- **Failover** : `is_online` + `resolve_effective_station` — reroutage automatique si station hors délai heartbeat
- `dispatch_order` en double-pass : Phase 1 routing rules → Phase 2 failover → Phase 3 dispatch unique par station effective
- Formatteurs ESC/POS : receipt (80mm), linerless, WYSIWYG fichier
- Print dispatcher : TCP/IP direct, proxy USB agent, sortie fichier

#### `kds-print-agent` (binaire Rust) — ✅ Complet
- Axum `POST /print` → écriture directe sur device USB (`/dev/usb/lp0`)
- Port configurable via `AGENT_PORT` (défaut 6611)

#### `kds-app` (React 19 + Vite + TypeScript strict + TailwindCSS) — ✅ Complet
- Routing via `window.location.pathname` (pas de React Router)
- `useKdsFeed` : EventSource SSE, Map-based state, backoff exponentiel 1 s → 30 s
- **Heartbeat** : `startHeartbeat()` — `POST /api/v1/kds/heartbeat/:stationId` toutes les 10 s, cleanup au unmount
- Composants : `TimerBadge` (coloré, 1 s), `ConnectionBanner`, `OrderCard`
- Vues : `PreparationStation`, `OrderReadyBoard` (ORB 2 colonnes), `ConfigPage`
- Routes : `/:stationId` → préparation, `/ready_board` → ORB, `/config` → config
- Tests : 2 Vitest (Vitest 2 + jsdom)

#### Back-office Cuisine — ✅ Complet
- `KdsStations` : liste + delete par site
- `KdsStationForm` : create/edit (tous champs, champs imprimante conditionnels)
- `KdsRoutingRules` : règles groupées par profil, ajout inline
- `KdsTimerThresholds` : seuils warning/critical éditables par station
- Navigation « Cuisine » ajoutée dans Layout.tsx + 5 routes `/kitchen/*`

### Sprint 6 — KDS Failover ✅ (2026-07-04)

- **`kds-engine`** : fonctions privées `is_online` + `resolve_effective_station` (8 tests unitaires)
- **`dispatch_order`** refactoré en double-pass : accumulation des lignes par station effective avant dispatch — évite tout double-broadcast si station primaire et repli reçoivent des lignes du même canal
- **`AppState`** : `station_heartbeats: Arc<DashMap<String, Instant>>` + `kds_heartbeat_timeout_secs: u64` (env `KDS_HEARTBEAT_TIMEOUT_SECS`, défaut 30 s)
- **`POST /api/v1/kds/heartbeat/:station_id`** → 204, aucune validation — convention safe-default : absent de la map = online
- **`kds-app`** : `startHeartbeat()` exportée, câblée dans `useKdsFeed` useEffect, 2 tests Vitest
- Dépendance `dashmap = "6"` ajoutée au workspace

---

## 5. Ce qui reste à faire

### Backlog KDS (post-MVP)

| Item | Priorité | Note |
|------|----------|------|
| **Page `/kitchen/triggers`** | Moyenne | Gestion déclencheurs canal × order_type depuis le back-office (defaults seedés en migration 021) |
| **kds-app servi par edge-api** | Moyenne | `ServeDir` Axum → `/kds/*` (actuellement SPA indépendante) |
| **`generate_short_number` race** | Basse | Légère race sur le compteur journalier en charge élevée |
| **`send_usb_agent` status check** | Basse | Pas de vérification du code HTTP retourné par kds-print-agent |

### Backlog long terme (hors MVP)

| Item | Note |
|------|------|
| **Intégration TPE** | Ingenico / Verifone — non démarré |
| **UI Remboursements/Remises** | Routes `/cancel` existantes, UI pos-app manquante |
| **Mode formation** | `OperationType::Training` non implémenté dans fiscal-engine |
| **Auto-génération clé signing** | `FISCAL_SIGNING_KEY_HEX` générée manuellement — automatiser au 1er démarrage edge-api |
| **Archive auto 1er janvier** | Tâche planifiée sync-client manquante |
| **Tests back-office** | Aucun test automatisé sur les pages admin/cuisine |
| **Migration 021 push Supabase** | `supabase db push` à lancer (nécessite réseau + projet ref `iawyngsvqjsogvkwkrxw`) |

---

## 6. Points d'attention permanents

| Règle | Raison |
|-------|--------|
| Ne jamais modifier `HashInput` | Hash figé pour certification LNE |
| Ne jamais committer `SUPABASE_SERVICE_KEY` | Accès total sans RLS |
| Ne jamais committer `FISCAL_SIGNING_KEY_HEX` | Clé privée Ed25519 archives fiscales |
| Sync ordre strict : `sessions → z_reports → fiscal_entries` | FK cloud — violation = erreur 23503 |
| CI : `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic` | Commande réelle dans `.github/workflows/ci.yml` — plus stricte que la forme courte |
| Tests E2E sync avec `--test-threads=1` + pre-cleanup | Tests E2E frappent le vrai Supabase — parallélisme = conflicts FK |
| Pull KDS non-fatal dans sync-client | Ne jamais propager l'erreur KDS — `warn!` et continuer le cycle fiscal |
| Failover KDS non-bloquant | `dispatch_order` dans `tokio::spawn` — l'erreur KDS ne remonte jamais en HTTP 500 |
| Heartbeat timeout défaut 30 s | Configurable via `KDS_HEARTBEAT_TIMEOUT_SECS` — absent de la map = online (safe-default démarrage) |

---

## 7. Couverture de tests

| Composant | Tests | Statut |
|-----------|-------|--------|
| `fiscal-engine` | 133 unitaires | ✅ |
| `sync-client` | 38 unitaires + 2 E2E (Supabase réel) | ✅ |
| `edge-api` | 18 intégration (Axum oneshot + SQLite tempfile) | ✅ |
| `kds-engine` | ~39 unitaires | ✅ |
| `common` | 7 unitaires | ✅ |
| `pos-app` | ~24 Vitest + 3 Playwright E2E | ✅ |
| `backoffice` | Aucun test automatisé | ❌ À faire |
| `kds-app` | 2 Vitest (heartbeat) | ✅ |

**Total Rust : ~240 tests — tous verts.**

---

## 8. Commandes de reprise

```bash
# CI complète Rust (commande réelle GitHub Actions)
cargo fmt --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic && \
cargo test --workspace && \
cargo build --release

# Tests E2E sync (vrai Supabase — .env.test requis)
source .env.test && cargo test -p sync-client --test e2e_sync -- --ignored --nocapture --test-threads=1

# edge-api local
DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api

# kds-app (dev)
cd kds-app && npm run dev     # port 5174 (VITE_EDGE_API_URL=http://localhost:8080)

# Back-office
cd backoffice && npm run dev  # port 5173

# pos-app (dev React)
cd pos-app && npm run dev     # port 5175

# pos-app (Electron dev)
cd pos-app && npm run electron:dev

# Tests E2E Playwright pos-app
cd pos-app && npm run test:e2e

# Tests kds-app (heartbeat)
cd kds-app && npm test

# Appliquer la migration 021 KDS sur Supabase cloud
supabase db push

# Vérifier l'état des migrations
supabase migration list
```

### Créer un utilisateur pos_admin (Supabase SQL Editor)
```sql
UPDATE auth.users
SET raw_app_meta_data = jsonb_set(COALESCE(raw_app_meta_data, '{}'::jsonb), '{role}', '"pos_admin"')
WHERE email = 'angelo.melleraud@gmail.com';
```
