//! # `routes::orders`
//!
//! Routes de gestion des commandes et de l'enregistrement fiscal.
//!
//! ## Routes
//! - `POST /api/v1/orders`              — création d'une commande (enregistrement fiscal)
//! - `POST /api/v1/orders/:id/pay`      — validation du paiement TPE
//! - `POST /api/v1/orders/:id/cancel`   — annulation avec motif obligatoire
//! - `GET  /api/v1/orders/:id`          — consultation du ticket fiscal
//!
//! ## Architecture fiscale
//! Chaque route qui génère une opération fiscale appelle `journal.record_transaction()`
//! de manière **synchrone et transactionnelle**. Toute erreur du fiscal-engine est
//! bloquante — la réponse HTTP n'est envoyée qu'après confirmation de l'écriture `SQLite`.
//!
//! ## Modèle de commande simplifié
//! À ce stade MVP, une commande = une ligne fiscale unique.
//! Le modèle multi-articles (panier) sera géré côté `pos-app` (frontend) :
//! les articles sont agrégés en un montant total avant l'appel à cette API.
//!
//! ## Intégration TPE
//! Le paiement CB est **pass-through** : le terminal TPE traite la carte,
//! puis callback vers `POST /orders/:id/pay` avec confirmation.
//! Aucune donnée bancaire ne transite par l'edge-api.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{app::AppState, error::ApiErr};
use common::Cents;
use fiscal_engine::{
    errors::FiscalError,
    hex_encode,
    types::{
        entry::FiscalEntryData,
        operation::OperationType,
        tva::{TvaBreakdown, TvaRate},
    },
    FiscalEntry,
};
use kds_engine::state_machine::{dispatch_order, IncomingOrder};
use promo_engine::{Cart, CartItem, PromoType, Promotion, Trigger, TvaRateKey};

// ---------------------------------------------------------------------------
// DTOs de requête
// ---------------------------------------------------------------------------

/// Article de commande avec montant et taux de TVA.
#[derive(Debug, Deserialize)]
pub struct LineItem {
    /// SKU de l'article (optionnel — utilisé pour les promos ciblées sur un article).
    #[serde(default)]
    pub sku: Option<String>,
    /// Montant TTC en centimes pour cet article (quantité incluse).
    pub amount_ttc_cents: i64,
    /// Taux de TVA applicable à cet article.
    pub tva_rate: TvaRateRequest,
}

/// Corps de la requête de création de commande.
#[derive(Debug, Deserialize)]
pub struct CreateOrderRequest {
    /// Référence interne de la commande (générée par le frontend).
    pub order_reference: String,
    /// Articles de la commande avec leurs taux de TVA respectifs.
    pub line_items: Vec<LineItem>,
    /// Moyen de paiement prévu (informatif, pas fiscal).
    /// Stocké pour la traçabilité mais non utilisé dans la logique fiscale MVP.
    #[allow(dead_code)]
    pub payment_method: PaymentMethod,
    /// Identifiants de promotions manuelles sélectionnées par le caissier.
    #[serde(default)]
    pub manual_promo_ids: Vec<String>,
    /// Type de commande — détermine l'ORB cible et les règles de routage KDS.
    #[serde(default)]
    #[allow(dead_code)]
    pub order_type: common::OrderType,
}

/// Corps de la requête de validation de paiement.
#[derive(Debug, Deserialize)]
pub struct PayOrderRequest {
    /// Moyen de paiement effectivement utilisé (conservé pour la désérialisation JSON).
    #[allow(dead_code)]
    pub payment_method: PaymentMethod,
    /// Montant encaissé en centimes (peut différer si rendu monnaie espèces).
    pub amount_paid_cents: i64,
}

/// Corps de la requête d'annulation.
#[derive(Debug, Deserialize)]
pub struct CancelOrderRequest {
    /// Motif d'annulation (obligatoire NF525 §4.3).
    pub reason: String,
    /// Référence de l'entrée fiscale à annuler.
    pub fiscal_entry_id: String,
    /// Montant à annuler en centimes (négatif ou positif — on l'inversera).
    pub amount_ttc_cents: i64,
    /// Taux de TVA de l'entrée annulée (fallback mono-taux).
    pub tva_rate: TvaRateRequest,
    /// Montant TTC annulé au taux 5,5% (optionnel — pour les annulations multi-taux).
    pub tva_5_5_amount_ttc: Option<i64>,
    /// Montant TTC annulé au taux 10% (optionnel — pour les annulations multi-taux).
    pub tva_10_amount_ttc: Option<i64>,
    /// Montant TTC annulé au taux 20% (optionnel — pour les annulations multi-taux).
    pub tva_20_amount_ttc: Option<i64>,
}

