//! # sync-client
//!
//! Agent de synchronisation offline-first : pousse les entrées fiscales
//! non synchronisées vers Supabase et reçoit les configurations mises à jour.
//!
//! ## Démarrage
//! ```bash
//! DATABASE_URL=sqlite:./restaurant.db \
//! SUPABASE_URL=https://xxxx.supabase.co \
//! SUPABASE_SERVICE_KEY=eyJ... \
//! SITE_ID=12345678900000 \
//! SYNC_INTERVAL_SECONDS=30 \
//! RUST_LOG=sync_client=info \
//! cargo run --release --bin sync-client
//! ```
//!
//! ## Garanties
//! - Jamais de perte de données fiscales : la queue SQLite est persistée
//! - Jamais de blocage de la caisse : sync en arrière-plan indépendant
//! - Arrêt propre sur SIGTERM / Ctrl+C

#![deny(clippy::all, clippy::pedantic)]
#![warn(missing_docs)]

mod client;
mod config;
mod config_puller;
mod error;
mod serializer;
mod sync_loop;

use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

use crate::{
    client::SupabaseClient,
    config::SyncConfig,
    sync_loop::run_sync_cycle,
};
use fiscal_engine::journal::store::JournalStore;

#[tokio::main]
async fn main() {
    // 1. Logging
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("sync_client=info"));

    let log_json = std::env::var("RUST_LOG_FORMAT").as_deref() == Ok("json");
    if log_json {
        tracing_subscriber::fmt().json().with_env_filter(filter).init();
    } else {
        tracing_subscriber::fmt().pretty().with_env_filter(filter).init();
    }

    // 2. Configuration
    let config = match SyncConfig::from_env() {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Configuration invalide — arrêt");
            std::process::exit(1);
        }
    };

    info!(
        site_id = %config.site_id,
        interval_secs = config.sync_interval_secs,
        batch_size = config.batch_size,
        "sync-client démarré"
    );

    // 3. Pool SQLite (connexion à la base du restaurant)
    let pool = match open_database(&config.database_url).await {
        Ok(p) => p,
        Err(e) => {
            error!(error = %e, "Impossible d'ouvrir SQLite");
            std::process::exit(1);
        }
    };

    let store = JournalStore { pool };

    // 4. Client Supabase
    let client = match SupabaseClient::new(&config) {
        Ok(c) => c,
        Err(e) => {
            error!(error = %e, "Impossible de créer le client Supabase");
            std::process::exit(1);
        }
    };

    // 5. Boucle de synchronisation avec arrêt propre
    let interval = std::time::Duration::from_secs(config.sync_interval_secs);

    loop {
        tokio::select! {
            () = tokio::time::sleep(interval) => {
                match run_sync_cycle(&store, &client, &config).await {
                    Ok(metrics) if metrics.was_offline => {
                        warn!("Cycle ignoré — Supabase inaccessible (mode offline)");
                    }
                    Ok(metrics) => {
                        info!(
                            pushed = metrics.entries_pushed,
                            duration_ms = metrics.duration_ms,
                            "Cycle terminé"
                        );
                    }
                    Err(e) => {
                        error!(error = %e, "Erreur de cycle — sera réessayé au prochain cycle");
                    }
                }
            }
            () = shutdown_signal() => {
                info!("Signal d'arrêt reçu — sync-client terminé proprement");
                break;
            }
        }
    }
}

async fn open_database(database_url: &str) -> Result<sqlx::sqlite::SqlitePool, sqlx::Error> {
    let path = database_url.strip_prefix("sqlite:").unwrap_or(database_url);
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false) // La base doit exister (créée par edge-api)
        .read_only(false)         // Besoin de UPDATE synced=1
        .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
        .foreign_keys(true);

    SqlitePoolOptions::new()
        .max_connections(2) // sync-client n'a pas besoin de beaucoup de connexions
        .min_connections(1)
        .connect_with(options)
        .await
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Handler Ctrl+C");
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
        () = ctrl_c => {}
        () = terminate => {}
    }
}
