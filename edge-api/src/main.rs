//! # edge-api
//!
//! Serveur HTTP local du restaurant — exposé sur le réseau LAN uniquement.
//!
//! ## Démarrage
//! ```bash
//! DATABASE_URL=sqlite:./restaurant.db \
//! EDGE_API_HOST=0.0.0.0 \
//! EDGE_API_PORT=8080 \
//! RUST_LOG=edge_api=info,fiscal_engine=debug \
//! cargo run --release --bin edge-api
//! ```

#![deny(clippy::all, clippy::pedantic)]
#![warn(missing_docs)]

mod app;
mod error;
mod middleware;
mod routes;

use std::net::SocketAddr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use tracing::{error, info};
use tracing_subscriber::EnvFilter;

use app::{build_app, AppState};
use fiscal_engine::Journal;

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    database_url: String,
    data_dir: String,
    log_json: bool,
}

impl Config {
    fn from_env() -> Self {
        Self {
            host: std::env::var("EDGE_API_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: std::env::var("EDGE_API_PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("EDGE_API_PORT doit être un entier"),
            database_url: std::env::var("DATABASE_URL")
                .unwrap_or_else(|_| "sqlite:./restaurant.db".to_string()),
            data_dir: std::env::var("DATA_DIR").unwrap_or_else(|_| "./data".to_string()),
            log_json: std::env::var("RUST_LOG_FORMAT").as_deref() == Ok("json"),
        }
    }
}

#[tokio::main]
async fn main() {
    let config = Config::from_env();

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("edge_api=info,fiscal_engine=info,tower_http=debug"));

    if config.log_json {
        tracing_subscriber::fmt()
            .json()
            .with_env_filter(filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .pretty()
            .with_env_filter(filter)
            .init();
    }

    info!(host = %config.host, port = config.port, "Démarrage edge-api");

    let pool = match open_database(&config.database_url).await {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Impossible d'ouvrir SQLite");
            std::process::exit(1);
        }
    };

    let db = pool.clone();
    let journal = match Journal::open(pool).await {
        Ok(j) => j,
        Err(e) => {
            error!(error = %e, "Impossible d'initialiser le journal fiscal");
            std::process::exit(1);
        }
    };

    if let Some(session) = journal.active_session().await {
        info!(session_id = %session.id, "Session active restaurée");
    } else {
        info!("Aucune session active au démarrage");
    }

    let state = AppState::new(journal, db, config.data_dir.clone());
    let app = build_app(state);

    let addr: SocketAddr = format!("{}:{}", config.host, config.port)
        .parse()
        .expect("Adresse invalide");

    info!(addr = %addr, "Serveur en écoute");

    let listener = match tokio::net::TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            error!(error = %e, "Impossible de lier le port");
            std::process::exit(1);
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Erreur serveur Axum");

    info!("edge-api arrêté");
}

async fn open_database(database_url: &str) -> Result<SqlitePool, sqlx::Error> {
    let path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .synchronous(sqlx::sqlite::SqliteSynchronous::Full)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(4)
        .min_connections(1)
        .connect_with(options)
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.expect("Handler Ctrl+C");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("Handler SIGTERM")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => { info!("Ctrl+C reçu"); }
        () = terminate => { info!("SIGTERM reçu"); }
    }
}
