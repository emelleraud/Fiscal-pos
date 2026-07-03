# KDS Failover Station — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Câbler `fallback_station_id` dans `dispatch_order` via un heartbeat HTTP : si une station ne bat plus depuis plus de N secondes, les nouvelles commandes sont reroutées vers sa station de repli.

**Architecture:** `kds-app` envoie `POST /api/v1/kds/heartbeat/:station_id` toutes les 10 s. `AppState` maintient une `DashMap<String, Instant>` en mémoire. Au dispatch, `resolve_effective_station` consulte la map et retourne la station de repli si la primaire est hors délai. Un double-pass sur les lignes garantit un seul `OrderNew` par station effective.

**Tech Stack:** Rust (dashmap 6, std::time::Instant), React 19 (fetch + setInterval), Vitest 2 (jsdom).

## Global Constraints

- `#![deny(clippy::all, clippy::pedantic)]` sur tous les fichiers Rust modifiés — zéro warning en CI
- CI : `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic`
- Ne jamais modifier `HashInput` / journal append-only / signature Ed25519 (fiscal-engine intact)
- Pull KDS dans sync-client reste non-fatal — cette feature ne touche pas sync-client
- Jamais committer `SUPABASE_SERVICE_KEY` ni `FISCAL_SIGNING_KEY_HEX`
- `dispatch_order` reste non-bloquant (spawn tokio) — l'erreur KDS ne doit jamais remonter en HTTP 500

---

## File Map

| Fichier | Action |
|---|---|
| `Cargo.toml` (workspace) | Modifier — ajouter `dashmap` dans `[workspace.dependencies]` |
| `kds-engine/Cargo.toml` | Modifier — référencer dashmap workspace |
| `edge-api/Cargo.toml` | Modifier — référencer dashmap workspace |
| `kds-engine/src/state_machine.rs` | Modifier — ajouter `is_online`, `resolve_effective_station`, refactorer `dispatch_order` en double-pass |
| `edge-api/src/app.rs` | Modifier — ajouter `station_heartbeats` et `kds_heartbeat_timeout_secs` à `AppState` |
| `edge-api/src/routes/kds.rs` | Modifier — ajouter handler `kds_heartbeat` |
| `edge-api/src/routes/mod.rs` | Modifier — enregistrer `POST /api/v1/kds/heartbeat/:station_id` |
| `edge-api/src/routes/orders.rs` | Modifier — passer `heartbeats` + `timeout_secs` à `dispatch_order` |
| `edge-api/tests/api.rs` | Modifier — ajouter 2 tests heartbeat |
| `kds-app/package.json` | Modifier — ajouter vitest + script test |
| `kds-app/vitest.config.ts` | Créer — config jsdom |
| `kds-app/src/hooks/useKdsFeed.ts` | Modifier — extraire `startHeartbeat` + appel dans `useEffect` |
| `kds-app/src/hooks/useKdsFeed.test.ts` | Créer — 2 tests Vitest |

---

## Task 1 — Ajouter la dépendance dashmap

**Files:**
- Modify: `Cargo.toml`
- Modify: `kds-engine/Cargo.toml`
- Modify: `edge-api/Cargo.toml`

**Interfaces:**
- Produces: `dashmap::DashMap` disponible dans kds-engine et edge-api

- [ ] **Step 1 — Ajouter dashmap dans workspace**

Dans `Cargo.toml`, section `[workspace.dependencies]`, ajouter :

```toml
dashmap = "6"
```

- [ ] **Step 2 — Référencer dans kds-engine**

Dans `kds-engine/Cargo.toml`, section `[dependencies]`, ajouter :

```toml
dashmap = { workspace = true }
```

- [ ] **Step 3 — Référencer dans edge-api**

Dans `edge-api/Cargo.toml`, section `[dependencies]`, ajouter :

```toml
dashmap = { workspace = true }
```

- [ ] **Step 4 — Vérifier la compilation**

```bash
cargo check --workspace
```

Résultat attendu : zéro erreur, zéro warning.

- [ ] **Step 5 — Commit**

```bash
git add Cargo.toml kds-engine/Cargo.toml edge-api/Cargo.toml Cargo.lock
git commit -m "chore: add dashmap workspace dependency for KDS heartbeat"
```

