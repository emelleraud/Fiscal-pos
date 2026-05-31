use time::OffsetDateTime;
use uuid::Uuid;

use crate::types::{Cart, EvalResult, Promotion};

#[must_use]
pub fn evaluate(
    _cart: &Cart,
    _promos: &[Promotion],
    _manual_selected_ids: &[Uuid],
    _now: OffsetDateTime,
) -> EvalResult {
    EvalResult { applied: vec![], rejected: vec![] }
}
