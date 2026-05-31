//! # routes::promotions
//!
//! Route de consultation des promotions actuellement actives.
//!
//! ## Routes
//! - `GET /api/v1/promotions/available` — liste les promotions valides à l'instant présent
//!
//! ## Architecture
//! Le filtrage temporel (validité date/heure/jour) est effectué côté promo-engine
//! lors de l'évaluation du panier. Cette route retourne toutes les promotions actives
//! (active = 1) sans filtrage supplémentaire, pour affichage côté pos-app.

use axum::{extract::State, http::StatusCode, Json};
use serde::Serialize;

use crate::app::AppState;

// ---------------------------------------------------------------------------
// DTO de réponse
// ---------------------------------------------------------------------------

/// Promotion disponible retournée par l'API.
#[derive(Debug, Serialize)]
pub struct AvailablePromo {
    /// Identifiant unique de la promotion (UUID).
    pub id: String,
    /// Nom affiché de la promotion.
    pub name: String,
    /// Type de promotion : fixed_amount, percentage, item_discount, bogo, happy_hour.
    pub promo_type: String,
    /// Mode de déclenchement : auto | manual.
    /// Sérialisé en "trigger" (nom API) — la colonne SQLite s'appelle trigger_type.
    #[serde(rename = "trigger")]
    pub trigger_type: String,
    /// Remise fixe en centimes (si applicable).
    pub value_cents: Option<i64>,
    /// Remise en points de base (si applicable, ex: 1000 = 10%).
    pub value_bps: Option<i64>,
    /// SKU cible (pour item_discount et bogo).
    pub target_sku: Option<String>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

/// `GET /api/v1/promotions/available`
///
/// Retourne toutes les promotions actives (active = 1).
/// Le filtrage temporel fin (date/heure/jour) est délégué au promo-engine lors
/// de l'évaluation du panier dans `POST /api/v1/orders`.
///
/// ## Réponse `200 OK`
/// ```json
/// [
///   {
///     "id": "uuid",
///     "name": "Happy Hour -20%",
///     "promo_type": "happy_hour",
///     "trigger_type": "auto",
///     "value_cents": null,
///     "value_bps": 2000,
///     "target_sku": null
///   }
/// ]
/// ```
///
/// # Errors
/// - `500` si la requête SQLite échoue
pub async fn get_available_promotions(
    State(state): State<AppState>,
) -> Result<Json<Vec<AvailablePromo>>, StatusCode> {
    let rows = sqlx::query(
        "SELECT id, name, promo_type, trigger_type, value_cents, value_bps, target_sku
         FROM promotions
         WHERE active = 1",
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let promos: Vec<AvailablePromo> = rows
        .into_iter()
        .map(|row| {
            use sqlx::Row as _;
            AvailablePromo {
                id: row.try_get("id").unwrap_or_default(),
                name: row.try_get("name").unwrap_or_default(),
                promo_type: row.try_get("promo_type").unwrap_or_default(),
                trigger_type: row.try_get("trigger_type").unwrap_or_default(),
                value_cents: row.try_get("value_cents").unwrap_or(None),
                value_bps: row.try_get("value_bps").unwrap_or(None),
                target_sku: row.try_get("target_sku").unwrap_or(None),
            }
        })
        .collect();

    Ok(Json(promos))
}
