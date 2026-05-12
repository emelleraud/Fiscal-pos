//! # app
//!
//! Construction de l'application Axum : état partagé + middlewares + routeur.
//!
//! ## AppState
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

use axum::{middleware, Router};
use tower_http::trace::TraceLayer;

use fiscal_engine::Journal;

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
    /// Chemin du répertoire de données local du restaurant.
    pub data_dir: String,
}

impl AppState {
    /// Crée un nouvel état applicatif.
    ///
    /// # Arguments
    /// * `journal` - Journal fiscal initialisé avec la pool SQLite.
    /// * `data_dir` - Chemin du répertoire de données (menu.json, archives).
    #[must_use]
    pub fn new(journal: Journal, data_dir: String) -> Self {
        Self {
            journal: Arc::new(journal),
            data_dir,
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
#[must_use]
pub fn build_app(state: AppState) -> Router {
    routes::build_router(state)
        .layer(middleware::from_fn(mw::request_id_middleware))
        .layer(TraceLayer::new_for_http())
        .layer(mw::cors_layer())
}
