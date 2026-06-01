use std::collections::HashMap;

use time::{OffsetDateTime, Weekday};
use uuid::Uuid;

use crate::types::{
    Cart, EvalResult, PromoApplication, PromoType, Promotion, Trigger, TvaAllocation, TvaRateKey,
};

#[must_use]
pub fn evaluate(
    cart: &Cart,
    promos: &[Promotion],
    manual_selected_ids: &[Uuid],
    now: OffsetDateTime,
) -> EvalResult {
    let candidates: Vec<(&Promotion, PromoApplication)> = promos
        .iter()
        .filter(|p| is_in_window(p, now))
        .filter(|p| match p.trigger {
            Trigger::Auto => true,
            Trigger::Manual => manual_selected_ids.contains(&p.id),
        })
        .filter(|p| cart_conditions_met(p, cart))
        .filter_map(|p| compute_discount(p, cart).map(|app| (p, app)))
        .collect();

    resolve_exclusion_groups(candidates)
}

fn is_in_window(p: &Promotion, now: OffsetDateTime) -> bool {
    let today = now.date();
    let t = now.time();
    let dow = weekday_num(now.weekday());

    if let Some(from) = p.valid_from {
        if today < from {
            return false;
        }
    }
    if let Some(to) = p.valid_to {
        if today > to {
            return false;
        }
    }

    if let Some(days) = &p.days_of_week {
        if !days.is_empty() && !days.contains(&dow) {
            return false;
        }
    }

    match (p.time_from, p.time_to) {
        (Some(from), Some(to)) if from <= to => {
            if t < from || t > to {
                return false;
            }
        }
        (Some(from), Some(to)) => {
            // Plage chevauchant minuit (ex: 22h-02h)
            if t < from && t > to {
                return false;
            }
        }
        (Some(from), None) => {
            if t < from {
                return false;
            }
        }
        (None, Some(to)) => {
            if t > to {
                return false;
            }
        }
        (None, None) => {}
    }

    true
}

fn weekday_num(w: Weekday) -> u8 {
    match w {
        Weekday::Monday => 1,
        Weekday::Tuesday => 2,
        Weekday::Wednesday => 3,
        Weekday::Thursday => 4,
        Weekday::Friday => 5,
        Weekday::Saturday => 6,
        Weekday::Sunday => 7,
    }
}

fn cart_conditions_met(p: &Promotion, cart: &Cart) -> bool {
    match p.promo_type {
        PromoType::ItemDiscount => p
            .target_sku
            .as_ref()
            .is_some_and(|sku| cart.line_items.iter().any(|i| &i.sku == sku)),
        PromoType::Bogo => p
            .target_sku
            .as_ref()
            .is_some_and(|sku| cart.line_items.iter().filter(|i| &i.sku == sku).count() >= 2),
        _ => !cart.line_items.is_empty(),
    }
}

fn compute_discount(p: &Promotion, cart: &Cart) -> Option<PromoApplication> {
    let discount_cents = match p.promo_type {
        PromoType::FixedAmount | PromoType::HappyHour if p.value_cents.is_some() => {
            p.value_cents?.min(cart.total_ttc_cents)
        }
        PromoType::Percentage | PromoType::HappyHour => {
            let bps = p.value_bps?;
            (cart.total_ttc_cents * bps / 10_000).min(cart.total_ttc_cents)
        }
        PromoType::FixedAmount => p.value_cents?.min(cart.total_ttc_cents),
        PromoType::ItemDiscount => {
            let sku = p.target_sku.as_ref()?;
            let total: i64 = cart
                .line_items
                .iter()
                .filter(|i| &i.sku == sku)
                .map(|i| i.amount_ttc_cents)
                .sum();
            if let Some(c) = p.value_cents {
                c.min(total)
            } else {
                let bps = p.value_bps?;
                (total * bps / 10_000).min(total)
            }
        }
        PromoType::Bogo => {
            let sku = p.target_sku.as_ref()?;
            cart.line_items
                .iter()
                .filter(|i| &i.sku == sku)
                .map(|i| i.amount_ttc_cents)
                .min()?
        }
    };

    if discount_cents <= 0 {
        return None;
    }

    Some(PromoApplication {
        promo_id: p.id,
        promo_name: p.name.clone(),
        discount_cents,
        tva_allocation: compute_tva_allocation(discount_cents, cart),
    })
}