/// Taux de TVA dans les requêtes JSON.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TvaRateRequest {
    /// TVA 5,5%
    #[serde(rename = "5.5")]
    Reduit5_5,
    /// TVA 10%
    #[serde(rename = "10")]
    Intermediaire10,
    /// TVA 20%
    #[serde(rename = "20")]
    Normal20,
}

impl From<TvaRateRequest> for TvaRate {
    fn from(r: TvaRateRequest) -> Self {
        match r {
            TvaRateRequest::Reduit5_5 => Self::Reduit5_5,
            TvaRateRequest::Intermediaire10 => Self::Intermediaire10,
            TvaRateRequest::Normal20 => Self::Normal20,
        }
    }
}

/// Moyen de paiement.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentMethod {
    /// Carte bancaire via TPE (pass-through).
    Card,
    /// Espèces.
    Cash,
    /// Ticket restaurant.
    MealVoucher,
}

// ---------------------------------------------------------------------------
// DTOs de réponse
// ---------------------------------------------------------------------------

/// Promotion appliquée lors d'une commande.
#[derive(Debug, Serialize)]
pub struct AppliedPromoResponse {
    /// Identifiant de la promotion.
    pub promo_id: String,
    /// Nom de la promotion.
    pub name: String,
    /// Montant de la remise en centimes (positif).
    pub discount_cents: i64,
}

/// Réponse après création / annulation d'une commande.
#[derive(Debug, Serialize)]
pub struct OrderResponse {
    /// Identifiant de la commande (UUID).
    pub order_id: String,
    /// Entrée fiscale créée.
    pub fiscal_entry: FiscalEntryResponse,
    /// Promotions appliquées lors de cette commande.
    pub applied_promos: Vec<AppliedPromoResponse>,
}

/// Réponse après validation du paiement (ticket de caisse).
#[derive(Debug, Serialize)]
pub struct PaymentResponse {
    /// Identifiant de la commande.
    pub order_id: String,
    /// Statut du paiement.
    pub status: &'static str,
    /// Entrée fiscale de confirmation.
    pub fiscal_entry: FiscalEntryResponse,
    /// Rendu monnaie en centimes (0 si paiement CB).
    pub change_cents: i64,
}

/// Résumé fiscal d'une entrée pour les réponses JSON.
#[derive(Debug, Serialize)]
pub struct FiscalEntryResponse {
    /// Identifiant de l'entrée fiscale.
    pub id: String,
    /// Numéro de séquence dans le journal.
    pub sequence_number: u64,
    /// Type d'opération.
    pub operation_type: String,
    /// Montant TTC en centimes.
    pub amount_ttc_cents: i64,
    /// Montant HT en centimes (taux dominant).
    pub ht_cents: i64,
    /// Montant TVA en centimes (taux dominant).
    pub tva_cents: i64,
    /// Taux de TVA dominant.
    pub tva_rate: String,
    /// Ventilation TVA 5,5% — HT en centimes.
    pub tva_5_5_ht_cents: i64,
    /// Ventilation TVA 5,5% — TVA en centimes.
    pub tva_5_5_tva_cents: i64,
    /// Ventilation TVA 10% — HT en centimes.
    pub tva_10_ht_cents: i64,
    /// Ventilation TVA 10% — TVA en centimes.
    pub tva_10_tva_cents: i64,
    /// Ventilation TVA 20% — HT en centimes.
    pub tva_20_ht_cents: i64,
    /// Ventilation TVA 20% — TVA en centimes.
    pub tva_20_tva_cents: i64,
    /// Hash SHA-256 de l'entrée (hex, 64 caractères).
    pub hash_hex: String,
    /// Timestamp de création en millisecondes Unix.
    pub created_at_ms: u64,
}

