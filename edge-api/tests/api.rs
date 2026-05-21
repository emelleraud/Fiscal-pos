//! Tests d'intégration de l'edge-api.
//!
//! Chaque test monte une application Axum complète avec SQLite in-memory.
//! Les requêtes sont envoyées via `tower::ServiceExt::oneshot` — pas de réseau.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use serde_json::{json, Value};
use sqlx::sqlite::SqlitePoolOptions;
use tempfile::NamedTempFile;
use tower::ServiceExt;

use edge_api::app::{build_app, AppState};
use fiscal_engine::{archive_engine::generate_signing_keypair, hex_encode, journal::Journal};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Crée une app de test avec une base SQLite dans un fichier temporaire.
///
/// On utilise un vrai fichier (pas `:memory:`) car `record_transaction()`
/// ouvre une transaction ET lit le pool en parallèle — ce qui deadlockerait
/// avec `max_connections(1)` sur une DB in-memory mono-connexion.
/// Retourne `(router, _db_file)` : le fichier doit rester en vie pendant le test.
async fn setup() -> (axum::Router, NamedTempFile) {
    let db_file = NamedTempFile::new().expect("tempfile SQLite");
    let db_path = db_file.path().to_str().unwrap().to_string();

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&format!("sqlite:{db_path}"))
        .await
        .expect("pool SQLite");

    let journal = Journal::open(pool).await.expect("journal");
    let state = AppState::new(journal, "/tmp".to_string());
    (build_app(state), db_file)
}

fn json_request(method: Method, uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn empty_request(method: Method, uri: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

async fn body_json(resp: axum::response::Response) -> Value {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap_or(Value::Null)
}

// ---------------------------------------------------------------------------
// Health
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_returns_ok() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(empty_request(Method::GET, "/api/v1/health"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "connected");
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

#[tokio::test]
async fn open_session_returns_201() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert!(body["session_id"].is_string());
    assert_eq!(body["session_sequence"], 1);
}

#[tokio::test]
async fn open_session_twice_returns_409() {
    let (app, _db) = setup().await;

    let req1 = empty_request(Method::POST, "/api/v1/sessions/open");
    app.clone().oneshot(req1).await.unwrap();

    let req2 = empty_request(Method::POST, "/api/v1/sessions/open");
    let resp = app.oneshot(req2).await.unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn get_current_session_no_session_returns_409() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(empty_request(Method::GET, "/api/v1/sessions/current"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn get_current_session_after_open_returns_200() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let resp = app
        .oneshot(empty_request(Method::GET, "/api/v1/sessions/current"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["session_sequence"], 1);
    assert_eq!(body["status"], "open");
}

#[tokio::test]
async fn close_session_no_session_returns_409() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/close"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn close_session_returns_200_with_z_report() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    // Une vente avant clôture (le rapport Z requiert au moins une entrée)
    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-TEST-001",
                "line_items": [{ "amount_ttc_cents": 1100, "tva_rate": "10" }],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();

    let resp = app
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/close"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert!(body["z_report"]["closing_hash_hex"].is_string());
    assert_eq!(body["z_report"]["total_sales_cents"], 1100);
}

// ---------------------------------------------------------------------------
// Commandes
// ---------------------------------------------------------------------------

#[tokio::test]
async fn create_order_no_session_returns_409() {
    let (app, _db) = setup().await;
    let resp = app
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-001",
                "line_items": [{ "amount_ttc_cents": 1100, "tva_rate": "10" }],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn create_order_single_rate_returns_201() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let resp = app
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-001",
                "line_items": [{ "amount_ttc_cents": 1100, "tva_rate": "10" }],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert!(body["order_id"].is_string());
    assert_eq!(body["fiscal_entry"]["amount_ttc_cents"], 1100);
    assert_eq!(body["fiscal_entry"]["sequence_number"], 1);
    assert_eq!(body["fiscal_entry"]["tva_10_ht_cents"], 1000);
    assert_eq!(body["fiscal_entry"]["tva_10_tva_cents"], 100);
}

#[tokio::test]
async fn create_order_multi_rate_returns_201() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let resp = app
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-MULTI",
                "line_items": [
                    { "amount_ttc_cents": 1100, "tva_rate": "10" },
                    { "amount_ttc_cents":  240, "tva_rate": "20" }
                ],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_json(resp).await;
    assert_eq!(body["fiscal_entry"]["amount_ttc_cents"], 1340);
    assert_eq!(body["fiscal_entry"]["tva_10_ht_cents"], 1000);
    assert_eq!(body["fiscal_entry"]["tva_10_tva_cents"], 100);
    assert_eq!(body["fiscal_entry"]["tva_20_ht_cents"], 200);
    assert_eq!(body["fiscal_entry"]["tva_20_tva_cents"], 40);
    assert_eq!(body["fiscal_entry"]["tva_5_5_ht_cents"], 0);
}

#[tokio::test]
async fn create_order_empty_line_items_returns_422() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let resp = app
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-EMPTY",
                "line_items": [],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn pay_order_returns_200() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let create_resp = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-PAY",
                "line_items": [{ "amount_ttc_cents": 1100, "tva_rate": "10" }],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();
    let create_body = body_json(create_resp).await;
    let order_id = create_body["order_id"].as_str().unwrap();

    let resp = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/orders/{order_id}/pay"),
            json!({ "payment_method": "card", "amount_paid_cents": 1100 }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["status"], "paid");
}

#[tokio::test]
async fn cancel_order_returns_200() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let create_resp = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-CANCEL",
                "line_items": [{ "amount_ttc_cents": 1100, "tva_rate": "10" }],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();
    let create_body = body_json(create_resp).await;
    let order_id = create_body["order_id"].as_str().unwrap();
    let entry_id = create_body["fiscal_entry"]["id"].as_str().unwrap();

    let resp = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/orders/{order_id}/cancel"),
            json!({
                "reason": "Erreur de saisie",
                "fiscal_entry_id": entry_id,
                "amount_ttc_cents": 1100,
                "tva_rate": "10"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = body_json(resp).await;
    assert_eq!(body["fiscal_entry"]["amount_ttc_cents"], -1100);
}

#[tokio::test]
async fn cancel_order_no_reason_returns_422() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    let create_resp = app
        .clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-CANCEL2",
                "line_items": [{ "amount_ttc_cents": 500, "tva_rate": "5.5" }],
                "payment_method": "cash"
            }),
        ))
        .await
        .unwrap();
    let create_body = body_json(create_resp).await;
    let order_id = create_body["order_id"].as_str().unwrap();
    let entry_id = create_body["fiscal_entry"]["id"].as_str().unwrap();

    let resp = app
        .oneshot(json_request(
            Method::POST,
            &format!("/api/v1/orders/{order_id}/cancel"),
            json!({
                "reason": "",
                "fiscal_entry_id": entry_id,
                "amount_ttc_cents": 500,
                "tva_rate": "5.5"
            }),
        ))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn session_accumulators_reflect_orders() {
    let (app, _db) = setup().await;

    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();

    // Deux ventes
    for (reference, amount) in [("ORD-A", 1100_i64), ("ORD-B", 550)] {
        app.clone()
            .oneshot(json_request(
                Method::POST,
                "/api/v1/orders",
                json!({
                    "order_reference": reference,
                    "line_items": [{ "amount_ttc_cents": amount, "tva_rate": "10" }],
                    "payment_method": "card"
                }),
            ))
            .await
            .unwrap();
    }

    let resp = app
        .oneshot(empty_request(Method::GET, "/api/v1/sessions/current"))
        .await
        .unwrap();

    let body = body_json(resp).await;
    assert_eq!(body["totals"]["sales_cents"], 1650);
    assert_eq!(body["totals"]["entry_count"], 2);
}