---

## Task 2 — kds-engine : logique pure (TDD)

**Files:**
- Modify: `kds-engine/src/state_machine.rs`

**Interfaces:**
- Produces:
  - `fn is_online(heartbeats: &dashmap::DashMap<String, std::time::Instant>, station_id: &str, timeout_secs: u64) -> bool`
  - `fn resolve_effective_station<'a>(primary: &'a Station, all_stations: &'a [Station], heartbeats: &dashmap::DashMap<String, std::time::Instant>, timeout_secs: u64) -> Option<&'a Station>`

- [ ] **Step 1 — Écrire les tests unitaires**

En bas de `kds-engine/src/state_machine.rs`, dans le module `#[cfg(test)]` existant (ou en créer un), ajouter :

```rust
#[cfg(test)]
mod failover_tests {
    use super::{is_online, resolve_effective_station};
    use crate::types::station::{OutputType, Station, StationRole};
    use dashmap::DashMap;
    use std::time::{Duration, Instant};

    fn station(id: &str, fallback: Option<&str>) -> Station {
        Station {
            id: id.to_string(),
            name: id.to_string(),
            role: StationRole::Preparation,
            temperature_group: None,
            output_type: OutputType::Screen,
            printer_address: None,
            printer_type: None,
            printer_mode: None,
            paper_width_mm: None,
            fallback_station_id: fallback.map(str::to_string),
            active_in_profiles: vec!["normal".to_string()],
            sort_order: 1,
            enabled: true,
        }
    }

    #[test]
    fn is_online_absent_means_online() {
        let hb: DashMap<String, Instant> = DashMap::new();
        assert!(is_online(&hb, "grill", 30));
    }

    #[test]
    fn is_online_recent_heartbeat() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now());
        assert!(is_online(&hb, "grill", 30));
    }

    #[test]
    fn is_online_respects_timeout() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now() - Duration::from_secs(31));
        assert!(!is_online(&hb, "grill", 30));
    }

    #[test]
    fn resolve_returns_primary_when_online() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now());
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone(), station("cold", None)];
        let result = resolve_effective_station(&primary, &all, &hb, 30);
        assert_eq!(result.map(|s| s.id.as_str()), Some("grill"));
    }

    #[test]
    fn resolve_returns_fallback_when_primary_down() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now() - Duration::from_secs(31));
        hb.insert("cold".to_string(), Instant::now());
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone(), station("cold", None)];
        let result = resolve_effective_station(&primary, &all, &hb, 30);
        assert_eq!(result.map(|s| s.id.as_str()), Some("cold"));
    }

    #[test]
    fn resolve_returns_none_when_no_fallback_configured() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now() - Duration::from_secs(31));
        let primary = station("grill", None);
        let all = vec![primary.clone()];
        assert!(resolve_effective_station(&primary, &all, &hb, 30).is_none());
    }

    #[test]
    fn resolve_returns_none_when_fallback_also_down() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now() - Duration::from_secs(31));
        hb.insert("cold".to_string(), Instant::now() - Duration::from_secs(31));
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone(), station("cold", None)];
        assert!(resolve_effective_station(&primary, &all, &hb, 30).is_none());
    }

    #[test]
    fn resolve_returns_none_when_fallback_not_in_profile() {
        let hb: DashMap<String, Instant> = DashMap::new();
        hb.insert("grill".to_string(), Instant::now() - Duration::from_secs(31));
        // "cold" n'est pas dans all_stations (non dans le profil actif)
        let primary = station("grill", Some("cold"));
        let all = vec![primary.clone()];
        assert!(resolve_effective_station(&primary, &all, &hb, 30).is_none());
    }
}
```

- [ ] **Step 2 — Vérifier que les tests échouent**

```bash
cargo test -p kds-engine failover_tests 2>&1 | head -30
```

Résultat attendu : erreurs de compilation (`is_online` et `resolve_effective_station` not found).

- [ ] **Step 3 — Implémenter is_online et resolve_effective_station**

Dans `kds-engine/src/state_machine.rs`, ajouter après les imports existants (avant `pub struct IncomingOrder`) :

