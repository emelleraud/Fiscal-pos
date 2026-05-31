#![deny(clippy::all, clippy::pedantic)]
#![allow(missing_docs)]

//! # promo-engine
//!
//! Évalue les promotions applicables à un panier de commande.
//! Zéro couplage avec `fiscal-engine`.

pub mod errors;
pub mod evaluator;
pub mod types;

pub use evaluator::evaluate;
pub use types::{
    Cart, CartItem, EvalResult, PromoApplication,
    Promotion, PromoType, Trigger, TvaAllocation, TvaRateKey,
};
