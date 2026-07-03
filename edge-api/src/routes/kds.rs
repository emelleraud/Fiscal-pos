//! # `routes::kds`
//!
//! Routes KDS (Kitchen Display System) de l'edge-api.
//!
//! ## Table des routes
//!
//! | Méthode | Chemin                              | Handler              |
//! |---------|--------------------------------------|----------------------|
//! | GET     | /api/v1/kds/feed/ready_board        | `kds_ready_board`    |
//! | GET     | /api/v1/kds/feed/:station_id        | `kds_feed`           |
//! | POST    | /api/v1/kds/orders/:order_id/ack    | `kds_ack`            |
//! | POST    | /api/v1/kds/orders/:order_id/served | `kds_served`         |
//! | GET     | /api/v1/kds/config                  | `kds_get_config`     |
//! | PUT     | /api/v1/kds/config                  | `kds_set_config`     |
//! | GET     | /api/v1/kds/stations                | `kds_stations`       |

use std::convert::Infallible;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::sse::{Event, KeepAlive, Sse},
    Json,
};
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use tokio_stream::wrappers::BroadcastStream;

use crate::{app::AppState, error::KdsApiErr};
use kds_engine::{routing, state_machine};

// ---------------------------------------------------------------------------
// SSE — GET /api/v1/kds/feed/ready_board
// ---------------------------------------------------------------------------

/// Flux SSE pour l'ORB (Order Ready Board) — reçoit tous les événements KDS.
pub async fn kds_ready_board(
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.kds_broadcaster.subscribe();

    let stream = BroadcastStream::new(rx).filter_map(|msg| async move {
        let event = msg.ok()?;
        let data = serde_json::to_string(&event).ok()?;
        Some(Ok(Event::default().event(event.event_type()).data(data)))
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// SSE — GET /api/v1/kds/feed/:station_id
// ---------------------------------------------------------------------------

/// Flux SSE filtré par station KDS.
pub async fn kds_feed(
    Path(station_id): Path<String>,
    State(state): State<AppState>,
) -> Sse<impl futures_util::Stream<Item = Result<Event, Infallible>>> {
    let rx = state.kds_broadcaster.subscribe();
    let sid = station_id.clone();

    let stream = BroadcastStream::new(rx).filter_map(move |msg| {
        let sid = sid.clone();
        async move {
            let event = msg.ok()?;
            // Diffuser à la station concernée OU événements broadcast (station_id vide)
            if event.station_id() == sid || event.station_id().is_empty() {
                let data = serde_json::to_string(&event).ok()?;
                Some(Ok(Event::default().event(event.event_type()).data(data)))
            } else {
                None
            }
        }
    });

    Sse::new(stream).keep_alive(KeepAlive::default())
}

// ---------------------------------------------------------------------------
// Acknowledge — POST /api/v1/kds/orders/:order_id/ack
// ---------------------------------------------------------------------------

/// Corps de la requête d'acquittement KDS.
#[derive(Debug, Deserialize)]
pub struct AckBody {
    /// Station KDS qui émet l'acquittement.
    pub station_id: String,
    /// Ligne acquittée, ou absent pour acquitter toute la commande.
    #[serde(default)]
    pub line_id: Option<String>,
}

/// Acquitte une commande ou une ligne pour une station KDS donnée.
///
/// # Errors
/// Returns [`KdsApiErr`] on database or broadcast errors.
pub async fn kds_ack(
    Path(order_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<AckBody>,
) -> Result<StatusCode, KdsApiErr> {
    state_machine::acknowledge(
        &state.db,
        &state.kds_broadcaster,
        &order_id,
        &body.station_id,
        body.line_id.as_deref(),
    )
    .await
    .map_err(KdsApiErr::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Servi — POST /api/v1/kds/orders/:order_id/served
// ---------------------------------------------------------------------------

/// Corps de la requête de marquage "servi".
#[derive(Debug, Deserialize)]
pub struct ServedBody {
    /// Station KDS qui marque la commande comme servie.
    pub station_id: String,
}

/// Marque une commande comme servie (2e bump expo).
///
/// # Errors
/// Returns [`KdsApiErr`] on database or broadcast errors.
pub async fn kds_served(
    Path(order_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<ServedBody>,
) -> Result<StatusCode, KdsApiErr> {
    state_machine::mark_served(
        &state.db,
        &state.kds_broadcaster,
        &order_id,
        &body.station_id,
    )
    .await
    .map_err(KdsApiErr::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Config — GET /api/v1/kds/config
// ---------------------------------------------------------------------------

/// Réponse de configuration KDS active.
#[derive(Debug, Serialize)]
pub struct KdsConfig {
    /// Identifiant du profil de service actif.
    pub active_profile: String,
}

/// Retourne la configuration KDS active (profil de service).
///
/// # Errors
/// Returns [`KdsApiErr`] on database errors.
pub async fn kds_get_config(State(state): State<AppState>) -> Result<Json<KdsConfig>, KdsApiErr> {
    let profile = routing::active_profile_id(&state.db)
        .await
        .map_err(KdsApiErr::from)?;

    Ok(Json(KdsConfig {
        active_profile: profile,
    }))
}

// ---------------------------------------------------------------------------
// Config — PUT /api/v1/kds/config
// ---------------------------------------------------------------------------

/// Corps de la requête de changement de profil.
#[derive(Debug, Deserialize)]
pub struct SetProfileBody {
    /// Identifiant du nouveau profil de service actif.
    pub active_profile: String,
}

/// Met à jour le profil de service actif.
///
/// # Errors
/// Returns [`KdsApiErr`] on database errors.
pub async fn kds_set_config(
    State(state): State<AppState>,
    Json(body): Json<SetProfileBody>,
) -> Result<StatusCode, KdsApiErr> {
    routing::set_active_profile(&state.db, &body.active_profile)
        .await
        .map_err(KdsApiErr::from)?;

    Ok(StatusCode::NO_CONTENT)
}

// ---------------------------------------------------------------------------
// Stations — GET /api/v1/kds/stations
// ---------------------------------------------------------------------------

/// Retourne la liste des stations actives pour le profil courant.
///
/// # Errors
/// Returns [`KdsApiErr`] on database errors.
pub async fn kds_stations(
    State(state): State<AppState>,
) -> Result<Json<Vec<kds_engine::types::station::Station>>, KdsApiErr> {
    let profile = routing::active_profile_id(&state.db)
        .await
        .map_err(KdsApiErr::from)?;

    let stations = routing::stations_for_profile(&state.db, &profile)
        .await
        .map_err(KdsApiErr::from)?;

    Ok(Json(stations))
}

// ---------------------------------------------------------------------------
// Heartbeat — POST /api/v1/kds/heartbeat/:station_id
// ---------------------------------------------------------------------------

/// Enregistre un heartbeat de présence pour une station KDS.
/// Utilisé par kds-app pour signaler qu'un écran est connecté.
/// Réponse toujours 204 — aucune validation du `station_id`.
pub async fn kds_heartbeat(
    Path(station_id): Path<String>,
    State(state): State<AppState>,
) -> StatusCode {
    state
        .station_heartbeats
        .insert(station_id, std::time::Instant::now());
    StatusCode::NO_CONTENT
}
