# Spec — KDS (Kitchen Display System)
_2026-07-02_

---

## 1. Objectif

Système d'affichage et d'impression cuisine (KDS) pour une chaîne QSR. Chaque commande est routée vers les stations de préparation concernées dès qu'elle est créée ou payée (selon le canal), affichée sur des écrans et/ou imprimée sur des imprimantes thermiques. Un Order Ready Board (ORB) informe le client ou le livreur que sa commande est prête.

---

## 2. Architecture générale (Option B)

```
pos-app / kiosk-app / drive-app / delivery-platform
    │  POST /orders + /orders/:id/pay
    ▼
edge-api (Axum)
    ├── fiscal-engine (existant)
    └── kds-engine (nouveau crate Rust)
          ├── Routing : produit → stations (profil actif)
          ├── Trigger : configurable par canal et order_type
          ├── State machine : new → in_progress → ready → held → assembled → served
          ├── SSE broadcaster (tokio::sync::broadcast par station_id)
          ├── Print dispatcher (TCP/IP | USB agent | file)
          └── Failover : fallback_station_id si device primaire KO

Routes KDS :
  GET  /api/v1/kds/feed/:station_id     → SSE stream
  GET  /api/v1/kds/feed/ready_board     → SSE ORB client + livreur
  POST /api/v1/kds/orders/:id/ack       → bump bon entier
  POST /api/v1/kds/orders/:id/lines/:lid/ack → bump ligne
  POST /api/v1/kds/orders/:id/served    → 2e bump expo (retire de l'ORB)
  GET  /api/v1/kds/stations             → liste stations actives
  GET|PUT /api/v1/kds/config            → config locale (profil actif, overrides)

SQLite (restaurant.db) :
  kds_stations, kds_routing_rules, kds_routing_profiles,
  kds_channel_triggers, kds_timer_thresholds,
  kds_orders, kds_order_lines, kds_failover_log, kds_print_log

kds-app (nouveau projet Vite + React)
  http://[edge-api-ip]:8080/kds/[station_id]
  → tablette / TV / navigateur quelconque sur le LAN

Page config locale (HTML pur, servi par edge-api)
  http://[edge-api-ip]:8080/kds/config
  → toggle profil Rush / Normal, overrides stations

backoffice (nouvelles pages section "Cuisine")
  → Supabase : kds_station_configs, kds_routing_configs, kds_routing_profiles
  → sync-client pull → SQLite local
```

**Latence SSE LAN :** < 10 ms (tokio broadcast in-process + SQLite WAL + TCP LAN).

---

## 3. Workflow de production

Les commandes apparaissent **simultanément sur toutes les stations concernées** dès le déclenchement. Pas de séquencement bloquant — uniquement une mise en évidence visuelle de l'étape courante.

```
NIVEAU 1 — PRÉPARATION (n stations spécialisées ou polyvalentes)
  Grill, Friture, Froid, Boissons…
  → Voit SES articles filtrés par routage
  → Bump ligne = article prêt           [⏱ timestamp capturé]
  → Bump en-tête = station entière prête [⏱ timestamp capturé]

NIVEAU 2 — RASSEMBLEMENT (par groupe de température)
  Chaud / Froid / Autres
  → Voit ses articles + statut des N1 associés
  → Bump = composants réceptionnés      [⏱ timestamp capturé]

NIVEAU 3 — ASSEMBLAGE EXPO
  Plateau / Sac
  → Voit commande complète + statut agrégé toutes stations
  → Bump 1 = commande prête → ORB colonne "Prêt" [⏱ timestamp capturé]
  → Bump 2 = commande servie → retire de l'ORB  [⏱ timestamp capturé]

ORDER READY BOARD (ORB)
  → Apparaît colonne gauche "En préparation" dès déclenchement KDS
  → Passe colonne droite "Prêt" au bump 1 expo
  → Disparaît au bump 2 expo (servi / récupéré)
```

**Machine d'états par (order_id, station_id) :**
`new → in_progress → ready → held → assembled → served`

**Métriques capturées :** temps préparation N1, attente N2, total N1→N3, service complet.

---

## 4. Déclencheurs par canal et order_type

Le champ `order_type` (`eat_in | takeaway | click_and_collect | delivery | drive`) est transmis à la création de commande et conditionne le routage ORB.

| Canal | order_type | Trigger KDS | ORB |
|---|---|---|---|
| Caisse | eat_in | paiement | aucun |
| Caisse | takeaway | paiement | Client |
| Kiosk | eat_in | commande | aucun |
| Kiosk | takeaway | commande | Client |
| Drive | drive | paiement | aucun |
| Livraison | delivery | commande | Livreur |
| Livraison | click_and_collect | commande | Client |

Configurable par restaurant dans `kds_channel_triggers`.

---

## 5. Modèle de données SQLite