fn compute_tva_allocation(discount_cents: i64, cart: &Cart) -> TvaAllocation {
    let total = cart.total_ttc_cents;
    if total == 0 {
        return TvaAllocation::default();
    }

    let mut by_rate: HashMap<TvaRateKey, i64> = HashMap::new();
    for item in &cart.line_items {
        *by_rate.entry(item.tva_rate).or_default() += item.amount_ttc_cents;
    }

    let alloc = |key: TvaRateKey| -> i64 {
        by_rate.get(&key).copied().unwrap_or(0) * discount_cents / total
    };

    let a5 = alloc(TvaRateKey::Reduit5_5);
    let a10 = alloc(TvaRateKey::Intermediaire10);
    let a20 = alloc(TvaRateKey::Normal20);
    let remainder = discount_cents - a5 - a10 - a20;

    // Remainder au taux dominant (évite de perdre des centimes par arrondi)
    let dominant = [
        (
            TvaRateKey::Reduit5_5,
            by_rate.get(&TvaRateKey::Reduit5_5).copied().unwrap_or(0),
        ),
        (
            TvaRateKey::Intermediaire10,
            by_rate
                .get(&TvaRateKey::Intermediaire10)
                .copied()
                .unwrap_or(0),
        ),
        (
            TvaRateKey::Normal20,
            by_rate.get(&TvaRateKey::Normal20).copied().unwrap_or(0),
        ),
    ]
    .into_iter()
    .max_by_key(|(_, v)| *v)
    .map_or(TvaRateKey::Intermediaire10, |(k, _)| k);

    let mut result = TvaAllocation {
        cents_5_5: a5,
        cents_10: a10,
        cents_20: a20,
    };
    match dominant {
        TvaRateKey::Reduit5_5 => result.cents_5_5 += remainder,
        TvaRateKey::Intermediaire10 => result.cents_10 += remainder,
        TvaRateKey::Normal20 => result.cents_20 += remainder,
    }
    result
}

