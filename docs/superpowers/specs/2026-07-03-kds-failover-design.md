# Spec — KDS Failover Station (heartbeat-based)
_2026-07-03_

## Contexte

Le champ `fallback_station_id` est défini dans la table `kds_stations` et chargé dans le struct `Station`, mais n'est jamais utilisé dans `dispatch_order`. Cette spec câble le reroutage automatique vers la station de repli lorsqu'une station primaire est détectée hors ligne via un mécanisme de heartbeat HTTP.

## Objectif

Quand une station KDS ne répond plus (aucun heartbeat reçu depuis plus de N secondes), les nouvelles commandes qui lui sont destinées sont reroutées vers sa `fallback_station_id`. Quand la station revient, les nouvelles commandes reprennent vers elle automatiquement. Aucune action rétroactive sur les commandes déjà reroutées.

---

## Architecture & composants

### 1. `edge-api` — `AppState`

Ajouter dans `AppState` :

```rust
station_heartbeats: Arc<DashMap<String, Instant>>,
kds_heartbeat_timeout_secs: u64,
```

- `DashMap` (crate `dashmap`) : concurrent HashMap sans `Mutex` explicite.
- `kds_heartbeat_timeout_secs` : lu depuis `KDS_HEARTBEAT_TIMEOUT_SECS` (env var), défaut **30**.

### 2. `edge-api` — Nouveau endpoint heartbeat

```
POST /api/v1/kds/heartbeat/:station_id
```

- Handler : `heartbeats.insert(station_id, Instant::now())`
- Réponse : **204 No Content**
- Aucune validation du `station_id` (un app qui redémarre avant la config peut envoyer un heartbeat inconnu — accepté silencieusement).

### 3. `kds-engine` — `state_machine::dispatch_order`

Signature modifiée :

```rust
pub async fn dispatch_order(
    pool: &SqlitePool,
    broadcaster: &KdsBroadcaster,
    order: &IncomingOrder,
    heartbeats: &DashMap<String, Instant>,
    timeout_secs: u64,
) -> Result<(), KdsError>
```

Logique de résolution pour chaque station cible :

```
effective_station = resolve_effective_station(station, &stations, heartbeats, timeout_secs)
```

### 4. `kds-engine` — Nouvelle fonction `resolve_effective_station`

```rust
fn resolve_effective_station<'a>(
    primary: &'a Station,
    all_stations: &'a [Station],
    heartbeats: &DashMap<String, Instant>,
    timeout_secs: u64,
) -> Option<&'a Station>
```

Règles :
1. Si `is_online(heartbeats, &primary.id, timeout_secs)` → retourner `Some(primary)`
2. Sinon, si `primary.fallback_station_id` est `Some(fid)` :
   - Chercher la station `fid` dans `all_stations`
   - Si trouvée ET online → retourner `Some(fallback)`
   - Sinon → `warn!("station {fid} fallback also down or not in profile")` + retourner `None`
3. Si pas de `fallback_station_id` → `warn!("station {} down, no fallback configured", primary.id)` + retourner `None`

Un seul niveau de repli. Pas de chaîne récursive.

### 5. `kds-engine` — Fonction `is_online`

```rust
fn is_online(heartbeats: &DashMap<String, Instant>, station_id: &str, timeout_secs: u64) -> bool {
    match heartbeats.get(station_id) {
        None => true, // absent = online (safe default au démarrage)
        Some(last_seen) => last_seen.elapsed().as_secs() < timeout_secs,
    }
}
```

### 6. `kds-app` — `useKdsFeed.ts`

Ajouter dans `useEffect` un interval de 10 s :

```typescript
const hbInterval = setInterval(() => {
  fetch(`${EDGE_API_URL}/api/v1/kds/heartbeat/${stationId}`, { method: 'POST' })
    .catch(() => {}); // silencieux — perte réseau temporaire non bloquante
}, 10_000);

return () => {
  clearInterval(hbInterval);
  // ... cleanup existant (EventSource.close())
};
```

---

## Flux de données

### Heartbeat

```
useKdsFeed setInterval 10 s
  └─ POST /api/v1/kds/heartbeat/:stationId
       └─ heartbeats.insert(station_id, Instant::now())
            └─ 204 No Content
```

### Dispatch avec failover

```
POST /orders/:id/pay
  └─ dispatch_order(pool, broadcaster, order, &heartbeats, timeout_secs)
       └─ Pour chaque station cible :
            ├─ resolve_effective_station(primary, all_stations, heartbeats, timeout_secs)
            │    Some(s) → dispatch + impression sur s
            │    None    → warn! + skip (commande non visible sur cette station)
            └─ Les autres stations (non concernées par la ligne) ne sont pas affectées
```

### Recovery

Dès que le heartbeat reprend, `last_seen` se met à jour. Au prochain dispatch, la station repasse online automatiquement. Les commandes déjà reroutées vers le repli restent sur le repli jusqu'à bump (`served`).

---

## Cas limites

| Cas | Comportement |
|---|---|
| Station absente de la map (démarrage) | `is_online` → `true`, dispatch normal |
| Station primaire down, pas de fallback | `warn!` + skip |
| Station de repli down | `warn!` + skip (un seul niveau) |
| Station de repli absente du profil actif | Non dans `all_stations` → `warn!` + skip |
| `KDS_HEARTBEAT_TIMEOUT_SECS` non défini | Défaut = 30 s |
| kds-app perd le réseau < 30 s | Fenêtre de grâce → station toujours online |
| Heartbeat pour `station_id` inconnu | Inséré dans la map, ignoré au dispatch si non dans le profil |
| edge-api redémarre | Map vide → toutes stations online (safe-default) |

Philosophie : **warn et continuer, jamais bloquer** — identique au pull KDS dans sync-client.

---

## Tests

### kds-engine — unitaires

| Test | Ce qu'il vérifie |
|---|---|
| `dispatch_routes_to_fallback_when_primary_down` | Primaire timeout → insertion sur `fallback_id` |
| `dispatch_uses_primary_when_online` | Primaire récente → comportement actuel inchangé |
| `dispatch_skips_when_no_fallback` | Down + `fallback_station_id = None` → skip, pas de panic |
| `dispatch_skips_when_fallback_also_down` | Repli aussi down → skip, pas d'erreur propagée |
| `is_online_absent_means_online` | Station absente de la map → `true` |
| `is_online_respects_timeout` | `last_seen = now - 31s`, timeout = 30 → `false` |

### edge-api — intégration

| Test | Ce qu'il vérifie |
|---|---|
| `heartbeat_returns_204` | `POST /kds/heartbeat/grill` → 204 |
| `heartbeat_updates_map` | Après POST, map contient l'entrée mise à jour |

### kds-app — Vitest

| Test | Ce qu'il vérifie |
|---|---|
| `sends_heartbeat_every_10s` | `vi.useFakeTimers()` — fetch appelé à 10 s, 20 s, 30 s |
| `cleanup_clears_interval` | Unmount → interval annulé, plus de fetch |

---

## Dépendances

- Crate `dashmap` à ajouter dans `edge-api/Cargo.toml` et `kds-engine/Cargo.toml`.
- Pas de nouvelle migration Supabase.
- Pas de changement du schéma SQLite local.
