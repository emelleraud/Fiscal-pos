# kds-app ServeDir Integration — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Servir le bundle statique de kds-app depuis edge-api sous `/kds/` avec fallback SPA, sans serveur Vite séparé.

**Architecture:** `ServeDir` (tower-http) monté via `nest_service("/kds", ...)` dans le `Router` Axum existant. Le chemin du dist est lu depuis `KDS_APP_DIST` (défaut `./kds-app/dist`) via `Config::from_env()` et transmis en paramètre à travers `build_app → build_router`.

**Tech Stack:** Rust/Axum 0.7, tower-http 0.6 (`ServeDir`, `ServeFile`), Vite 8 (React 19 SPA)

## Global Constraints

- `#![deny(clippy::all, clippy::pedantic)]` — zéro warning toléré sur tout le workspace
- Ne jamais toucher `fiscal-engine` / `HashInput` / journal append-only
- Ne pas committer `SUPABASE_SERVICE_KEY` ni `FISCAL_SIGNING_KEY_HEX`
- Tests E2E sync (`--ignored`) exclus du scope — seuls les 44 tests cargo sans `--ignored`
- Vite base path doit être `/kds/` (avec slash final) — obligatoire pour les imports d'assets

---

## File Map

| Fichier | Action | Rôle |
|---|---|---|
| `Cargo.toml` | Modifier ligne 39 | Ajouter feature `"fs"` à tower-http |
| `kds-app/vite.config.ts` | Modifier | Ajouter `base: '/kds/'` |
| `edge-api/src/main.rs` | Modifier | Ajouter `kds_app_dist` à `Config`, mettre à jour l'appel `build_app` |
| `edge-api/src/app.rs` | Modifier | Étendre signature `build_app(state, kds_dist: String)` |
| `edge-api/src/routes/mod.rs` | Modifier | Étendre signature `build_router`, ajouter imports + `nest_service`, mettre à jour doc-comment |
| `edge-api/tests/api.rs` | Modifier | Mettre à jour `setup()`, ajouter `setup_with_kds()` + 2 nouveaux tests |

---

## Task 1: tower-http `fs` feature + Vite base path

**Files:**
- Modify: `Cargo.toml:39`
- Modify: `kds-app/vite.config.ts`

**Interfaces:**
- Produces: `tower_http::services::{ServeDir, ServeFile}` disponible à la compilation edge-api ; `kds-app/dist/index.html` avec tous les imports préfixés `/kds/assets/`

- [ ] **Step 1 : Modifier `Cargo.toml` — ajouter `"fs"` à tower-http**

  Ligne 39, remplacer :
  ```toml
  tower-http = { version = "0.6", features = ["trace", "cors"] }
  ```
  par :
  ```toml
  tower-http = { version = "0.6", features = ["trace", "cors", "fs"] }
  ```

- [ ] **Step 2 : Vérifier que la feature est disponible**

  ```bash
  cargo check -p edge-api
  ```
  Attendu : compilation OK (pas encore de code qui utilise `fs`, mais la feature est déclarée).

- [ ] **Step 3 : Modifier `kds-app/vite.config.ts` — ajouter le base path**

  Remplacer le contenu entier par :
  ```ts
  import { defineConfig } from 'vite'
  import react from '@vitejs/plugin-react'

  export default defineConfig({
    plugins: [react()],
    base: '/kds/',
  })
  ```

- [ ] **Step 4 : Rebuilder kds-app**

  ```bash
  cd kds-app && npm run build
  ```
  Attendu : `dist/` régénéré sans erreur.

- [ ] **Step 5 : Vérifier que les imports sont préfixés `/kds/`**

  ```bash
  grep -o 'src="/kds/\|href="/kds/' kds-app/dist/index.html | head -5
  ```
  Attendu : au moins une occurrence `/kds/` dans le HTML (les scripts et styles).

  Vérification alternative :
  ```bash
  grep '/kds/assets' kds-app/dist/index.html
  ```
  Attendu : lines référençant `/kds/assets/index-*.js` et `/kds/assets/index-*.css`.

- [ ] **Step 6 : Commit**

  ```bash
  git add Cargo.toml kds-app/vite.config.ts kds-app/dist/
  git commit -m "feat(kds): add tower-http fs feature + vite base path /kds/"
  ```

---

## Task 2: Étendre signatures Rust + monter ServeDir (TDD)

**Files:**
- Modify: `edge-api/tests/api.rs` — `setup()` + `setup_with_kds()` + 2 tests
- Modify: `edge-api/src/routes/mod.rs` — signature + imports + `nest_service` + doc-comment
- Modify: `edge-api/src/app.rs` — signature `build_app`
- Modify: `edge-api/src/main.rs` — `Config` + appel `build_app`

**Interfaces:**
- Consumes: `tower_http::services::{ServeDir, ServeFile}` (Task 1)
- Produces:
  - `pub fn build_app(state: AppState, kds_dist: String) -> Router`
  - `pub fn build_router(state: AppState, kds_dist: String) -> Router`
  - `Config { kds_app_dist: String, ... }`