```sql
kds_stations (
  id TEXT PK,
  name TEXT NOT NULL,
  role TEXT NOT NULL,               -- 'preparation'|'holding'|'assembly'|'ready_board'
  temperature_group TEXT,           -- 'hot'|'cold'|'other'|NULL
  output_type TEXT NOT NULL,        -- 'screen'|'printer'|'both'
  printer_address TEXT,             -- IP:port | /dev/usb/lp0 | /chemin/dossier (file)
  printer_type TEXT,                -- 'tcpip'|'usb'|'file'
  printer_mode TEXT,                -- 'receipt'|'linerless_label'
  paper_width_mm INTEGER,           -- 80 ou 50
  fallback_station_id TEXT REFERENCES kds_stations(id),
  active_in_profiles TEXT NOT NULL DEFAULT '["normal"]', -- JSON array
  sort_order INTEGER NOT NULL DEFAULT 0,
  enabled INTEGER NOT NULL DEFAULT 1
)

kds_routing_profiles (
  id TEXT PK,                       -- 'normal'|'rush'|custom
  name TEXT NOT NULL,
  description TEXT
)

kds_routing_rules (
  id TEXT PK,
  profile_id TEXT NOT NULL REFERENCES kds_routing_profiles(id),
  rule_type TEXT NOT NULL,          -- 'category'|'product'|'tag'
  match_value TEXT NOT NULL,
  station_ids TEXT NOT NULL,        -- JSON array
  priority INTEGER NOT NULL DEFAULT 0
)

kds_channel_triggers (
  channel TEXT NOT NULL,
  order_type TEXT NOT NULL,
  trigger_on TEXT NOT NULL,         -- 'order'|'payment'|'both'
  orb_type TEXT,                    -- 'client'|'livreur'|NULL
  PRIMARY KEY (channel, order_type)
)

kds_timer_thresholds (
  station_id TEXT PK REFERENCES kds_stations(id),
  warning_secs INTEGER NOT NULL DEFAULT 120,
  critical_secs INTEGER NOT NULL DEFAULT 300
)

kds_active_profile (
  singleton INTEGER PK DEFAULT 1 CHECK (singleton = 1),
  profile_id TEXT NOT NULL DEFAULT 'normal'
)

kds_orders (
  order_id TEXT NOT NULL,
  station_id TEXT NOT NULL,
  order_number_short TEXT NOT NULL,
  external_order_id TEXT,           -- réf Deliveroo / Uber Eats
  channel TEXT NOT NULL,
  order_type TEXT NOT NULL,
  customer_name TEXT,
  status TEXT NOT NULL DEFAULT 'new',
  stage TEXT NOT NULL DEFAULT 'preparation',
  station_statuses TEXT,            -- JSON snapshot {station_name: status}
  arrived_at INTEGER NOT NULL,
  first_bump_at INTEGER,
  ready_at INTEGER,
  served_at INTEGER,
  PRIMARY KEY (order_id, station_id)
)

kds_order_lines (
  order_id TEXT NOT NULL,
  line_id TEXT NOT NULL,
  station_id TEXT NOT NULL,
  product_name TEXT NOT NULL,
  quantity INTEGER NOT NULL DEFAULT 1,
  parent_line_id TEXT,              -- NULL = article racine
  line_type TEXT NOT NULL,          -- 'item'|'combo_component'|'modifier'|'comment'
  comment TEXT,
  acknowledged INTEGER NOT NULL DEFAULT 0,
  acknowledged_at INTEGER,
  PRIMARY KEY (order_id, line_id, station_id)
)

kds_failover_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL,
  order_id TEXT NOT NULL,
  primary_station_id TEXT NOT NULL,
  fallback_station_id TEXT NOT NULL,
  reason TEXT
)

kds_print_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  ts INTEGER NOT NULL,
  order_id TEXT NOT NULL,
  station_id TEXT NOT NULL,
  attempt INTEGER NOT NULL DEFAULT 1,
  result TEXT NOT NULL,             -- 'ok'|'error'|'failover'
  error_msg TEXT
)
```

---

## 6. API edge-api — routes KDS

### SSE — format des événements

```
event: order_new
data: {
  "order_id": "...",
  "order_number_short": "A42",
  "external_order_id": "UE-98765",     // null si absent
  "channel": "delivery",
  "order_type": "delivery",
  "customer_name": "Jean D.",          // null si absent
  "stage": "preparation",
  "lines": [
    {
      "line_id": "...",
      "product_name": "Burger Classic",
      "quantity": 2,
      "parent_line_id": null,
      "line_type": "item",
      "comment": "sans oignon",
      "acknowledged": false
    },
    {
      "line_id": "...",
      "product_name": "Pain brioche",
      "quantity": 2,
      "parent_line_id": "burger-line-id",
      "line_type": "combo_component",
      "comment": null,
      "acknowledged": false
    }
  ],
  "station_statuses": {
    "Grill": "in_progress",
    "Friture": "ready",
    "Boissons": "ready"
  },
  "arrived_at": 1783013313000,
  "timer_thresholds": { "warning_secs": 120, "critical_secs": 300 }
}

event: order_updated
data: {
  "order_id": "...",
  "status": "paid"|"modified"|"cancelled",
  "stage": "holding",
  "station_statuses": { ... }
}

event: order_acked
data: { "order_id": "...", "station_id": "...", "line_id": null|"..." }
```