```rust
use dashmap::DashMap;
use std::time::Instant;
```

Puis ajouter les deux fonctions privées juste avant la fonction `dispatch_order` :

```rust
fn is_online(heartbeats: &DashMap<String, Instant>, station_id: &str, timeout_secs: u64) -> bool {
    match heartbeats.get(station_id) {
        None => true,
        Some(last_seen) => last_seen.elapsed().as_secs() < timeout_secs,
    }
}

fn resolve_effective_station<'a>(
    primary: &'a Station,
    all_stations: &'a [Station],
    heartbeats: &DashMap<String, Instant>,
    timeout_secs: u64,
) -> Option<&'a Station> {
    if is_online(heartbeats, &primary.id, timeout_secs) {
        return Some(primary);
    }
    let Some(ref fid) = primary.fallback_station_id else {
        tracing::warn!(station_id = %primary.id, "station down, no fallback configured");
        return None;
    };
    match all_stations.iter().find(|s| &s.id == fid) {
        Some(fallback) if is_online(heartbeats, &fallback.id, timeout_secs) => Some(fallback),
        _ => {
            tracing::warn!(
                station_id = %primary.id,
                fallback_id = %fid,
                "station down, fallback also down or not in profile"
            );
            None
        }
    }
}
```

- [ ] **Step 4 — Vérifier que les tests passent**

```bash
cargo test -p kds-engine failover_tests -- --nocapture
```

Résultat attendu : 7 tests passent.

- [ ] **Step 5 — CI locale**

```bash
cargo clippy -p kds-engine --all-targets -- -D warnings -D clippy::pedantic
```

Résultat attendu : zéro warning.

- [ ] **Step 6 — Commit**

```bash
git add kds-engine/src/state_machine.rs
git commit -m "feat(kds-engine): add is_online + resolve_effective_station with tests"
```

---

## Task 3 — dispatch_order + AppState + endpoint heartbeat

**Files:**
- Modify: `kds-engine/src/state_machine.rs` — refactorer dispatch_order (double-pass)
- Modify: `edge-api/src/app.rs` — ajouter champs AppState
- Modify: `edge-api/src/routes/kds.rs` — handler kds_heartbeat
- Modify: `edge-api/src/routes/mod.rs` — enregistrer route + mettre à jour doc
- Modify: `edge-api/src/routes/orders.rs` — mettre à jour appel dispatch_order
- Modify: `edge-api/tests/api.rs` — 2 tests heartbeat

**Interfaces:**
- Consumes:
  - `is_online` et `resolve_effective_station` de Task 2
  - `AppState` de `edge-api/src/app.rs`
- Produces:
  - `dispatch_order(pool, broadcaster, order, heartbeats: &DashMap<String, Instant>, timeout_secs: u64)`
  - `POST /api/v1/kds/heartbeat/:station_id` → 204

- [ ] **Step 1 — Écrire les tests heartbeat dans api.rs**

Dans `edge-api/tests/api.rs`, à la fin du fichier, ajouter :

```rust
// ---------------------------------------------------------------------------
// KDS Heartbeat — POST /api/v1/kds/heartbeat/:station_id
// ---------------------------------------------------------------------------

#[tokio::test]
#[serial]
async fn heartbeat_returns_204() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(empty_request(Method::POST, "/api/v1/kds/heartbeat/grill"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}

#[tokio::test]
#[serial]
async fn heartbeat_unknown_station_returns_204() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(empty_request(
            Method::POST,
            "/api/v1/kds/heartbeat/station-inconnue",
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
}
```

- [ ] **Step 2 — Vérifier que les tests échouent**

```bash
cargo test -p edge-api --test api heartbeat 2>&1 | head -20
```

Résultat attendu : 404 (route non enregistrée) ou erreur de compilation si la signature de `dispatch_order` a changé.

- [ ] **Step 3 — Refactorer dispatch_order dans kds-engine**

Remplacer intégralement la fonction `dispatch_order` dans `kds-engine/src/state_machine.rs` par :

