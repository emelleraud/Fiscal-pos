use serde::{Deserialize, Serialize};
use time::{Date, Time};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cart {
    pub line_items: Vec<CartItem>,
    pub total_ttc_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CartItem {
    pub sku: String,
    pub amount_ttc_cents: i64,
    pub tva_rate: TvaRateKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TvaRateKey {
    Reduit5_5,
    Intermediaire10,
    Normal20,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Promotion {
    pub id: Uuid,
    pub name: String,
    pub promo_type: PromoType,
    pub value_cents: Option<i64>,
    pub value_bps: Option<i64>,
    pub target_sku: Option<String>,
    pub trigger: Trigger,
    pub exclusion_group: Option<String>,
    pub priority: i32,
    pub valid_from: Option<Date>,
    pub valid_to: Option<Date>,
    pub days_of_week: Option<Vec<u8>>,  // 1=Lun, 7=Dim
    pub time_from: Option<Time>,
    pub time_to: Option<Time>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromoType {
    FixedAmount,
    Percentage,
    ItemDiscount,
    Bogo,
    HappyHour,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Trigger {
    Auto,
    Manual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalResult {
    pub applied: Vec<PromoApplication>,
    pub rejected: Vec<PromoApplication>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromoApplication {
    pub promo_id: Uuid,
    pub promo_name: String,
    pub discount_cents: i64,        // toujours positif
    pub tva_allocation: TvaAllocation,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TvaAllocation {
    pub cents_5_5: i64,
    pub cents_10:  i64,
    pub cents_20:  i64,
}

impl TvaAllocation {
    #[must_use]
    pub fn total(&self) -> i64 {
        self.cents_5_5 + self.cents_10 + self.cents_20
    }
}
