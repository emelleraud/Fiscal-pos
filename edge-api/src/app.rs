//! # app
//!
//! Construction de l'application Axum : état partagé + middlewares + routeur.
//!
//! ## `AppState`
//! L'état partagé entre tous les handlers Axum. Il est clonable (Arc interne)
//! et injecté via `State<AppState>` dans chaque handler.
//!
//! Contient :
//! - `journal` — le moteur fiscal (thread-safe, Arc<Mutex> interne)
//! - `data_dir` — chemin du répertoire de données (menu.json, archives CSV)
//!
//! ## Middlewares appliqués (du plus externe au plus interne)
//! 1. `CorsLayer` — origines LAN autorisées
//! 2. `request_id_middleware` — UUID par requête
//! 3. `TraceLayer` — logs HTTP structurés (tower-http)
//! 4. `TimeoutLayer` — 30 secondes max par requête

use std::sync::Arc;
use std::time::Instant;

use axum::{middleware, Router};
use dashmap::DashMap;
use sqlx::sqlite::SqlitePool;
use tower_http::trace::TraceLayer;

use fiscal_engine::Journal;
use kds_engine::broadcaster::KdsBroadcaster;

use crate::{middleware as mw, routes};

// ---------------------------------------------------------------------------
// État partagé
// ---------------------------------------------------------------------------

/// État partagé injecté dans tous les handlers Axum.
///
/// Clonable à faible coût : les champs coûteux sont derrière `Arc`.
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
    /// Derniers heartbeats reçus par station KDS (`station_id` → `Instant`).
    pub station_heartbeats: Arc<DashMap<String, Instant>>,
    /// Timeout heartbeat en secondes ; station absente = online (safe-default).
    pub kds_heartbeat_timeout_secs: u64,
}

impl AppState {
    /// Crée un nouvel état applicatif.
    ///
    /// # Arguments
    /// * `journal` - Journal fiscal initialisé avec la pool `SQLite`.
    /// * `db` - Pool `SQLite` partagé.
    /// * `data_dir` - Chemin du répertoire de données (menu.json, archives).
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
}

// ---------------------------------------------------------------------------
// Construction de l'application
// ---------------------------------------------------------------------------

/// Construit l'application Axum complète avec middlewares et routes.
///
/// # Arguments
/// * `state` - État applicatif partagé.
///
/// Note : le timeout par requête est géré dans le middleware `request_id_middleware`
/// via `tokio::time::timeout` plutôt que `tower::timeout::TimeoutLayer`, pour
/// éviter les incompatibilités de types avec axum 0.7.
pub fn build_app(state: AppState) -> Router {
    routes::build_router(state)
        .layer(middleware::from_fn(mw::request_id_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(mw::cors_layer())
}