```rust
/// Route une commande vers les stations KDS actives et diffuse les événements SSE.
/// Applique le failover : si une station est hors délai heartbeat, reroute vers `fallback_station_id`.
///
/// # Errors
/// Retourne `KdsError::Database` si une écriture `SQLite` échoue.
pub async fn dispatch_order(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order: &IncomingOrder,
    heartbeats: &DashMap<String, Instant>,
    timeout_secs: u64,
) -> Result<(), KdsError> {
    let profile_id = routing::active_profile_id(pool).await?;
    let rules = routing::routing_rules_for_profile(pool, &profile_id).await?;
    let stations = routing::stations_for_profile(pool, &profile_id).await?;

    let now_ms = now_ms();
    let order_number_short = generate_short_number(pool, now_ms).await?;

    // Phase 1 — résoudre les lignes par station primaire (routing rules)
    let mut station_lines: HashMap<String, Vec<&IncomingLine>> = HashMap::new();
    for line in &order.lines {
        let station_ids =
            routing::resolve_stations(&rules, &line.category, &line.product_id, &line.tags);
        for sid in station_ids {
            station_lines.entry(sid.to_string()).or_default().push(line);
        }
    }

    // Phase 2 — appliquer le failover : accumuler les lignes par station effective
    let mut effective_lines: HashMap<String, Vec<&IncomingLine>> = HashMap::new();
    for station in &stations {
        let Some(lines) = station_lines.get(&station.id) else {
            continue;
        };
        let Some(effective) =
            resolve_effective_station(station, &stations, heartbeats, timeout_secs)
        else {
            continue;
        };
        effective_lines
            .entry(effective.id.clone())
            .or_default()
            .extend(lines.iter().copied());
    }

    // Phase 3 — dispatcher une seule fois par station effective
    for station in &stations {
        let Some(lines) = effective_lines.get(&station.id) else {
            continue;
        };

        insert_kds_order(pool, order, station, &order_number_short, now_ms).await?;
        for line in lines {
            insert_kds_line(pool, order, station, line).await?;
        }

        let thresholds = load_thresholds(pool, &station.id).await?;
        let payload = build_payload(
            order,
            station,
            lines,
            &order_number_short,
            now_ms,
            thresholds,
            &stations,
        );
        broadcaster.send(KdsEvent::OrderNew(payload));

        if matches!(
            station.role,
            StationRole::Preparation | StationRole::Holding
        ) {
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
```

- [ ] **Step 4 — Mettre à jour AppState dans edge-api/src/app.rs**

Ajouter l'import en haut du fichier :

```rust
use dashmap::DashMap;
use std::time::Instant;
```

Remplacer le struct `AppState` par :

```rust
#[derive(Clone, Debug)]
pub struct AppState {
    /// Journal fiscal — point d'entrée unique pour toutes les opérations NF525.
    pub journal: Arc<Journal>,
    /// Pool `SQLite` partagé (direct access pour promotions et autres tables).
    pub db: SqlitePool,
    /// Chemin du répertoire de données local du restaurant.
    pub data_dir: String,
    /// Broadcaster SSE partagé pour les événements KDS.
    pub kds_broadcaster: KdsBroadcaster,
    /// Derniers heartbeats reçus par station KDS (station_id → Instant).
    pub station_heartbeats: Arc<DashMap<String, Instant>>,
    /// Timeout heartbeat en secondes ; station absente = online (safe-default).
    pub kds_heartbeat_timeout_secs: u64,
}
```

Remplacer `AppState::new` par :

```rust
#[must_use]
pub fn new(journal: Journal, db: SqlitePool, data_dir: String) -> Self {
    let kds_heartbeat_timeout_secs = std::env::var("KDS_HEARTBEAT_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(30);
    Self {
        journal: Arc::new(journal),
        db,
        data_dir,
        kds_broadcaster: KdsBroadcaster::new(),
        station_heartbeats: Arc::new(DashMap::new()),
        kds_heartbeat_timeout_secs,
    }
}
```

- [ ] **Step 5 — Ajouter le handler heartbeat dans edge-api/src/routes/kds.rs**

Ajouter après la section `// Stations` (en fin de fichier) :

