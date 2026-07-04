# Design — kds-app servi par edge-api via ServeDir

**Date :** 2026-07-04  
**Statut :** Approuvé  
**Approche retenue :** A — `nest_service` dans `build_router`

---

## Contexte

`kds-app/` est une SPA React 19 (Vite) compilée dans `kds-app/dist/` (`index.html` + `assets/`).  
`edge-api` (Axum, port 8080) expose toutes les routes `/api/v1/...`.

**Objectif :** edge-api sert le bundle statique de kds-app sous le préfixe `/kds/` afin qu'un écran KDS n'ait besoin que de `http://<ip-restaurant>:8080/kds/` sans dépendance à un serveur Vite séparé.

Le routing interne de kds-app repose sur `window.location.pathname` (pas de React Router) — le serveur doit renvoyer `index.html` pour toute requête non-asset (fallback SPA).

---

## Architecture

Aucune nouvelle infrastructure. `ServeDir` (tower-http) est monté directement dans le `Router` Axum existant. Le chemin du dist est configuré par env var `KDS_APP_DIST`.

```
Navigateur KDS
    │  GET /kds/*
    ▼
edge-api :8080  (Axum)
    ├─ /api/v1/**   → handlers existants (inchangés)
    └─ /kds/**      → ServeDir(kds-app/dist/)
                        ├─ asset connu  → fichier statique
                        └─ route inconnue → fallback index.html
```

---

## Changements

### 1. `Cargo.toml` (workspace root)

Ajouter la feature `"fs"` à tower-http :

```toml
tower-http = { version = "0.6", features = ["trace", "cors", "fs"] }
```

### 2. `kds-app/vite.config.ts`

Définir le base path `/kds/` pour que les imports d'assets soient préfixés correctement :

```ts
export default defineConfig({
  plugins: [react()],
  base: '/kds/',
})
```

Rebuild après modification :

```bash
cd kds-app && npm run build
```

### 3. `edge-api/src/main.rs`

Ajouter `kds_app_dist` à `Config` :

```rust
struct Config {
    host: String,
    port: u16,
    database_url: String,
    data_dir: String,
    log_json: bool,
    kds_app_dist: String,   // nouveau
}

impl Config {
    fn from_env() -> Self {
        Self {
            // ...champs existants...
            kds_app_dist: std::env::var("KDS_APP_DIST")
                .unwrap_or_else(|_| "./kds-app/dist".to_string()),
        }
    }
}
```

Passer `kds_app_dist` à `build_app` :

```rust
let app = build_app(state, config.kds_app_dist);
```

### 4. `edge-api/src/app.rs`

Étendre la signature de `build_app` et transmettre `kds_dist` à `build_router` :

```rust
pub fn build_app(state: AppState, kds_dist: String) -> Router {
    routes::build_router(state, kds_dist)
        .layer(middleware::from_fn(mw::request_id_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(mw::cors_layer())
}
```

### 5. `edge-api/src/routes/mod.rs`

Étendre la signature de `build_router`, importer les services tower-http, monter ServeDir :

```rust
use tower_http::services::{ServeDir, ServeFile};

pub fn build_router(state: AppState, kds_dist: String) -> Router {
    Router::new()
        // --- Health ---
        .route("/api/v1/health", get(health::health_handler))
        // ... toutes les routes API existantes inchangées ...
        // --- KDS static SPA ---
        .nest_service("/kds", {
            let index = format!("{kds_dist}/index.html");
            ServeDir::new(kds_dist).fallback(ServeFile::new(index))
        })
        .with_state(state)
}
```

Mettre à jour le doc-comment (table des routes) pour inclure `GET /kds/*`.

### 6. Tests edge-api existants

Les tests d'intégration qui appellent `build_router` directement devront passer un `kds_dist` factice (ex. `"./kds-app/dist"` ou `"/tmp"`). Aucun test ne teste les routes `/kds/` — les 44 tests existants continuent de valider uniquement l'API.

---

## Variables d'environnement

| Variable | Défaut | Description |
|---|---|---|
| `KDS_APP_DIST` | `./kds-app/dist` | Chemin vers le build Vite de kds-app |

---

## Validation

### Tests Rust
```bash
cargo test --workspace
```
Les 44 tests existants doivent passer sans régression.

### Clippy pedantic
```bash
cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic
```

### Smoke test manuel
| Requête | Résultat attendu |
|---|---|
| `GET /kds/` | `200` + `index.html` |
| `GET /kds/assets/index-*.js` | `200` + JS bundle |
| `GET /kds/une-route-spa` | `200` + `index.html` (fallback) |
| `GET /api/v1/health` | `200` inchangé |

---

## Contraintes permanentes

- `#![deny(clippy::all, clippy::pedantic)]` — aucun warning toléré
- Ne jamais toucher `fiscal-engine` / `HashInput` / journal append-only
- Ne pas committer `SUPABASE_SERVICE_KEY` ni `FISCAL_SIGNING_KEY_HEX`
