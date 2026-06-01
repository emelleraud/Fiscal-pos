//! # middleware
//!
//! Middlewares Axum pour l'`edge-api`.
//!
//! ## Middlewares appliqués (dans l'ordre, du plus externe au plus interne)
//!
//! 1. **`RequestId`** — génère un UUID v4 par requête, propagé dans les logs
//! 2. **`TraceLayer`** — log structuré de chaque requête HTTP (tower-http)
//! 3. **`TimeoutLayer`** — timeout global de 30 secondes (protection contre les hangs)
//! 4. **`CorsLayer`** — CORS restreint au réseau LAN (origines locales uniquement)
//!
//! ## Logging structuré
//! Chaque requête produit deux événements tracing :
//! - `request` au début : méthode, URI, request-id
//! - `response` à la fin : status HTTP, durée en ms
//!
//! Format JSON activé en production (`RUST_LOG_FORMAT=json`).

use axum::{
    body::Body,
    http::{HeaderName, HeaderValue, Request},
    middleware::Next,
    response::Response,
};
use std::time::Instant;
use tower_http::cors::CorsLayer;
use tracing::{info, info_span, Instrument};
use uuid::Uuid;

/// Header HTTP transportant l'identifiant de requête.
pub static X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// Middleware : injecte un `x-request-id` unique dans chaque requête et réponse.
///
/// Si le client envoie déjà un `x-request-id`, il est conservé (tracing distribué).
/// Sinon, un UUID v4 est généré.
pub async fn request_id_middleware(
    mut req: Request<Body>,
    next: Next,
) -> Response {
    let request_id = req
        .headers()
        .get(&X_REQUEST_ID)
        .and_then(|v| v.to_str().ok()).map_or_else(|| Uuid::now_v7().to_string(), ToString::to_string);

    // Injecter dans les extensions pour que les handlers puissent le lire
    req.extensions_mut().insert(RequestId(request_id.clone()));

    let method = req.method().clone();
    let uri = req.uri().clone();
    let start = Instant::now();

    let span = info_span!(
        "http_request",
        method = %method,
        uri = %uri,
        request_id = %request_id,
    );

    let response = next.run(req).instrument(span.clone()).await;

    let duration_ms = start.elapsed().as_millis();
    let status = response.status().as_u16();

    // Log structuré de la réponse dans le même span
    span.in_scope(|| {
        info!(
            status = status,
            duration_ms = duration_ms,
            "Réponse envoyée"
        );
    });

    // Propager le request-id dans la réponse pour le frontend
    let mut response = response;
    if let Ok(val) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(X_REQUEST_ID.clone(), val);
    }

    response
}

/// Extension Axum contenant le request-id de la requête courante.
///
/// Accessible dans les handlers via `Extension<RequestId>`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RequestId(pub String);

/// Construit le `CorsLayer` pour le réseau LAN.
///
/// En production, seules les origines locales sont autorisées.
/// Le frontend Electron tourne en `file://` ou `http://localhost:*`.
///
/// # Sécurité
/// Cette API ne doit **jamais** être exposée sur Internet.
/// Le CORS est une défense en profondeur — la vraie protection est le réseau LAN.
pub fn cors_layer() -> CorsLayer {
    use tower_http::cors::AllowOrigin;
    CorsLayer::new()
        .allow_origin(AllowOrigin::predicate(|origin, _| {
            let origin = origin.as_bytes();
            // Autoriser : localhost, 127.0.0.1, 192.168.x.x, 10.x.x.x, 172.16-31.x.x
            // et file:// (Electron en mode développement)
            origin.starts_with(b"http://localhost")
                || origin.starts_with(b"http://127.")
                || origin.starts_with(b"http://192.168.")
                || origin.starts_with(b"http://10.")
                || origin.starts_with(b"http://172.")
                || origin == b"file://"
        }))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
            X_REQUEST_ID.clone(),
        ])
}
