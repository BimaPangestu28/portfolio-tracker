use crate::domain::cost_basis::CostBasis;
use crate::domain::models::Transaction;
use rust_decimal::Decimal;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Position {
    pub instrument_id: i64,
    #[serde(with = "rust_decimal::serde::str")]
    pub quantity: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub avg_cost: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub cost_basis_total: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub latest_price: Decimal,
    pub price_stale: bool,
    #[serde(with = "rust_decimal::serde::str")]
    pub market_value_native: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub market_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub market_value_usd: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_pnl: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub income: Decimal,
}

/// Latest price + FX context for one instrument at valuation time.
#[derive(Debug, Clone)]
pub struct PriceContext {
    #[allow(dead_code)] // identifies the instrument this context is for; not read during valuation
    pub instrument_id: i64,
    pub latest_price_native: Decimal,
    pub price_stale: bool,
    pub fx_native_to_idr: Decimal,
    pub fx_native_to_usd: Decimal,
}

pub fn build_position(instrument_id: i64, cb: &CostBasis, ctx: &PriceContext) -> Position {
    let mv_native = cb.quantity * ctx.latest_price_native;
    Position {
        instrument_id,
        quantity: cb.quantity,
        avg_cost: cb.avg_cost,
        cost_basis_total: cb.cost_basis_total,
        latest_price: ctx.latest_price_native,
        price_stale: ctx.price_stale,
        market_value_native: mv_native,
        market_value_idr: mv_native * ctx.fx_native_to_idr,
        market_value_usd: mv_native * ctx.fx_native_to_usd,
        unrealized_pnl: mv_native - cb.cost_basis_total,
        realized_pnl: cb.realized_pnl,
        income: cb.income,
    }
}

/// Group transactions by instrument id, preserving chronological order within each group.
pub fn group_by_instrument(
    txns: Vec<Transaction>,
) -> std::collections::BTreeMap<i64, Vec<Transaction>> {
    let mut map: std::collections::BTreeMap<i64, Vec<Transaction>> =
        std::collections::BTreeMap::new();
    for t in txns {
        map.entry(t.instrument_id).or_default().push(t);
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::cost_basis::compute_cost_basis;
    use crate::domain::models::TxnType;
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

    fn tx(t: TxnType, qty: Decimal, price: Decimal) -> Transaction {
        Transaction {
            id: 0,
            account_id: 1,
            instrument_id: 7,
            txn_type: t,
            executed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            quantity: qty,
            price_native: price,
            fee_native: dec!(0),
            currency: "USD".into(),
            fx_to_idr: dec!(16000),
            fx_to_usd: dec!(1),
            note: None,
        }
    }

    #[test]
    fn position_values_in_dual_currency() {
        let txns = vec![tx(TxnType::Buy, dec!(2), dec!(100))];
        let cb = compute_cost_basis(&txns);
        let ctx = PriceContext {
            instrument_id: 7,
            latest_price_native: dec!(150),
            price_stale: false,
            fx_native_to_idr: dec!(16000),
            fx_native_to_usd: dec!(1),
        };
        let p = build_position(7, &cb, &ctx);
        assert_eq!(p.market_value_native, dec!(300));
        assert_eq!(p.market_value_usd, dec!(300));
        assert_eq!(p.market_value_idr, dec!(4800000));
        assert_eq!(p.unrealized_pnl, dec!(100));
    }
}