Reconnexion SSE : header `Last-Event-ID` → edge-api rejoue les événements manqués depuis SQLite (conservés 24 h). Bandeau rouge côté client si EventSource en état `CLOSED` > 3 s.

### Broadcaster interne

```
POST /orders/:id/pay
  → fiscal-engine (existant)
  → kds-engine::on_payment(order)
      → évalue trigger du canal/order_type
      → route_to_stations(order, active_profile)
      → écrit kds_orders + kds_order_lines
      → broadcast event sur chaque station receiver
      → dispatch impression si output_type inclut printer
```

---

## 7. kds-app — UI

### Accès

`http://[edge-api-ip]:8080/kds/[station_id]` — n'importe quel navigateur sur LAN.

### Vue Préparation / Rassemblement

Grille horizontale de cartes (~280 px), triées par `arrived_at` ASC (plus ancienne à gauche).

Chaque carte :
- **En-tête** : `[A42]` + icône canal + timer coloré + badge étape (`PREP`/`HOLD`/`ASSE`)
- **Badge urgence** : fond orange clignotant si modifiée, rouge si annulée
- **Lignes filtrées** par station, avec indentation :
  - `☐ 2× Burger Classic` — clic = acknowledge ligne
  - `  ↳ sans oignon` — modifier indenté
  - `  ↳ Pain brioche ×2` — composant combo indenté
  - `  💬 commentaire libre` — italique
- **Pied** : `N articles` + `[TOUT ✓]` (bump en-tête)
- **Couleur timer** : vert < warning_secs, orange < critical_secs, rouge au-delà

### Vue Assemblage Expo

