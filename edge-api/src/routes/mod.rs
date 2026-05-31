//! # routes
//!
//! Construction du routeur Axum complet de l'`edge-api`.
//!
//! ## Table des routes
//!
//! | Méthode | Chemin                          | Handler                    | Niveau d'accès |
//! |---------|----------------------------------|----------------------------|----------------|
//! | GET     | /api/v1/health                   | health_handler             | Public LAN     |
//! | GET     | /api/v1/menu                     | menu_handler               | Public LAN     |
//! | GET     | /api/v1/sessions/current         | get_current_session_handler| Caisse         |
//! | POST    | /api/v1/sessions/open            | open_session_handler       | Caisse         |
//! | POST    | /api/v1/sessions/close           | close_session_handler      | Manager        |
//! | POST    | /api/v1/orders                   | create_order_handler       | Caisse         |
//! | GET     | /api/v1/orders/:id               | get_order_handler          | Caisse         |
//! | POST    | /api/v1/orders/:id/pay           | pay_order_handler          | Caisse/TPE     |
//! | POST    | /api/v1/orders/:id/cancel        | cancel_order_handler       | Manager        |
//!
//! ## Niveau d'accès
//! Toutes les routes sont exposées sur le LAN uniquement (pas d'Internet).
//! L'authentification manager (PIN ou badge) est validée côté `pos-app` —
//! l'API elle-même ne distingue pas les niveaux pour le MVP.

pub mod archive;
pub mod health;
pub mod menu;
pub mod orders;
pub mod promotions;
pub mod sessions;

use axum::{routing::{get, post}, Router};

use crate::app::AppState;

/// Construit le routeur Axum complet avec toutes les routes de l'API.
#[must_use]
pub fn build_router(state: AppState) -> Router {
    Router::new()
        // --- Health ---
        .route("/api/v1/health", get(health::health_handler))
        // --- Menu ---
        .route("/api/v1/menu", get(menu::menu_handler))
        // --- Sessions ---
        .route("/api/v1/sessions/current", get(sessions::get_current_session_handler))
        .route("/api/v1/sessions/open",    post(sessions::open_session_handler))
        .route("/api/v1/sessions/close",   post(sessions::close_session_handler))
        // --- Commandes ---
        .route("/api/v1/orders",               post(orders::create_order_handler))
        .route("/api/v1/orders/:id",           get(orders::get_order_handler))
        .route("/api/v1/orders/:id/pay",       post(orders::pay_order_handler))
        .route("/api/v1/orders/:id/cancel",    post(orders::cancel_order_handler))
        // --- Promotions ---
        .route("/api/v1/promotions/available", get(promotions::get_available_promotions))
        // --- Archive annuelle NF525 §7 ---
        .route("/api/v1/archive/:year",        post(archive::generate_archive_handler))
        .with_state(state)
}
