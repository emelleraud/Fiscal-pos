#![deny(clippy::all, clippy::pedantic)]

use axum::{body::Bytes, extract::State, http::StatusCode, routing::post, Router};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

#[derive(Clone)]
struct AgentState {
    device_path: Arc<String>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let device_path =
        std::env::var("PRINTER_DEVICE").unwrap_or_else(|_| "/dev/usb/lp0".to_string());
    let port: u16 = std::env::var("AGENT_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(6611);

    let state = AgentState {
        device_path: Arc::new(device_path.clone()),
    };

    let app = Router::new()
        .route("/print", post(handle_print))
        .with_state(state);

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    tracing::info!(%addr, %device_path, "kds-print-agent démarré");

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind failed");
    axum::serve(listener, app).await.expect("serve failed");
}

async fn handle_print(
    State(state): State<AgentState>,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    write_to_device(&state.device_path, &body)
        .await
        .map(|()| StatusCode::NO_CONTENT)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))
}

async fn write_to_device(path: &str, data: &[u8]) -> std::io::Result<()> {
    let mut file = tokio::fs::OpenOptions::new().write(true).open(path).await?;
    file.write_all(data).await?;
    file.flush().await
}