Même grille + **statut agrégé** des stations upstream dans chaque carte :
```
Grill    ✅ prêt
Friture  ⏳ en cours
Froid    ✅ prêt
```
Bouton `[PRÊT →]` → bump 1 (passe en ORB colonne "Prêt").
Bouton `[SERVI ✓]` → bump 2 (retire de l'ORB).

### Vue Order Ready Board

Plein écran, 2 colonnes, gros texte lisible à 5 m :

```
┌──────────────────────┬──────────────────────┐
│   EN PRÉPARATION     │        PRÊT          │
│   A42  Jean D.       │   A38                │
│   A44                │   A39  Marie L.      │
│   B01  UE-98765      │   A40                │
└──────────────────────┴──────────────────────┘
```

Deux instances distinctes : ORB Client (takeaway + click_and_collect) et ORB Livreur (delivery).

### Comportements transverses

- Reconnexion SSE automatique avec backoff exponentiel + bandeau rouge si déconnecté
- Son configurable à l'arrivée d'un nouveau bon
- `?fullscreen=true` dans l'URL pour mode kiosque
- Thème sombre par défaut, clair optionnel

---

## 8. Impression cuisine

### Trois modes (`printer_type`)

| Mode | Mécanisme | Usage |
|---|---|---|
| `tcpip` | `TcpStream::connect(ip:port)` → bytes ESC/POS | Réseau filaire + WiFi |
| `usb` | `POST http://localhost:6611/print` → kds-print-agent | USB local sur Raspberry/PC |
| `file` | Écriture `.txt` dans `DATA_DIR/kds_tickets/` | Test WYSIWYG mise en page |

Le mode `file` utilise exactement les mêmes fonctions de formatage que `receipt`/`linerless_label` mais sans bytes ESC/POS — le fichier texte est le rendu exact du ticket.

**kds-print-agent** : mini-serveur Axum (< 200 lignes), port 6611 configurable, écoute `POST /print` et écrit les bytes sur `/dev/usb/lp0` ou équivalent Windows. Déployé sur le même PC/Raspberry que l'imprimante USB.

### Deux modes papier (`printer_mode`)

**`receipt`** — ticket continu standard :
```
================================
  GRILL          ⏰ 14:32
================================
BON A42          🖥 CAISSE
Jean D.
--------------------------------
  2x BURGER CLASSIC
    > sans oignon
    > Pain brioche x2
  1x BURGER BBQ
--------------------------------
  3 articles
================================
```

**`linerless_label`** — label adhésif 80 mm ou 50 mm, 1 label par article, coupe partielle entre labels :
```
╔══════════════════════════════╗
║ A42  GRILL   14:32          ║
╠══════════════════════════════╣
║  2x BURGER CLASSIC          ║
║    > sans oignon            ║
╚══════════════════════════════╝
- - - - - (coupe partielle) - -
╔══════════════════════════════╗
║ A42  GRILL   14:32          ║
╠══════════════════════════════╣
║  1x BURGER BBQ              ║
╚══════════════════════════════╝
```

Le champ `paper_width_mm` (80 ou 50) ajuste la largeur de colonnes et la police condensée.

### Fiabilité

- 3 retries avec backoff 500 ms → failover vers `fallback_station_id` → log dans `kds_failover_log`
- File d'attente Tokio `mpsc` par imprimante — bons redelivrés dans l'ordre si l'imprimante revient
- Timeout 5 s par tentative
- Chaque tentative loguée dans `kds_print_log`

---

## 9. Profils de routage

Au moins 2 profils configurés dans le back-office :

| Profil | Usage | Stations actives |
|---|---|---|
| `normal` | Service calme, personnel polyvalent | Stations généralistes |
| `rush` | Rush, personnel spécialisé | Stations spécialisées par catégorie |

Le gérant bascule via `http://[edge-api]:8080/kds/config` :

```
┌─────────────────────────────────┐
│  Profil actif                   │
│  ○ Normal   ● RUSH              │
│  [Basculer en mode Normal]      │
└─────────────────────────────────┘
```

Un clic → update `kds_active_profile.profile_id` en SQLite → kds-engine applique immédiatement les nouvelles règles. Les profils sont définis en back-office (Supabase), pullés par sync-client ; le gérant ne fait que choisir lequel activer.

---

## 10. Configuration back-office

Nouvelle section "Cuisine" dans la nav backoffice.

### Nouvelles pages

- `/kitchen/stations` — CRUD stations (rôle, output, imprimante, seuils, fallback, profils)
- `/kitchen/routing` — règles de routage par profil (mode hiérarchie catégorie→produit ou mode tags)
- `/kitchen/triggers` — déclencheurs par canal × order_type + ORB associé

### Migrations Supabase

```sql
-- Tables cloud (RLS : pos_admin / regional_director en écriture)
kds_station_configs     (site_id + champs kds_stations)
kds_routing_configs     (site_id + champs kds_routing_rules)
kds_routing_profiles    (site_id + id + name + description)
kds_channel_triggers    (site_id + channel + order_type + trigger_on + orb_type)
kds_timer_thresholds    (site_id + station_id + warning_secs + critical_secs)
```

Sync-client : pull uniquement (KDS ne pousse pas de données vers Supabase).

### Override local

Page HTML pur servie par edge-api (`/kds/config`) :
- Toggle profil actif (Rush / Normal)
- Activation/désactivation de stations individuelles
- Modification adresse imprimante
- Toutes les overrides stockées en SQLite avec `source = 'local'`, prioritaires sur le pull Supabase, non remontées vers le cloud

---

## 11. Modification du modèle existant edge-api

Le champ `order_type` (`eat_in | takeaway | click_and_collect | delivery | drive`) doit être ajouté :

- Table SQLite `orders` : nouvelle colonne `order_type TEXT NOT NULL DEFAULT 'eat_in'`
- Route `POST /api/v1/orders` : accepte `order_type` dans le body JSON
- Route `POST /api/v1/orders/:id/pay` : passe `order_type` à kds-engine lors du déclenchement
- Struct `Order` dans `common` ou `edge-api` : nouveau champ `order_type`

Migration SQLite : `ALTER TABLE orders ADD COLUMN order_type TEXT NOT NULL DEFAULT 'eat_in'` — idempotente via `_applied_migrations`.

---

## 12. Nouveau crate Rust `kds-engine`

Isolé de `fiscal-engine`. Contient :
- Moteur de routage (évaluation des règles par profil actif)
- State machine des commandes
- Broadcaster SSE (wrappers autour de `tokio::sync::broadcast`)
- Formatteur ESC/POS (receipt + linerless, 80 mm + 50 mm)
- Formatteur file (WYSIWYG texte)
- Print dispatcher (tcpip + usb + file)
- Failover logic

`edge-api` l'importe comme dépendance workspace, identique à `fiscal-engine`.

---

## 12. Hors scope (chantiers futurs)

- **App client** — suivi de commande en temps réel sur device client, historique, notation satisfaction (SSE kds-engine anticipé dans l'architecture)
- **Analytics production** — temps par station, par heure, par canal (`/kitchen/analytics`)
- **kds-print-agent Windows** — agent USB pour OS Windows (à confirmer selon parcs matériels)
- **Intégration plateforme livraison** — Deliveroo / Uber Eats (canal `delivery` câblé, intégration API à spécifier séparément)