fn resolve_exclusion_groups(candidates: Vec<(&Promotion, PromoApplication)>) -> EvalResult {
    let mut grouped: HashMap<String, Vec<(&Promotion, PromoApplication)>> = HashMap::new();
    let mut applied: Vec<PromoApplication> = Vec::new();
    let mut rejected: Vec<PromoApplication> = Vec::new();

    for (promo, app) in candidates {
        if let Some(grp) = &promo.exclusion_group {
            grouped.entry(grp.clone()).or_default().push((promo, app));
        } else {
            applied.push(app);
        }
    }

    for (_grp, mut group) in grouped {
        group.sort_by(|a, b| {
            b.0.priority
                .cmp(&a.0.priority)
                .then_with(|| b.1.discount_cents.cmp(&a.1.discount_cents))
        });
        let mut iter = group.into_iter();
        if let Some((_p, winner)) = iter.next() {
            applied.push(winner);
        }
        for (_p, loser) in iter {
            rejected.push(loser);
        }
    }

    EvalResult { applied, rejected }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CartItem, PromoType, Trigger, TvaRateKey};
    use time::macros::{date, datetime, time};
    use uuid::Uuid;

    fn uuid(b: u8) -> Uuid {
        Uuid::from_bytes([b; 16])
    }

    fn base_promo(b: u8) -> Promotion {
        Promotion {
            id: uuid(b),
            name: format!("P{b}"),
            promo_type: PromoType::FixedAmount,
            value_cents: Some(100),
            value_bps: None,
            target_sku: None,
            trigger: Trigger::Auto,
            exclusion_group: None,
            priority: 0,
            valid_from: None,
            valid_to: None,
            days_of_week: None,
            time_from: None,
            time_to: None,
        }
    }

    fn cart_1000() -> Cart {
        Cart {
            line_items: vec![CartItem {
                sku: "SKU-A".into(),
                amount_ttc_cents: 1000,
                tva_rate: TvaRateKey::Intermediaire10,
            }],
            total_ttc_cents: 1000,
        }
    }

    #[test]
    fn promo_after_valid_to_rejected() {
        let p = Promotion {
            valid_to: Some(date!(2020 - 01 - 01)),
            ..base_promo(1)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_before_valid_from_rejected() {
        let p = Promotion {
            valid_from: Some(date!(2099 - 01 - 01)),
            ..base_promo(2)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_in_date_range_applied() {
        let p = Promotion {
            valid_from: Some(date!(2026 - 01 - 01)),
            valid_to: Some(date!(2026 - 12 - 31)),
            ..base_promo(3)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn promo_wrong_day_rejected() {
        // 2026-06-01 = lundi (1) ; promo sam+dim seulement
        let p = Promotion {
            days_of_week: Some(vec![6, 7]),
            ..base_promo(4)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-06-01 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_correct_day_applied() {
        // 2026-06-01 = lundi (1)
        let p = Promotion {
            days_of_week: Some(vec![1]),
            ..base_promo(5)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-06-01 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn promo_outside_time_range_rejected() {
        let p = Promotion {
            time_from: Some(time!(16:00)),
            time_to: Some(time!(18:00)),
            ..base_promo(6)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 14:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn promo_inside_time_range_applied() {
        let p = Promotion {
            time_from: Some(time!(16:00)),
            time_to: Some(time!(18:00)),
            ..base_promo(7)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 17:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn manual_promo_without_selection_rejected() {
        let p = Promotion {
            trigger: Trigger::Manual,
            ..base_promo(8)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn manual_promo_selected_applied() {
        let p = Promotion {
            trigger: Trigger::Manual,
            ..base_promo(9)
        };
        let id = p.id;
        let r = evaluate(&cart_1000(), &[p], &[id], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied.len(), 1);
    }

    #[test]
    fn fixed_amount_discount_applied() {
        let p = Promotion {
            value_cents: Some(200),
            ..base_promo(10)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 200);
    }

    #[test]
    fn fixed_amount_capped_at_cart_total() {
        let p = Promotion {
            value_cents: Some(9999),
            ..base_promo(11)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 1000);
    }

    #[test]
    fn percentage_10_percent() {
        let p = Promotion {
            promo_type: PromoType::Percentage,
            value_cents: None,
            value_bps: Some(1000), // 10%
            ..base_promo(12)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 100);
    }

    #[test]
    fn item_discount_sku_absent_rejected() {
        let p = Promotion {
            promo_type: PromoType::ItemDiscount,
            target_sku: Some("ABSENT".into()),
            value_cents: Some(100),
            ..base_promo(13)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(r.applied.is_empty());
    }

    #[test]
    fn item_discount_sku_present_applied() {
        let p = Promotion {
            promo_type: PromoType::ItemDiscount,
            target_sku: Some("SKU-A".into()), // SKU-A exists in cart_1000()
            value_cents: Some(150),
            ..base_promo(25)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 150);
    }

    #[test]
    fn bogo_gives_cheapest_item_free() {
        let cart = Cart {
            line_items: vec![
                CartItem {
                    sku: "BUR".into(),
                    amount_ttc_cents: 900,
                    tva_rate: TvaRateKey::Intermediaire10,
                },
                CartItem {
                    sku: "BUR".into(),
                    amount_ttc_cents: 1200,
                    tva_rate: TvaRateKey::Intermediaire10,
                },
            ],
            total_ttc_cents: 2100,
        };
        let p = Promotion {
            promo_type: PromoType::Bogo,
            target_sku: Some("BUR".into()),
            value_cents: None,
            ..base_promo(14)
        };
        let r = evaluate(&cart, &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 900); // cheapest item free
    }

    #[test]
    fn exclusion_group_highest_priority_wins() {
        let p1 = Promotion {
            exclusion_group: Some("g".into()),
            priority: 5,
            value_cents: Some(300),
            ..base_promo(15)
        };
        let p2 = Promotion {
            exclusion_group: Some("g".into()),
            priority: 10,
            value_cents: Some(50),
            ..base_promo(16)
        };
        let r = evaluate(
            &cart_1000(),
            &[p1, p2],
            &[],
            datetime!(2026-05-31 10:00 UTC),
        );
        assert_eq!(r.applied.len(), 1);
        assert_eq!(r.applied[0].discount_cents, 50); // p2 priority 10 wins
        assert_eq!(r.rejected.len(), 1);
    }

    #[test]
    fn exclusion_group_tie_biggest_discount_wins() {
        let p1 = Promotion {
            exclusion_group: Some("g".into()),
            priority: 0,
            value_cents: Some(200),
            ..base_promo(17)
        };
        let p2 = Promotion {
            exclusion_group: Some("g".into()),
            priority: 0,
            value_cents: Some(500),
            ..base_promo(18)
        };
        let r = evaluate(
            &cart_1000(),
            &[p1, p2],
            &[],
            datetime!(2026-05-31 10:00 UTC),
        );
        assert_eq!(r.applied.len(), 1);
        assert_eq!(r.applied[0].discount_cents, 500);
        assert_eq!(r.rejected.len(), 1);
        assert_eq!(r.rejected[0].discount_cents, 200);
    }

    #[test]
    fn no_group_all_cumulated() {
        let p1 = Promotion {
            value_cents: Some(100),
            ..base_promo(19)
        };
        let p2 = Promotion {
            value_cents: Some(200),
            ..base_promo(20)
        };
        let r = evaluate(
            &cart_1000(),
            &[p1, p2],
            &[],
            datetime!(2026-05-31 10:00 UTC),
        );
        assert_eq!(r.applied.len(), 2);
    }

    #[test]
    fn tva_allocation_single_rate_coherent() {
        let p = Promotion {
            value_cents: Some(100),
            ..base_promo(21)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        let a = &r.applied[0].tva_allocation;
        assert_eq!(a.total(), 100);
        assert_eq!(a.cents_5_5, 0);
        assert_eq!(a.cents_10, 100);
        assert_eq!(a.cents_20, 0);
    }

    #[test]
    fn tva_allocation_multi_rate_proportional() {
        // 600 @ 10% + 400 @ 20% = 1000 total ; discount 100
        let cart = Cart {
            line_items: vec![
                CartItem {
                    sku: "A".into(),
                    amount_ttc_cents: 600,
                    tva_rate: TvaRateKey::Intermediaire10,
                },
                CartItem {
                    sku: "B".into(),
                    amount_ttc_cents: 400,
                    tva_rate: TvaRateKey::Normal20,
                },
            ],
            total_ttc_cents: 1000,
        };
        let p = Promotion {
            value_cents: Some(100),
            ..base_promo(22)
        };
        let r = evaluate(&cart, &[p], &[], datetime!(2026-05-31 10:00 UTC));
        let a = &r.applied[0].tva_allocation;
        assert_eq!(a.total(), 100);
        assert_eq!(a.cents_10, 60);
        assert_eq!(a.cents_20, 40);
    }

    #[test]
    fn bogo_single_item_not_discounted() {
        // Only 1 item: BOGO requires >= 2 — should NOT apply
        let cart = Cart {
            line_items: vec![CartItem {
                sku: "BUR".into(),
                amount_ttc_cents: 900,
                tva_rate: TvaRateKey::Intermediaire10,
            }],
            total_ttc_cents: 900,
        };
        let p = Promotion {
            promo_type: PromoType::Bogo,
            target_sku: Some("BUR".into()),
            value_cents: None,
            ..base_promo(23)
        };
        let r = evaluate(&cart, &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert!(
            r.applied.is_empty(),
            "BOGO ne doit pas s'appliquer avec 1 seul article"
        );
    }

    #[test]
    fn happy_hour_percentage_applied() {
        let p = Promotion {
            promo_type: PromoType::HappyHour,
            value_cents: None,
            value_bps: Some(2000), // 20%
            ..base_promo(24)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 10:00 UTC));
        assert_eq!(r.applied[0].discount_cents, 200);
    }

    #[test]
    fn happy_hour_outside_window_rejected() {
        let p = Promotion {
            promo_type: PromoType::HappyHour,
            value_cents: None,
            value_bps: Some(2000),
            time_from: Some(time!(16:00)),
            time_to: Some(time!(18:00)),
            ..base_promo(26)
        };
        let r = evaluate(&cart_1000(), &[p], &[], datetime!(2026-05-31 14:00 UTC));
        assert!(
            r.applied.is_empty(),
            "HappyHour hors fenêtre horaire ne doit pas s'appliquer"
        );
    }
}