```rust
// ---------------------------------------------------------------------------
// Heartbeat — POST /api/v1/kds/heartbeat/:station_id
// ---------------------------------------------------------------------------

/// Enregistre un heartbeat de présence pour une station KDS.
/// Utilisé par kds-app pour signaler qu'un écran est connecté.
/// Réponse toujours 204 — aucune validation du station_id.
pub async fn kds_heartbeat(
    Path(station_id): Path<String>,
    State(state): State<AppState>,
) -> StatusCode {
    state
        .station_heartbeats
        .insert(station_id, std::time::Instant::now());
    StatusCode::NO_CONTENT
}
```

- [ ] **Step 6 — Enregistrer la route dans edge-api/src/routes/mod.rs**

Mettre à jour la table des routes dans le doc comment (ajouter la ligne heartbeat) :

```rust
//! | POST    | /api/v1/kds/heartbeat/:station_id   | `kds_heartbeat`      | KDS screen    |
```

Dans `build_router`, après `.route("/api/v1/kds/stations", get(kds::kds_stations))`, ajouter :

```rust
.route(
    "/api/v1/kds/heartbeat/:station_id",
    post(kds::kds_heartbeat),
)
```

- [ ] **Step 7 — Mettre à jour l'appel dispatch_order dans orders.rs**

Localiser dans `edge-api/src/routes/orders.rs` le bloc tokio::spawn (~ligne 686) :

```rust
    let broadcaster = state.kds_broadcaster.clone();
    let db = state.db.clone();
    tokio::spawn(async move {
        if let Err(e) = dispatch_order(&db, &broadcaster, &incoming).await {
            tracing::warn!(error = %e, "KDS dispatch non-bloquant échoué");
        }
    });
```

Remplacer par :

```rust
    let broadcaster = state.kds_broadcaster.clone();
    let db = state.db.clone();
    let heartbeats = state.station_heartbeats.clone();
    let timeout_secs = state.kds_heartbeat_timeout_secs;
    tokio::spawn(async move {
        if let Err(e) =
            dispatch_order(&db, &broadcaster, &incoming, &heartbeats, timeout_secs).await
        {
            tracing::warn!(error = %e, "KDS dispatch non-bloquant échoué");
        }
    });
```

- [ ] **Step 8 — Vérifier la compilation**

```bash
cargo build --workspace 2>&1 | grep -E "^error"
```

Résultat attendu : aucune ligne d'erreur.

- [ ] **Step 9 — Lancer les tests**

```bash
cargo test --workspace 2>&1 | tail -20
```

Résultat attendu : tous les tests passent, y compris les deux nouveaux `heartbeat_*`.

- [ ] **Step 10 — CI locale**

```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic 2>&1 | grep -E "^error"
```

Résultat attendu : aucune ligne d'erreur.

- [ ] **Step 11 — Commit**

```bash
git add kds-engine/src/state_machine.rs \
        edge-api/src/app.rs \
        edge-api/src/routes/kds.rs \
        edge-api/src/routes/mod.rs \
        edge-api/src/routes/orders.rs \
        edge-api/tests/api.rs
git commit -m "feat(kds): heartbeat endpoint + dispatch_order failover (double-pass)"
```

---

## Task 4 — kds-app : heartbeat côté client (TDD)

**Files:**
- Modify: `kds-app/package.json` — ajouter vitest + script test
- Create: `kds-app/vitest.config.ts`
- Modify: `kds-app/src/hooks/useKdsFeed.ts` — extraire `startHeartbeat` + câbler dans `useEffect`
- Create: `kds-app/src/hooks/useKdsFeed.test.ts` — 2 tests

**Interfaces:**
- Produces: `export function startHeartbeat(stationId: string, baseUrl: string): () => void`

- [ ] **Step 1 — Ajouter Vitest dans package.json**

Dans `kds-app/package.json`, dans `"scripts"`, ajouter :

```json
"test": "vitest run",
"test:watch": "vitest"
```

Dans `"devDependencies"`, ajouter :

```json
"vitest": "^2.0",
"jsdom": "^25.0"
```

Puis installer :

```bash
cd kds-app && npm install
```

- [ ] **Step 2 — Créer kds-app/vitest.config.ts**

```typescript
import { defineConfig } from 'vitest/config'

export default defineConfig({
  test: {
    environment: 'jsdom',
    globals: true,
  },
})
```

