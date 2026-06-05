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
    #[serde(with = "rust_decimal::serde::str")]
    pub cost_basis_idr_total: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_fx_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_fx_pnl_idr: Decimal,
    /// True when some txns had no usable purchase-time FX rate (see service::portfolio's resolve_fx_gaps), making the IDR decomposition for this position incomplete.
    pub fx_incomplete: bool,
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

pub fn build_position(instrument_id: i64, cb: &CostBasis, ctx: &PriceContext, fx_incomplete: bool) -> Position {
    let mv_native = cb.quantity * ctx.latest_price_native;
    let mv_idr = mv_native * ctx.fx_native_to_idr;
    // FX-aware unrealized P&L: current value at today's rate minus the purchase-rate
    // basis. Price component is the native P&L at today's rate; FX is the residual,
    // so price + fx == total exactly.
    let unrealized_idr = mv_idr - cb.cost_basis_idr_total;
    let unrealized_price_idr = (mv_native - cb.cost_basis_total) * ctx.fx_native_to_idr;
    Position {
        instrument_id,
        quantity: cb.quantity,
        avg_cost: cb.avg_cost,
        cost_basis_total: cb.cost_basis_total,
        latest_price: ctx.latest_price_native,
        price_stale: ctx.price_stale,
        market_value_native: mv_native,
        market_value_idr: mv_idr,
        market_value_usd: mv_native * ctx.fx_native_to_usd,
        unrealized_pnl: mv_native - cb.cost_basis_total,
        realized_pnl: cb.realized_pnl,
        income: cb.income,
        cost_basis_idr_total: cb.cost_basis_idr_total,
        unrealized_pnl_idr: unrealized_idr,
        unrealized_price_pnl_idr: unrealized_price_idr,
        unrealized_fx_pnl_idr: unrealized_idr - unrealized_price_idr,
        realized_pnl_idr: cb.realized_pnl_idr,
        realized_price_pnl_idr: cb.realized_price_pnl_idr,
        realized_fx_pnl_idr: cb.realized_fx_pnl_idr,
        fx_incomplete,
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
    fn position_decomposes_unrealized_pnl_into_price_and_fx() {
        // Buy 2 @ 100 at fx 16000 (cost 3.2M IDR). Now price 150, fx 17000:
        //   mv_idr = 300*17000 = 5,100,000 ; total = 1,900,000
        //   price  = (300-200)*17000 = 1,700,000 ; fx = 200,000
        let txns = vec![tx(TxnType::Buy, dec!(2), dec!(100))];
        let cb = compute_cost_basis(&txns);
        let ctx = PriceContext {
            instrument_id: 7,
            latest_price_native: dec!(150),
            price_stale: false,
            fx_native_to_idr: dec!(17000),
            fx_native_to_usd: dec!(1),
        };
        let p = build_position(7, &cb, &ctx, false);
        assert_eq!(p.cost_basis_idr_total, dec!(3200000));
        assert_eq!(p.unrealized_pnl_idr, dec!(1900000));
        assert_eq!(p.unrealized_price_pnl_idr, dec!(1700000));
        assert_eq!(p.unrealized_fx_pnl_idr, dec!(200000));
        assert_eq!(p.unrealized_pnl_idr, p.unrealized_price_pnl_idr + p.unrealized_fx_pnl_idr);
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
        let p = build_position(7, &cb, &ctx, false);
        assert_eq!(p.market_value_native, dec!(300));
        assert_eq!(p.market_value_usd, dec!(300));
        assert_eq!(p.market_value_idr, dec!(4800000));
        assert_eq!(p.unrealized_pnl, dec!(100));
    }
}