- [ ] **Step 1 : Écrire les tests en échec — `edge-api/tests/api.rs`**

  **1a.** Mettre à jour `setup()` : changer la ligne `(build_app(state), db_file)` (ligne 41) :
  ```rust
  (build_app(state, "./kds-app/dist".to_string()), db_file)
  ```

  **1b.** Ajouter le helper `setup_with_kds()` juste après la fonction `setup()` :
  ```rust
  async fn setup_with_kds() -> (axum::Router, NamedTempFile, tempfile::TempDir) {
      let db_file = NamedTempFile::new().expect("tempfile SQLite");
      let db_path = db_file.path().to_str().unwrap().to_string();

      let pool = SqlitePoolOptions::new()
          .max_connections(5)
          .connect(&format!("sqlite:{db_path}"))
          .await
          .expect("pool SQLite");

      let journal = Journal::open(pool.clone()).await.expect("journal");
      let state = AppState::new(journal, pool, "/tmp".to_string());

      let kds_dir = tempfile::TempDir::new().expect("tempdir kds");
      std::fs::write(kds_dir.path().join("index.html"), "<html>KDS</html>")
          .expect("écriture index.html");
      let kds_dist = kds_dir.path().to_str().unwrap().to_string();

      (build_app(state, kds_dist), db_file, kds_dir)
  }
  ```

  **1c.** Ajouter une section et deux nouveaux tests à la fin du fichier :
  ```rust
  // ---------------------------------------------------------------------------
  // KDS static SPA — GET /kds/*
  // ---------------------------------------------------------------------------

  #[tokio::test]
  async fn kds_root_returns_200() {
      let (app, _db, _kds_dir) = setup_with_kds().await;
      let resp = app
          .oneshot(empty_request(Method::GET, "/kds/"))
          .await
          .unwrap();
      assert_eq!(resp.status(), StatusCode::OK);
  }

  #[tokio::test]
  async fn kds_spa_fallback_returns_index() {
      let (app, _db, _kds_dir) = setup_with_kds().await;
      let resp = app
          .oneshot(empty_request(Method::GET, "/kds/une-route-inconnue"))
          .await
          .unwrap();
      assert_eq!(resp.status(), StatusCode::OK);
  }
  ```

- [ ] **Step 2 : Vérifier que le code ne compile pas encore (signatures incorrectes)**

  ```bash
  cargo test -p edge-api 2>&1 | grep "error\[" | head -5
  ```
  Attendu : erreur de compilation — `build_app` attend 1 argument, reçoit 2 / route `/kds/` inexistante.

