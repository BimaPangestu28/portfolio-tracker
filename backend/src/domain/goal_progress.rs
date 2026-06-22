use crate::domain::models::{Transaction, TxnType};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GoalProgress {
    pub market_value_idr: Decimal,
    pub invested_idr: Decimal,
    pub gain_loss_idr: Decimal,
}

/// Compute a goal's progress from its tagged transactions.
///
/// - net units per instrument = Σ(buy qty) − Σ(sell qty)
/// - market_value_idr = Σ net_units × current_price_idr (0 for instruments absent from `price_idr`)
/// - invested_idr = Σ buy cost incl. fee − Σ sell proceeds net of fee, each × the txn's fx_to_idr
/// - gain_loss_idr = market − invested
///
/// Only Buy/Sell contribute; other txn types tagged to a goal are ignored.
/// Cost basis is net-cash (not FIFO) — a documented approximation.
pub fn compute_goal_progress(txns: &[Transaction], price_idr: &HashMap<i64, Decimal>) -> GoalProgress {
    let mut net_units: HashMap<i64, Decimal> = HashMap::new();
    let mut invested = Decimal::ZERO;
    for t in txns {
        match t.txn_type {
            TxnType::Buy => {
                *net_units.entry(t.instrument_id).or_default() += t.quantity;
                invested += (t.quantity * t.price_native + t.fee_native) * t.fx_to_idr;
            }
            TxnType::Sell => {
                *net_units.entry(t.instrument_id).or_default() -= t.quantity;
                invested -= (t.quantity * t.price_native - t.fee_native) * t.fx_to_idr;
            }
            _ => {}
        }
    }
    let market_value_idr: Decimal = net_units
        .iter()
        .map(|(iid, units)| price_idr.get(iid).copied().unwrap_or(Decimal::ZERO) * *units)
        .sum();
    GoalProgress {
        market_value_idr,
        invested_idr: invested,
        gain_loss_idr: market_value_idr - invested,
    }
}

/// Whole months from `now` to `target`, clamped to at least 1 (a past or
/// same-month target still shows the full remaining amount as "this month").
pub fn months_until(now: NaiveDate, target: NaiveDate) -> i64 {
    use chrono::Datelike;
    let months = (target.year() - now.year()) * 12 + (target.month() as i32 - now.month() as i32);
    (months as i64).max(1)
}

/// Monthly contribution needed to reach the target from the current value over
/// `months_left` months; 0 once the target is already met.
pub fn required_monthly(target_idr: Decimal, current_idr: Decimal, months_left: i64) -> Decimal {
    let remaining = target_idr - current_idr;
    if remaining <= Decimal::ZERO || months_left <= 0 {
        return Decimal::ZERO;
    }
    remaining / Decimal::from(months_left)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Transaction, TxnType};
    use chrono::{NaiveDate, TimeZone, Utc};
    use rust_decimal::Decimal;
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    fn txn(id: i64, instrument_id: i64, kind: TxnType, qty: Decimal, price: Decimal, fee: Decimal, fx_to_idr: Decimal) -> Transaction {
        Transaction {
            id, account_id: 1, instrument_id, txn_type: kind,
            executed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            quantity: qty, price_native: price, fee_native: fee,
            currency: "IDR".into(), fx_to_idr, fx_to_usd: dec!(1), note: None,
        }
    }

    #[test]
    fn buy_only_market_invested_and_gain() {
        // Buy 100 @ 9000 (+10 fee), IDR. Current price 9500 IDR.
        let txns = vec![txn(1, 7, TxnType::Buy, dec!(100), dec!(9000), dec!(10), dec!(1))];
        let mut price = HashMap::new();
        price.insert(7, dec!(9500));
        let p = compute_goal_progress(&txns, &price);
        assert_eq!(p.market_value_idr, dec!(950000));        // 100 * 9500
        assert_eq!(p.invested_idr, dec!(900010));            // 100*9000 + 10
        assert_eq!(p.gain_loss_idr, dec!(49990));            // 950000 - 900010
    }

    #[test]
    fn buy_then_partial_sell_nets_units_and_invested() {
        // Buy 100 @ 9000; Sell 40 @ 9500 (−5 fee). Current price 9500.
        let txns = vec![
            txn(1, 7, TxnType::Buy, dec!(100), dec!(9000), dec!(0), dec!(1)),
            txn(2, 7, TxnType::Sell, dec!(40), dec!(9500), dec!(5), dec!(1)),
        ];
        let mut price = HashMap::new();
        price.insert(7, dec!(9500));
        let p = compute_goal_progress(&txns, &price);
        assert_eq!(p.market_value_idr, dec!(570000));        // (100-40)=60 * 9500
        // invested = buy 900000 - sell proceeds (40*9500 - 5 = 379995) = 520005
        assert_eq!(p.invested_idr, dec!(520005));
        assert_eq!(p.gain_loss_idr, dec!(49995));            // 570000 - 520005
    }

    #[test]
    fn multi_instrument_sums_and_missing_price_is_zero() {
        let txns = vec![
            txn(1, 7, TxnType::Buy, dec!(10), dec!(100), dec!(0), dec!(1)),
            txn(2, 8, TxnType::Buy, dec!(5),  dec!(200), dec!(0), dec!(1)),
        ];
        let mut price = HashMap::new();
        price.insert(7, dec!(150)); // instrument 8 has no price -> contributes 0 to market value
        let p = compute_goal_progress(&txns, &price);
        assert_eq!(p.market_value_idr, dec!(1500));          // 10*150 + 5*0
        assert_eq!(p.invested_idr, dec!(2000));              // 1000 + 1000
    }

    #[test]
    fn months_until_is_clamped_to_one() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 22).unwrap();
        assert_eq!(months_until(now, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()), 1); // past -> 1
        assert_eq!(months_until(now, NaiveDate::from_ymd_opt(2026, 6, 30).unwrap()), 1); // same month -> 1
        assert_eq!(months_until(now, NaiveDate::from_ymd_opt(2027, 6, 22).unwrap()), 12);
    }

    #[test]
    fn required_monthly_is_remaining_over_months() {
        assert_eq!(required_monthly(dec!(200000000), dec!(80000000), 12), dec!(10000000));
        assert_eq!(required_monthly(dec!(100), dec!(150), 5), dec!(0)); // already met -> 0
    }
}