- [ ] **Step 3 — Écrire les tests dans useKdsFeed.test.ts**

Créer `kds-app/src/hooks/useKdsFeed.test.ts` :

```typescript
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { startHeartbeat } from './useKdsFeed'

const mockFetch = vi.fn().mockResolvedValue(new Response(null, { status: 204 }))
vi.stubGlobal('fetch', mockFetch)

describe('startHeartbeat', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    mockFetch.mockClear()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('sends heartbeat every 10 seconds', () => {
    const stop = startHeartbeat('grill', 'http://localhost:8080')

    expect(mockFetch).not.toHaveBeenCalled()

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(1)
    expect(mockFetch).toHaveBeenCalledWith(
      'http://localhost:8080/api/v1/kds/heartbeat/grill',
      { method: 'POST' }
    )

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(2)

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(3)

    stop()
  })

  it('stops sending after stop() is called', () => {
    const stop = startHeartbeat('grill', 'http://localhost:8080')

    vi.advanceTimersByTime(10_000)
    expect(mockFetch).toHaveBeenCalledTimes(1)

    stop()
    mockFetch.mockClear()

    vi.advanceTimersByTime(30_000)
    expect(mockFetch).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 4 — Vérifier que les tests échouent**

```bash
cd kds-app && npm test 2>&1 | head -20
```

Résultat attendu : `startHeartbeat is not a function` (export manquant).

- [ ] **Step 5 — Extraire startHeartbeat dans useKdsFeed.ts**

Dans `kds-app/src/hooks/useKdsFeed.ts`, ajouter la constante `BASE` est déjà définie au niveau module (`const BASE = import.meta.env.VITE_EDGE_API_URL as string`).

Ajouter la fonction exportée juste après la constante `BASE` :

```typescript
/**
 * Démarre un interval qui signale la présence de l'écran au edge-api toutes les 10 s.
 * Retourne une fonction d'arrêt à appeler au unmount (clearInterval).
 */
export function startHeartbeat(stationId: string, baseUrl: string): () => void {
  const id = setInterval(() => {
    fetch(`${baseUrl}/api/v1/kds/heartbeat/${stationId}`, { method: 'POST' }).catch(() => {})
  }, 10_000)
  return () => clearInterval(id)
}
```

Puis dans `useKdsFeed`, dans `useEffect`, câbler le heartbeat.

Remplacer le `useEffect` existant :

```typescript
  useEffect(() => {
    connect()
    return () => {
      esRef.current?.close()
      if (retryTimer.current !== null) clearTimeout(retryTimer.current)
    }
  }, [connect, stationId])
```

Par :

```typescript
  useEffect(() => {
    connect()
    const stopHeartbeat = startHeartbeat(stationIdRef.current, BASE)
    return () => {
      esRef.current?.close()
      if (retryTimer.current !== null) clearTimeout(retryTimer.current)
      stopHeartbeat()
    }
  }, [connect, stationId])
```

- [ ] **Step 6 — Vérifier que les tests passent**

```bash
cd kds-app && npm test
```

Résultat attendu :

```
✓ startHeartbeat > sends heartbeat every 10 seconds
✓ startHeartbeat > stops sending after stop() is called

Test Files  1 passed (1)
Tests       2 passed (2)
```

- [ ] **Step 7 — Vérifier le build TypeScript**

```bash
cd kds-app && npm run build 2>&1 | tail -10
```

Résultat attendu : build réussi, aucune erreur TS.

- [ ] **Step 8 — Commit**

```bash
cd ..
git add kds-app/package.json kds-app/package-lock.json \
        kds-app/vitest.config.ts \
        kds-app/src/hooks/useKdsFeed.ts \
        kds-app/src/hooks/useKdsFeed.test.ts
git commit -m "feat(kds-app): heartbeat 10s + vitest setup"
```

---

## Vérification finale

```bash
# CI Rust complète
cargo fmt --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic && \
cargo test --workspace && \
cargo build --release

# Tests kds-app
cd kds-app && npm test
```

Résultats attendus : tous les tests Rust passent (≥ 236 au total), 2 tests Vitest passent, build release ok.