- [ ] **Step 3 : Mettre à jour `edge-api/src/routes/mod.rs`**

  **3a.** Remplacer le bloc d'imports existant (lignes 33–38) :
  ```rust
  use axum::{
      routing::{get, post},
      Router,
  };
  use std::path::Path;
  use tower_http::services::{ServeDir, ServeFile};

  use crate::app::AppState;
  ```

  **3b.** Mettre à jour la table des routes dans le doc-comment (ajouter la dernière ligne) :
  ```rust
  //! | GET     | `/kds/*`                              | ServeDir (kds-app SPA) | KDS screen     |
  ```

  **3c.** Changer la signature de `build_router` et ajouter `nest_service` :
  ```rust
  pub fn build_router(state: AppState, kds_dist: String) -> Router {
      Router::new()
          // --- Health ---
          .route("/api/v1/health", get(health::health_handler))
          // --- Menu ---
          .route("/api/v1/menu", get(menu::menu_handler))
          // --- Sessions ---
          .route(
              "/api/v1/sessions/current",
              get(sessions::get_current_session_handler),
          )
          .route(
              "/api/v1/sessions/open",
              post(sessions::open_session_handler),
          )
          .route(
              "/api/v1/sessions/close",
              post(sessions::close_session_handler),
          )
          // --- Commandes ---
          .route("/api/v1/orders", post(orders::create_order_handler))
          .route("/api/v1/orders/:id", get(orders::get_order_handler))
          .route("/api/v1/orders/:id/pay", post(orders::pay_order_handler))
          .route(
              "/api/v1/orders/:id/cancel",
              post(orders::cancel_order_handler),
          )
          // --- Promotions ---
          .route(
              "/api/v1/promotions/available",
              get(promotions::get_available_promotions),
          )
          // --- Archive annuelle NF525 §7 ---
          .route(
              "/api/v1/archive/:year",
              post(archive::generate_archive_handler),
          )
          // --- KDS (Kitchen Display System) ---
          // IMPORTANT : ready_board DOIT précéder :station_id (route littérale avant route paramétrée)
          .route("/api/v1/kds/feed/ready_board", get(kds::kds_ready_board))
          .route("/api/v1/kds/feed/:station_id", get(kds::kds_feed))
          .route("/api/v1/kds/orders/:order_id/ack", post(kds::kds_ack))
          .route("/api/v1/kds/orders/:order_id/served", post(kds::kds_served))
          .route(
              "/api/v1/kds/config",
              get(kds::kds_get_config).put(kds::kds_set_config),
          )
          .route("/api/v1/kds/stations", get(kds::kds_stations))
          .route(
              "/api/v1/kds/heartbeat/:station_id",
              post(kds::kds_heartbeat),
          )
          // --- kds-app SPA statique ---
          .nest_service("/kds", {
              let index_html = Path::new(&kds_dist).join("index.html");
              ServeDir::new(kds_dist).fallback(ServeFile::new(index_html))
          })
          .with_state(state)
  }
  ```

- [ ] **Step 4 : Mettre à jour `edge-api/src/app.rs`**

  Changer la signature de `build_app` (ligne 91) :
  ```rust
  pub fn build_app(state: AppState, kds_dist: String) -> Router {
      routes::build_router(state, kds_dist)
          .layer(middleware::from_fn(mw::request_id_middleware))
          .layer(TraceLayer::new_for_http())
          .layer(mw::cors_layer())
  }
  ```

- [ ] **Step 5 : Mettre à jour `edge-api/src/main.rs`**

  **5a.** Ajouter `kds_app_dist` à la struct `Config` (après `log_json`) :
  ```rust
  #[derive(Debug)]
  struct Config {
      host: String,
      port: u16,
      database_url: String,
      data_dir: String,
      log_json: bool,
      kds_app_dist: String,
  }
  ```

  **5b.** Ajouter le champ dans `Config::from_env()` (après `log_json`) :
  ```rust
  kds_app_dist: std::env::var("KDS_APP_DIST")
      .unwrap_or_else(|_| "./kds-app/dist".to_string()),
  ```

  **5c.** Mettre à jour l'appel `build_app` dans `main()` (ligne ~101) :
  ```rust
  let app = build_app(state, config.kds_app_dist);
  ```

  Note : `config.kds_app_dist` est déplacé (move) ici. `config.host` et `config.port`, utilisés ensuite dans le `format!`, restent accessibles (move partiel de champ — Rust autorisé).

- [ ] **Step 6 : Lancer les tests edge-api — vérifier que tous passent**

  ```bash
  cargo test -p edge-api -- --nocapture 2>&1 | tail -10
  ```
  Attendu :
  ```
  test result: ok. 46 passed; 0 failed; ...
  ```
  (44 existants + 2 nouveaux = 46)

- [ ] **Step 7 : Lancer les tests du workspace complet**

  ```bash
  cargo test --workspace
  ```
  Attendu : tous les tests passent, 0 failed.

- [ ] **Step 8 : Commit**

  ```bash
  git add edge-api/src/routes/mod.rs edge-api/src/app.rs edge-api/src/main.rs edge-api/tests/api.rs
  git commit -m "feat(edge-api): serve kds-app SPA via ServeDir on /kds/"
  ```

---

## Task 3: Clippy pedantic + smoke test

**Files:**
- Modify: tout fichier Rust signalé par clippy (si besoin)

**Interfaces:**
- Consumes: workspace compilable (Task 2)

- [ ] **Step 1 : Lancer Clippy pedantic sur tout le workspace**

  ```bash
  cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::pedantic
  ```
  Attendu : aucun warning, exit 0.

  Si des warnings apparaissent, les corriger avant de continuer. Patterns courants dans ce projet : voir `docs/superpowers/` pour les solutions validées.

- [ ] **Step 2 : Smoke test — démarrer edge-api et vérifier les routes**

  Dans un terminal :
  ```bash
  DATABASE_URL=sqlite:./restaurant.db DATA_DIR=./data cargo run -p edge-api
  ```

  Dans un autre terminal :
  ```bash
  # Route API inchangée
  curl -s http://localhost:8080/api/v1/health | python3 -m json.tool
  # Attendu : {"status":"ok","database":"connected"}

  # SPA root
  curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/kds/
  # Attendu : 200

  # Asset JS
  curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/kds/assets/$(ls kds-app/dist/assets/*.js | head -1 | xargs basename)
  # Attendu : 200

  # Fallback SPA (route inconnue → index.html)
  curl -s -o /dev/null -w "%{http_code}" http://localhost:8080/kds/une-route-spa
  # Attendu : 200
  ```

- [ ] **Step 3 : Commit si des corrections clippy ont été nécessaires**

  ```bash
  git add -p
  git commit -m "fix(edge-api): clippy pedantic — kds ServeDir"
  ```
  (Sauter cette étape si Step 1 était propre.)

---

## Résumé des changements attendus

| Cible | Avant | Après |
|---|---|---|
| `tower-http` features | `["trace", "cors"]` | `["trace", "cors", "fs"]` |
| `vite base` | *(absent)* | `'/kds/'` |
| `build_app` signature | `(state: AppState)` | `(state: AppState, kds_dist: String)` |
| `build_router` signature | `(state: AppState)` | `(state: AppState, kds_dist: String)` |
| Route `/kds/*` | 404 | `ServeDir` avec fallback `index.html` |
| Tests Rust | 44 | 46 (+2 kds) |
| Env var `KDS_APP_DIST` | *(inconnue)* | Lue par `Config::from_env()`, défaut `./kds-app/dist` |