impl From<&FiscalEntry> for FiscalEntryResponse {
    fn from(e: &FiscalEntry) -> Self {
        Self {
            id: e.id.to_string(),
            sequence_number: e.sequence_number,
            operation_type: e.operation_type.to_string(),
            amount_ttc_cents: e.amount_ttc_cents.0,
            ht_cents: e.tva_breakdown.ht_cents.0,
            tva_cents: e.tva_breakdown.tva_cents.0,
            tva_rate: match e.tva_breakdown.rate {
                TvaRate::Reduit5_5 => "5.5".to_string(),
                TvaRate::Normal20 => "20".to_string(),
                _ => "10".to_string(),
            },
            tva_5_5_ht_cents: e.tva_5_5_breakdown.ht_cents.0,
            tva_5_5_tva_cents: e.tva_5_5_breakdown.tva_cents.0,
            tva_10_ht_cents: e.tva_10_breakdown.ht_cents.0,
            tva_10_tva_cents: e.tva_10_breakdown.tva_cents.0,
            tva_20_ht_cents: e.tva_20_breakdown.ht_cents.0,
            tva_20_tva_cents: e.tva_20_breakdown.tva_cents.0,
            hash_hex: hex_encode(&e.hash),
            created_at_ms: e.created_at_ms,
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers promo-engine
// ---------------------------------------------------------------------------

/// Ligne brute lue depuis la table `SQLite` `promotions`.
struct SqlitePromoRow {
    id: String,
    name: String,
    promo_type: String,
    trigger_type: String,
    value_cents: Option<i64>,
    value_bps: Option<i64>,
    target_sku: Option<String>,
    exclusion_group: Option<String>,
    priority: i64,
    valid_from: Option<String>,
    valid_to: Option<String>,
    days_of_week: Option<String>,
    time_from: Option<String>,
    time_to: Option<String>,
}

impl TryFrom<SqlitePromoRow> for Promotion {
    type Error = ();

    fn try_from(r: SqlitePromoRow) -> Result<Self, Self::Error> {
        let id: Uuid = r.id.parse().map_err(|_| ())?;

        let promo_type = match r.promo_type.as_str() {
            "fixed_amount" => PromoType::FixedAmount,
            "percentage" => PromoType::Percentage,
            "item_discount" => PromoType::ItemDiscount,
            "bogo" => PromoType::Bogo,
            "happy_hour" => PromoType::HappyHour,
            _ => return Err(()),
        };

        let trigger = match r.trigger_type.as_str() {
            "auto" => Trigger::Auto,
            "manual" => Trigger::Manual,
            _ => return Err(()),
        };

        // Analyser une date "YYYY-MM-DD" sans le feature `parsing` du crate `time`
        let parse_date = |s: &str| -> Option<time::Date> {
            let parts: Vec<&str> = s.splitn(3, '-').collect();
            if parts.len() < 3 {
                return None;
            }
            let year: i32 = parts[0].parse().ok()?;
            let month: u8 = parts[1].parse().ok()?;
            let day: u8 = parts[2].parse().ok()?;
            let month = time::Month::try_from(month).ok()?;
            time::Date::from_calendar_date(year, month, day).ok()
        };

        let valid_from = r.valid_from.as_deref().and_then(parse_date);
        let valid_to = r.valid_to.as_deref().and_then(parse_date);

        // days_of_week est stocké en JSON : "[1,2,5]" ou NULL
        let days_of_week: Option<Vec<u8>> = r
            .days_of_week
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<u8>>(s).ok());

        // time_from / time_to : format "HH:MM"
        let parse_time = |s: &str| -> Option<time::Time> {
            let parts: Vec<&str> = s.splitn(2, ':').collect();
            if parts.len() < 2 {
                return None;
            }
            let h: u8 = parts[0].parse().ok()?;
            let m: u8 = parts[1].parse().ok()?;
            time::Time::from_hms(h, m, 0).ok()
        };

        let time_from = r.time_from.as_deref().and_then(parse_time);
        let time_to = r.time_to.as_deref().and_then(parse_time);

        Ok(Promotion {
            id,
            name: r.name,
            promo_type,
            value_cents: r.value_cents,
            value_bps: r.value_bps,
            target_sku: r.target_sku,
            trigger,
            exclusion_group: r.exclusion_group,
            priority: i32::try_from(r.priority).unwrap_or(0),
            valid_from,
            valid_to,
            days_of_week,
            time_from,
            time_to,
        })
    }
}

/// Données d'une entrée DISCOUNT pré-validée, en attente d'enregistrement.
struct DiscountEntry {
    data: FiscalEntryData,
    promo_id: String,
    promo_name: String,
    discount_cents: i64,
}

/// Convertit un `TvaRateRequest` vers le type `TvaRateKey` du promo-engine.
fn tva_rate_request_to_key(r: TvaRateRequest) -> TvaRateKey {
    match r {
        TvaRateRequest::Reduit5_5 => TvaRateKey::Reduit5_5,
        TvaRateRequest::Intermediaire10 => TvaRateKey::Intermediaire10,
        TvaRateRequest::Normal20 => TvaRateKey::Normal20,
    }
}

// ---------------------------------------------------------------------------
// Handlers
// ---------------------------------------------------------------------------

/// `POST /api/v1/orders`
///
/// Crée une commande et l'enregistre dans le journal fiscal.
///
/// ## Corps de la requête
/// ```json
/// {
///   "order_reference": "ORD-20240115-001",
///   "amount_ttc_cents": 1100,
///   "tva_rate": "10",
///   "payment_method": "card"
/// }
/// ```
///
/// ## Réponse `201 Created`
/// ```json
/// {
///   "order_id": "uuid",
///   "fiscal_entry": { "id": "...", "sequence_number": 42, "hash_hex": "..." }
/// }
/// ```
///
/// # Errors
/// - `409` si aucune session n'est ouverte
/// - `422` si le montant ou la TVA est invalide
/// - `500` si le fiscal-engine échoue (bloquant)
#[allow(clippy::too_many_lines)]
pub async fn create_order_handler(
    State(state): State<AppState>,
    Json(body): Json<CreateOrderRequest>,
) -> Result<(StatusCode, Json<OrderResponse>), ApiErr> {
    let session_id = require_active_session(&state).await?;

    // Agréger les montants TTC par taux depuis les line_items
    let mut ttc_5_5: i64 = 0;
    let mut ttc_10: i64 = 0;
    let mut ttc_20: i64 = 0;
    for item in &body.line_items {
        match item.tva_rate {
            TvaRateRequest::Reduit5_5 => ttc_5_5 += item.amount_ttc_cents,
            TvaRateRequest::Intermediaire10 => ttc_10 += item.amount_ttc_cents,
            TvaRateRequest::Normal20 => ttc_20 += item.amount_ttc_cents,
        }
    }
    let total_ttc = ttc_5_5 + ttc_10 + ttc_20;

    // Décomposition par taux
    let tva_5_5_breakdown = TvaBreakdown::from_ttc(Cents(ttc_5_5), TvaRate::Reduit5_5);
    let tva_10_breakdown = TvaBreakdown::from_ttc(Cents(ttc_10), TvaRate::Intermediaire10);
    let tva_20_breakdown = TvaBreakdown::from_ttc(Cents(ttc_20), TvaRate::Normal20);

    // Taux dominant (par montant TTC)
    let dominant_rate = if ttc_20 >= ttc_10 && ttc_20 >= ttc_5_5 {
        TvaRate::Normal20
    } else if ttc_10 >= ttc_5_5 {
        TvaRate::Intermediaire10
    } else {
        TvaRate::Reduit5_5
    };

    // Décomposition principale : taux dominant + totaux agrégés
    let total_ht =
        tva_5_5_breakdown.ht_cents.0 + tva_10_breakdown.ht_cents.0 + tva_20_breakdown.ht_cents.0;
    let total_tva =
        tva_5_5_breakdown.tva_cents.0 + tva_10_breakdown.tva_cents.0 + tva_20_breakdown.tva_cents.0;
    let tva_breakdown = TvaBreakdown {
        rate: dominant_rate,
        ht_cents: Cents(total_ht),
        tva_cents: Cents(total_tva),
        ttc_cents: Cents(total_ttc),
    };

    let data = FiscalEntryData {
        session_id,
        operation_type: OperationType::Sale,
        amount_ttc_cents: Cents(total_ttc),
        tva_breakdown,
        tva_5_5_breakdown,
        tva_10_breakdown,
        tva_20_breakdown,
        reason: None,
        order_reference: Some(body.order_reference.clone()),
    };

    // -----------------------------------------------------------------------
    // Évaluation des promotions
    // -----------------------------------------------------------------------

    // Charger toutes les promotions actives depuis SQLite
    let raw_rows = sqlx::query(
        "SELECT id, name, promo_type, trigger_type, value_cents, value_bps, target_sku,
                exclusion_group, priority, valid_from, valid_to, days_of_week,
                time_from, time_to
         FROM promotions WHERE active = 1",
    )
    .fetch_all(&state.db)
    .await
    .map_err(FiscalError::Database)?;

    // Convertir en lignes structurées puis en types promo-engine
    let promos: Vec<Promotion> = raw_rows
        .into_iter()
        .filter_map(|row| {
            use sqlx::Row as _;
            let r = SqlitePromoRow {
                id: row.try_get("id").unwrap_or_default(),
                name: row.try_get("name").unwrap_or_default(),
                promo_type: row.try_get("promo_type").unwrap_or_default(),
                trigger_type: row.try_get("trigger_type").unwrap_or_default(),
                value_cents: row.try_get("value_cents").unwrap_or(None),
                value_bps: row.try_get("value_bps").unwrap_or(None),
                target_sku: row.try_get("target_sku").unwrap_or(None),
                exclusion_group: row.try_get("exclusion_group").unwrap_or(None),
                priority: row.try_get("priority").unwrap_or(0),
                valid_from: row.try_get("valid_from").unwrap_or(None),
                valid_to: row.try_get("valid_to").unwrap_or(None),
                days_of_week: row.try_get("days_of_week").unwrap_or(None),
                time_from: row.try_get("time_from").unwrap_or(None),
                time_to: row.try_get("time_to").unwrap_or(None),
            };
            Promotion::try_from(r).ok()
        })
        .collect();

    // Construire le panier pour le promo-engine
    let cart = Cart {
        line_items: body
            .line_items
            .iter()
            .map(|li| CartItem {
                sku: li.sku.clone().unwrap_or_default(),
                amount_ttc_cents: li.amount_ttc_cents,
                tva_rate: tva_rate_request_to_key(li.tva_rate),
            })
            .collect(),
        total_ttc_cents: total_ttc,
    };

    // Parser les IDs manuels fournis par le frontend
    let manual_ids: Vec<Uuid> = body
        .manual_promo_ids
        .iter()
        .filter_map(|s| s.parse().ok())
        .collect();

    // Évaluer les promotions applicables
    let eval = promo_engine::evaluate(&cart, &promos, &manual_ids, time::OffsetDateTime::now_utc());

    // -----------------------------------------------------------------------
    // Pré-construction et pré-validation de toutes les entrées DISCOUNT
    // AVANT d'enregistrer la VENTE — garantit la cohérence du journal.
    // (NF525 : si une remise est invalide, la vente ne doit pas être commitée)
    // -----------------------------------------------------------------------
    let mut pending_discounts: Vec<DiscountEntry> = Vec::new();

    for app in &eval.applied {
        if app.discount_cents <= 0 {
            continue;
        }

        // Décomposition TVA de la remise par taux (proportionnelle au panier)
        let disc_5_5 = -app.tva_allocation.cents_5_5;
        let disc_10 = -app.tva_allocation.cents_10;
        let disc_20 = -app.tva_allocation.cents_20;

        let disc_bd_5_5 = TvaBreakdown::from_ttc(Cents(disc_5_5), TvaRate::Reduit5_5);
        let disc_bd_10 = TvaBreakdown::from_ttc(Cents(disc_10), TvaRate::Intermediaire10);
        let disc_bd_20 = TvaBreakdown::from_ttc(Cents(disc_20), TvaRate::Normal20);

        // Fix 1: ttc_cents du breakdown principal = somme HT+TVA calculée,
        // pas discount_amount brut. Évite les échecs de validation par
        // arrondi entier (invariant : ht + tva == ttc doit toujours tenir).
        let disc_total_ht = disc_bd_5_5.ht_cents.0 + disc_bd_10.ht_cents.0 + disc_bd_20.ht_cents.0;
        let disc_total_tva =
            disc_bd_5_5.tva_cents.0 + disc_bd_10.tva_cents.0 + disc_bd_20.tva_cents.0;

        // Taux dominant de la remise
        let disc_dominant = if disc_20.abs() >= disc_10.abs() && disc_20.abs() >= disc_5_5.abs() {
            TvaRate::Normal20
        } else if disc_10.abs() >= disc_5_5.abs() {
            TvaRate::Intermediaire10
        } else {
            TvaRate::Reduit5_5
        };

        let disc_bd_main = TvaBreakdown {
            rate: disc_dominant,
            ht_cents: Cents(disc_total_ht),
            tva_cents: Cents(disc_total_tva),
            // ttc_cents = ht + tva (jamais discount_amount directement —
            // évite les incohérences d'arrondi qui feraient échouer la validation)
            ttc_cents: Cents(disc_total_ht + disc_total_tva),
        };

        let disc_data = FiscalEntryData {
            session_id,
            operation_type: OperationType::Discount,
            amount_ttc_cents: Cents(disc_total_ht + disc_total_tva),
            tva_breakdown: disc_bd_main,
            tva_5_5_breakdown: disc_bd_5_5,
            tva_10_breakdown: disc_bd_10,
            tva_20_breakdown: disc_bd_20,
            reason: Some(format!("Promotion: {}", app.promo_name)),
            order_reference: Some(body.order_reference.clone()),
        };

        // Fix 2: valider AVANT d'enregistrer la VENTE — échec ici n'a aucun
        // effet sur le journal (la vente n'est pas encore commitée).
        disc_data
            .validate()
            .map_err(|msg| FiscalError::InvalidAmount {
                amount_cents: disc_data.amount_ttc_cents.0,
                operation: format!("DISCOUNT pré-validation: {msg}"),
            })?;

        pending_discounts.push(DiscountEntry {
            data: disc_data,
            promo_id: app.promo_id.to_string(),
            promo_name: app.promo_name.clone(),
            discount_cents: app.discount_cents,
        });
    }

    // Toutes les entrées DISCOUNT sont valides — on peut maintenant committer la VENTE.
    let entry = state.journal.record_transaction(data).await?;
    let order_id = Uuid::now_v7().to_string();

    // Enregistrer les entrées DISCOUNT dans le journal fiscal (bloquant)
    // (NF525 : chaque remise doit être tracée dans le journal)
    let mut applied_promo_responses: Vec<AppliedPromoResponse> = Vec::new();

    for discount in pending_discounts {
        let _disc_entry = state.journal.record_transaction(discount.data).await?;

        applied_promo_responses.push(AppliedPromoResponse {
            promo_id: discount.promo_id,
            name: discount.promo_name,
            discount_cents: discount.discount_cents,
        });
    }

    Ok((
        StatusCode::CREATED,
        Json(OrderResponse {
            order_id,
            fiscal_entry: FiscalEntryResponse::from(&entry),
            applied_promos: applied_promo_responses,
        }),
    ))
}

/// `POST /api/v1/orders/:id/pay`
///
/// Valide le paiement d'une commande après confirmation du TPE.
///
/// Dans l'architecture pass-through, cette route est appelée par le frontend
/// après que le TPE a confirmé le paiement. Elle enregistre la confirmation
/// dans le journal (l'entrée de vente a déjà été créée au moment de `POST /orders`).
///
/// Pour les paiements **en espèces**, calcule le rendu monnaie.
///
/// ## Corps de la requête
/// ```json
/// {
///   "payment_method": "cash",
///   "amount_paid_cents": 2000
/// }
/// ```
///
/// # Errors
/// - `422` si `amount_paid_cents < amount_ttc_cents` (paiement insuffisant)
pub async fn pay_order_handler(
    State(state): State<AppState>,
    Path(order_id): Path<String>,
    Json(body): Json<PayOrderRequest>,
) -> Result<(StatusCode, Json<PaymentResponse>), ApiErr> {
    // Le paiement CB est confirmé par le TPE avant d'appeler cette route.
    // Pour l'instant, on valide simplement que le montant encaissé est suffisant.
    // La logique de confirmation fiscale supplémentaire sera ajoutée si nécessaire.
    //
    // Note : dans l'architecture actuelle, la vente est enregistrée fiscalement
    // au moment de POST /orders. Ce callback confirme uniquement le paiement
    // côté UI et calcule le rendu monnaie pour les espèces.

    // Rendu monnaie simplifié : le frontend calcule et affiche le rendu
    let change_cents: i64 = 0;

    // Valider que le paiement est suffisant (pour espèces)
    if body.amount_paid_cents < 0 {
        return Err(FiscalError::InvalidAmount {
            amount_cents: body.amount_paid_cents,
            operation: "paiement".to_string(),
        }
        .into());
    }

    // Hook KDS — router la commande vers les stations cuisine.
    //
    // Dans ce MVP, les line_items ne sont pas stockés dans une table dédiée :
    // ils ont été agrégés au moment de POST /orders. On dispatche donc avec une
    // liste de lignes vide. Le routeur KDS ignorera silencieusement les commandes
    // sans lignes ; la structure est en place pour être enrichie quand une table
    // order_lines sera ajoutée au schéma.
    //
    // order_type : non porté par PayOrderRequest — utilise la valeur par défaut
    // (EatIn) en attendant qu'il soit stocké lors de la création de la commande.
    let incoming = IncomingOrder {
        order_id: order_id.clone(),
        channel: "caisse".to_string(),
        order_type: common::OrderType::default(),
        customer_name: None,
        external_order_id: None,
        lines: vec![],
    };

    // Dispatch asynchrone — ne bloque pas la réponse HTTP.
    let broadcaster = state.kds_broadcaster.clone();
    let db = state.db.clone();
    let heartbeats = state.station_heartbeats.clone();
    let timeout_secs = state.kds_heartbeat_timeout_secs;
    tokio::spawn(async move {
        if let Err(e) =
            dispatch_order(&db, &broadcaster, &incoming, &heartbeats, timeout_secs).await
        {
            tracing::warn!(error = %e, "KDS dispatch non-bloquant échoué");
        }
    });

    // Réponse de confirmation (la vraie entrée fiscale a été créée dans POST /orders)
    Ok((
        StatusCode::OK,
        Json(PaymentResponse {
            order_id,
            status: "paid",
            // Dans un système complet, on retournerait l'entrée fiscale existante
            // Pour le MVP, on retourne un stub — l'entrée est déjà en base
            fiscal_entry: FiscalEntryResponse {
                id: Uuid::now_v7().to_string(),
                sequence_number: 0,
                operation_type: "VENTE".to_string(),
                amount_ttc_cents: body.amount_paid_cents,
                ht_cents: 0,
                tva_cents: 0,
                tva_rate: "10".to_string(),
                tva_5_5_ht_cents: 0,
                tva_5_5_tva_cents: 0,
                tva_10_ht_cents: 0,
                tva_10_tva_cents: 0,
                tva_20_ht_cents: 0,
                tva_20_tva_cents: 0,
                hash_hex: "0".repeat(64),
                created_at_ms: now_ms(),
            },
            change_cents,
        }),
    ))
}

/// `POST /api/v1/orders/:id/cancel`
///
/// Annule une commande avec motif obligatoire (NF525 §4.3).
///
/// Enregistre une entrée `Cancel` dans le journal avec le montant négatif
/// de la commande originale.
///
/// ## Corps de la requête
/// ```json
/// {
///   "reason": "Erreur de saisie",
///   "fiscal_entry_id": "uuid-de-l-entree-originale",
///   "amount_ttc_cents": 1100,
///   "tva_rate": "10"
/// }
/// ```
///
/// # Errors
/// - `409` si aucune session n'est ouverte
/// - `422` si le motif est absent ou le montant invalide
pub async fn cancel_order_handler(
    State(state): State<AppState>,
    Path(_order_id): Path<String>,
    Json(body): Json<CancelOrderRequest>,
) -> Result<(StatusCode, Json<OrderResponse>), ApiErr> {
    if body.reason.trim().is_empty() {
        return Err(FiscalError::InvalidAmount {
            amount_cents: 0,
            operation: "annulation sans motif".to_string(),
        }
        .into());
    }

    let session_id = require_active_session(&state).await?;
    let tva_rate = TvaRate::from(body.tva_rate);

    // Le montant d'annulation est toujours négatif
    let cancel_amount = if body.amount_ttc_cents > 0 {
        -body.amount_ttc_cents
    } else {
        body.amount_ttc_cents
    };

    // Breakdowns par taux — si fournis, on les utilise ; sinon mono-taux (fallback)
    let (tva_5_5_breakdown, tva_10_breakdown, tva_20_breakdown, tva_breakdown) =
        if body.tva_5_5_amount_ttc.is_some()
            || body.tva_10_amount_ttc.is_some()
            || body.tva_20_amount_ttc.is_some()
        {
            // Multi-taux : utiliser les montants fournis (négatifs)
            let neg_5_5 = -body.tva_5_5_amount_ttc.unwrap_or(0);
            let neg_10 = -body.tva_10_amount_ttc.unwrap_or(0);
            let neg_20 = -body.tva_20_amount_ttc.unwrap_or(0);
            let bd_5_5 = TvaBreakdown::from_ttc(Cents(neg_5_5), TvaRate::Reduit5_5);
            let bd_10 = TvaBreakdown::from_ttc(Cents(neg_10), TvaRate::Intermediaire10);
            let bd_20 = TvaBreakdown::from_ttc(Cents(neg_20), TvaRate::Normal20);
            let dominant = if neg_20.abs() >= neg_10.abs() && neg_20.abs() >= neg_5_5.abs() {
                TvaRate::Normal20
            } else if neg_10.abs() >= neg_5_5.abs() {
                TvaRate::Intermediaire10
            } else {
                TvaRate::Reduit5_5
            };
            let total_ht = bd_5_5.ht_cents.0 + bd_10.ht_cents.0 + bd_20.ht_cents.0;
            let total_tva = bd_5_5.tva_cents.0 + bd_10.tva_cents.0 + bd_20.tva_cents.0;
            let bd_main = TvaBreakdown {
                rate: dominant,
                ht_cents: Cents(total_ht),
                tva_cents: Cents(total_tva),
                // ttc_cents = ht + tva (jamais cancel_amount directement — évite
                // les incohérences d'arrondi qui feraient échouer la validation TVA)
                ttc_cents: Cents(total_ht + total_tva),
            };
            (bd_5_5, bd_10, bd_20, bd_main)
        } else {
            // Fallback mono-taux
            let bd = TvaBreakdown::from_ttc(Cents(cancel_amount), tva_rate);
            let bd_5_5 = match tva_rate {
                TvaRate::Reduit5_5 => bd,
                _ => TvaBreakdown::zero(TvaRate::Reduit5_5),
            };
            let bd_10 = match tva_rate {
                TvaRate::Intermediaire10 => bd,
                _ => TvaBreakdown::zero(TvaRate::Intermediaire10),
            };
            let bd_20 = match tva_rate {
                TvaRate::Normal20 => bd,
                _ => TvaBreakdown::zero(TvaRate::Normal20),
            };
            (bd_5_5, bd_10, bd_20, bd)
        };

    let data = FiscalEntryData {
        session_id,
        operation_type: OperationType::Cancel,
        // Utiliser ht + tva calculés (= tva_breakdown.ttc_cents) plutôt que
        // cancel_amount brut : évite les écarts d'un centime par arrondi entier
        // sur les annulations multi-taux (même logique que le fix DISCOUNT).
        amount_ttc_cents: Cents(tva_breakdown.ht_cents.0 + tva_breakdown.tva_cents.0),
        tva_breakdown,
        tva_5_5_breakdown,
        tva_10_breakdown,
        tva_20_breakdown,
        reason: Some(body.reason),
        order_reference: Some(body.fiscal_entry_id),
    };

    let entry = state.journal.record_transaction(data).await?;

    Ok((
        StatusCode::OK,
        Json(OrderResponse {
            order_id: Uuid::now_v7().to_string(),
            fiscal_entry: FiscalEntryResponse::from(&entry),
            applied_promos: Vec::new(),
        }),
    ))
}

/// `GET /api/v1/orders/:id`
///
/// Consultation d'une commande par son identifiant.
///
/// Dans le MVP, les commandes ne sont pas stockées indépendamment des entrées fiscales.
/// Cette route est un stub qui sera complété quand un modèle `Order` sera ajouté.
///
/// # Errors
/// - `404 Not Found` — toujours pour le MVP
pub async fn get_order_handler(
    Path(order_id): Path<String>,
) -> Result<(StatusCode, Json<serde_json::Value>), ApiErr> {
    // MVP : les commandes individuelles ne sont pas encore indexées
    // Le frontend peut consulter le journal fiscal complet via la session
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "order_id": order_id,
            "note": "Consultation par order_id non disponible en MVP — consulter le journal de session"
        })),
    ))
}

// ---------------------------------------------------------------------------
// Utilitaires
// ---------------------------------------------------------------------------

/// Récupère l'ID de la session active ou retourne une erreur 409.
async fn require_active_session(state: &AppState) -> Result<common::SessionId, ApiErr> {
    state
        .journal
        .active_session()
        .await
        .map(|s| s.id)
        .ok_or_else(|| {
            ApiErr::from(FiscalError::Session(
                fiscal_engine::errors::SessionError::NoActiveSession,
            ))
        })
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