// ---------------------------------------------------------------------------
// Archive annuelle NF525 §7
// ---------------------------------------------------------------------------

#[tokio::test]
async fn archive_no_signing_key_returns_500() {
    let (app, _db) = setup().await;
    // Sans FISCAL_SIGNING_KEY_HEX, la route doit retourner une erreur serveur
    std::env::remove_var("FISCAL_SIGNING_KEY_HEX");

    let resp = app
        .oneshot(empty_request(Method::POST, "/api/v1/archive/2024"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn archive_no_data_returns_404() {
    let (app, _db) = setup().await;
    let (priv_bytes, _) = generate_signing_keypair();
    std::env::set_var("FISCAL_SIGNING_KEY_HEX", hex_encode(&priv_bytes));

    let resp = app
        .oneshot(empty_request(Method::POST, "/api/v1/archive/1999"))
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn archive_generates_and_returns_409_on_duplicate() {
    let (app, _db) = setup().await;
    let (priv_bytes, _) = generate_signing_keypair();
    std::env::set_var("FISCAL_SIGNING_KEY_HEX", hex_encode(&priv_bytes));

    // Ouvrir une session, faire une vente, clôturer
    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/open"))
        .await
        .unwrap();
    app.clone()
        .oneshot(json_request(
            Method::POST,
            "/api/v1/orders",
            json!({
                "order_reference": "ORD-ARCH-001",
                "line_items": [{ "amount_ttc_cents": 1100, "tva_rate": "10" }],
                "payment_method": "card"
            }),
        ))
        .await
        .unwrap();
    app.clone()
        .oneshot(empty_request(Method::POST, "/api/v1/sessions/close"))
        .await
        .unwrap();

    // Première génération → 201
    let year = current_year();
    let resp1 = app
        .clone()
        .oneshot(empty_request(
            Method::POST,
            &format!("/api/v1/archive/{year}"),
        ))
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::CREATED);
    let body = body_json(resp1).await;
    assert!(body["csv_hash_hex"].is_string());
    assert!(body["signature_hex"].is_string());
    assert_eq!(body["year"], year as i64);

    // Deuxième tentative → 409
    let resp2 = app
        .oneshot(empty_request(
            Method::POST,
            &format!("/api/v1/archive/{year}"),
        ))
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::CONFLICT);
}

fn current_year() -> u32 {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    1970 + (secs / (365 * 24 * 3600 + 20952)) as u32
}
