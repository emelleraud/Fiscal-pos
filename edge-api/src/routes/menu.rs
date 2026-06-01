//! # `routes::menu`
//!
//! Route de consultation de la carte active du restaurant.
//!
//! ## GET /api/v1/menu
//!
//! Retourne la liste des articles disponibles à la vente, leur prix TTC
//! et le taux de TVA applicable.
//!
//! ## Source de données
//! La carte est lue depuis un fichier JSON local (`menu.json`) situé dans
//! le répertoire de données du restaurant. Ce fichier est mis à jour par
//! le `sync-client` lorsqu'une nouvelle configuration est reçue du cloud.
//!
//! Si le fichier est absent, une carte vide est retournée (la caisse peut
//! toujours fonctionner en mode dégradé avec saisie manuelle des montants).

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::app::AppState;
use fiscal_engine::TvaRate;

/// Article de menu.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuItem {
    /// Identifiant unique de l'article (UUID ou code interne).
    pub id: String,
    /// Nom affiché sur la caisse.
    pub name: String,
    /// Prix TTC en centimes.
    pub price_ttc_cents: i64,
    /// Taux de TVA applicable.
    pub tva_rate: TvaRateDto,
    /// Catégorie (ex: "Burgers", "Boissons", "Desserts").
    pub category: String,
    /// L'article est-il disponible à la vente actuellement.
    pub available: bool,
}

/// DTO pour le taux de TVA dans les réponses JSON.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvaRateDto {
    /// TVA 5,5%
    Reduit5_5,
    /// TVA 10%
    Intermediaire10,
    /// TVA 20%
    Normal20,
}

impl From<TvaRate> for TvaRateDto {
    fn from(rate: TvaRate) -> Self {
        match rate {
            TvaRate::Reduit5_5 => Self::Reduit5_5,
            TvaRate::Normal20 => Self::Normal20,
            _ => Self::Intermediaire10, // fallback : taux intermédiaire par défaut
        }
    }
}

/// Réponse de la route menu.
#[derive(Debug, Serialize, Deserialize)]
pub struct MenuResponse {
    /// Liste des articles disponibles.
    pub items: Vec<MenuItem>,
    /// Timestamp de la dernière mise à jour de la carte (Unix ms).
    pub last_updated_ms: u64,
    /// Version de la carte (incrémentée à chaque modification par le sync-client).
    pub version: u32,
}

/// `GET /api/v1/menu`
///
/// Retourne la carte active du restaurant.
///
/// # Responses
/// - `200 OK` avec la liste des articles
pub async fn menu_handler(State(state): State<AppState>) -> (StatusCode, Json<MenuResponse>) {
    // Charger depuis le fichier menu.json si disponible
    let menu = load_menu_from_file(&state.data_dir).unwrap_or_else(default_menu);

    (StatusCode::OK, Json(menu))
}

/// Charge le menu depuis `{data_dir}/menu.json`.
///
/// Retourne `None` si le fichier est absent ou invalide.
fn load_menu_from_file(data_dir: &str) -> Option<MenuResponse> {
    let path = format!("{data_dir}/menu.json");
    let content = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Carte par défaut retournée si `menu.json` est absent.
///
/// Permet à la caisse de démarrer même sans configuration initiale.
fn default_menu() -> MenuResponse {
    MenuResponse {
        items: vec![
            MenuItem {
                id: "default-001".to_string(),
                name: "Article générique".to_string(),
                price_ttc_cents: 1000,
                tva_rate: TvaRateDto::Intermediaire10,
                category: "Général".to_string(),
                available: true,
            },
        ],
        last_updated_ms: 0,
        version: 0,
    }
}
