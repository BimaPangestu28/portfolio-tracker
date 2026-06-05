use crate::domain::models::{Transaction, TxnType};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct CostBasis {
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub cost_basis_total: Decimal,
    pub realized_pnl: Decimal,
    pub income: Decimal,
    /// Average cost per unit in IDR at purchase-time FX (`txn.fx_to_idr`).
    pub avg_cost_idr: Decimal,
    pub cost_basis_idr_total: Decimal,
    /// Realized P&L in IDR, decomposed: price (native P&L × FX at sell) + fx (residual).
    pub realized_pnl_idr: Decimal,
    pub realized_price_pnl_idr: Decimal,
    pub realized_fx_pnl_idr: Decimal,
}

/// Average-cost engine. `txns` MUST be sorted ascending by `executed_at`.
pub fn compute_cost_basis(txns: &[Transaction]) -> CostBasis {
    let mut qty = Decimal::ZERO;
    let mut avg = Decimal::ZERO;
    let mut avg_idr = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut realized_idr = Decimal::ZERO;
    let mut realized_price_idr = Decimal::ZERO;
    let mut realized_fx_idr = Decimal::ZERO;
    let mut income = Decimal::ZERO;

    for t in txns {
        match t.txn_type {
            TxnType::Buy | TxnType::OpeningBalance | TxnType::Deposit => {
                let added_cost = t.quantity * t.price_native + t.fee_native;
                let added_cost_idr = added_cost * t.fx_to_idr;
                let new_qty = qty + t.quantity;
                if new_qty.is_zero() {
                    avg = Decimal::ZERO;
                    avg_idr = Decimal::ZERO;
                } else {
                    avg = (qty * avg + added_cost) / new_qty;
                    avg_idr = (qty * avg_idr + added_cost_idr) / new_qty;
                }
                qty = new_qty;
            }
            TxnType::Sell | TxnType::Withdrawal => {
                // Cap the sold quantity at what is actually held. Overselling (recording a
                // sell larger than the current position) would otherwise realize P&L on
                // phantom units. We realize only on owned units and zero the position.
                // NOTE: surfacing the oversell as a user-facing validation error is a
                // Phase 3 (review-queue) concern; here we keep the math sound.
                let sold = if t.quantity > qty { qty } else { t.quantity };
                let realized_delta = (t.price_native - avg) * sold - t.fee_native;
                realized += realized_delta;
                // IDR realized: proceeds at the sell-time rate minus the purchase-rate
                // basis of the units consumed. Decomposed as price (native P&L at the
                // sell-time rate) + fx (residual on the principal), so price+fx == total.
                let realized_idr_delta =
                    (t.price_native * t.fx_to_idr - avg_idr) * sold - t.fee_native * t.fx_to_idr;
                let realized_price_idr_delta = realized_delta * t.fx_to_idr;
                realized_idr += realized_idr_delta;
                realized_price_idr += realized_price_idr_delta;
                realized_fx_idr += realized_idr_delta - realized_price_idr_delta;
                qty -= sold;
                if qty < Decimal::ZERO {
                    qty = Decimal::ZERO;
                }
            }
            TxnType::Dividend | TxnType::Interest => {
                income += t.quantity * t.price_native;
            }
            TxnType::Fee => {
                income -= t.quantity * t.price_native;
            }
        }
    }

    CostBasis {
        quantity: qty,
        avg_cost: avg,
        cost_basis_total: qty * avg,
        realized_pnl: realized,
        income,
        avg_cost_idr: avg_idr,
        cost_basis_idr_total: qty * avg_idr,
        realized_pnl_idr: realized_idr,
        realized_price_pnl_idr: realized_price_idr,
        realized_fx_pnl_idr: realized_fx_idr,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

    fn tx(t: TxnType, qty: Decimal, price: Decimal, fee: Decimal) -> Transaction {
        tx_fx(t, qty, price, fee, dec!(16000))
    }

    #[test]
    fn buy_then_buy_averages_cost() {
        let txns = vec![
            tx(TxnType::Buy, dec!(1), dec!(100), dec!(0)),
            tx(TxnType::Buy, dec!(1), dec!(200), dec!(0)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(2));
        assert_eq!(cb.avg_cost, dec!(150));
        assert_eq!(cb.cost_basis_total, dec!(300));
        assert_eq!(cb.realized_pnl, dec!(0));
    }

    #[test]
    fn fee_increases_cost_basis() {
        let txns = vec![tx(TxnType::Buy, dec!(1), dec!(100), dec!(10))];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.avg_cost, dec!(110));
        assert_eq!(cb.cost_basis_total, dec!(110));
    }

    #[test]
    fn sell_realizes_pnl_at_average() {
        let txns = vec![
            tx(TxnType::Buy, dec!(2), dec!(100), dec!(0)),
            tx(TxnType::Sell, dec!(1), dec!(150), dec!(0)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(1));
        assert_eq!(cb.avg_cost, dec!(100));
        assert_eq!(cb.realized_pnl, dec!(50));
    }

    #[test]
    fn dividend_is_income_not_position() {
        let txns = vec![
            tx(TxnType::Buy, dec!(1), dec!(100), dec!(0)),
            tx(TxnType::Dividend, dec!(1), dec!(5), dec!(0)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(1));
        assert_eq!(cb.income, dec!(5));
    }

    #[test]
    fn opening_balance_seeds_position() {
        let txns = vec![tx(TxnType::OpeningBalance, dec!(3), dec!(50), dec!(0))];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(3));
        assert_eq!(cb.avg_cost, dec!(50));
    }

    #[test]
    fn oversell_caps_at_held_quantity() {
        // Hold 1 @ 100, then sell 2 @ 150: only 1 unit is owned, so realize on 1 unit.
        let txns = vec![
            tx(TxnType::Buy, dec!(1), dec!(100), dec!(0)),
            tx(TxnType::Sell, dec!(2), dec!(150), dec!(0)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(0));
        assert_eq!(cb.realized_pnl, dec!(50)); // (150-100)*1, NOT *2
    }

    fn tx_fx(t: TxnType, qty: Decimal, price: Decimal, fee: Decimal, fx_to_idr: Decimal) -> Transaction {
        Transaction {
            id: 0,
            account_id: 1,
            instrument_id: 1,
            txn_type: t,
            executed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            quantity: qty,
            price_native: price,
            fee_native: fee,
            currency: "USD".into(),
            fx_to_idr,
            fx_to_usd: dec!(1),
            note: None,
        }
    }

    #[test]
    fn idr_cost_basis_uses_purchase_fx() {
        // 1 @ 100 at fx 16000, 1 @ 100 at fx 17000 -> avg_idr 1,650,000/unit.
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(1), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Buy, dec!(1), dec!(100), dec!(0), dec!(17000)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.avg_cost_idr, dec!(1650000));
        assert_eq!(cb.cost_basis_idr_total, dec!(3300000));
    }

    #[test]
    fn sell_decomposes_realized_into_price_and_fx() {
        // Buy 2 @ 100 at fx 16000 (avg_idr 1.6M). Sell 1 @ 150 at fx 17000:
        //   native realized = 50
        //   total idr = 150*17000 - 1,600,000 = 950,000
        //   price idr = 50*17000 = 850,000
        //   fx idr    = 100,000  (IDR weakened on the principal)
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(2), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Sell, dec!(1), dec!(150), dec!(0), dec!(17000)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl, dec!(50));
        assert_eq!(cb.realized_pnl_idr, dec!(950000));
        assert_eq!(cb.realized_price_pnl_idr, dec!(850000));
        assert_eq!(cb.realized_fx_pnl_idr, dec!(100000));
        // Remaining lot keeps its purchase-rate IDR basis.
        assert_eq!(cb.cost_basis_idr_total, dec!(1600000));
    }

    #[test]
    fn sell_fee_hits_both_idr_components_consistently() {
        // Buy 1 @ 100 fx 16000. Sell 1 @ 110 fee 5 fx 16500:
        //   native realized = (110-100)*1 - 5 = 5
        //   total idr = 110*16500 - 1,600,000 - 5*16500 = 132,500
        //   price idr = 5*16500 = 82,500 ; fx idr = 50,000
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(1), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Sell, dec!(1), dec!(110), dec!(5), dec!(16500)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl, dec!(5));
        assert_eq!(cb.realized_pnl_idr, dec!(132500));
        assert_eq!(cb.realized_price_pnl_idr, dec!(82500));
        assert_eq!(cb.realized_fx_pnl_idr, dec!(50000));
    }

    #[test]
    fn idr_instrument_has_zero_fx_component() {
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(2), dec!(1000), dec!(0), dec!(1)),
            tx_fx(TxnType::Sell, dec!(1), dec!(1500), dec!(0), dec!(1)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl_idr, dec!(500));
        assert_eq!(cb.realized_price_pnl_idr, dec!(500));
        assert_eq!(cb.realized_fx_pnl_idr, dec!(0));
        assert_eq!(cb.cost_basis_idr_total, dec!(1000));
    }

    #[test]
    fn realized_idr_invariant_price_plus_fx_equals_total() {
        // Multi-leg sequence with mixed rates; the residual definition makes this
        // exact, but keep the regression guard.
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(3), dec!(80), dec!(2), dec!(15500)),
            tx_fx(TxnType::Buy, dec!(1), dec!(120), dec!(1), dec!(16200)),
            tx_fx(TxnType::Sell, dec!(2), dec!(110), dec!(3), dec!(16800)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl_idr, cb.realized_price_pnl_idr + cb.realized_fx_pnl_idr);
    }

    #[test]
    fn rebuy_after_full_close_resets_idr_basis() {
        // Buy 2 @ 100 fx 16000 -> avg_idr 1,600,000.
        // Sell 2 @ 150 fx 17000 (full close):
        //   native realized = (150-100)*2 = 100
        //   total idr = (150*17000 - 1,600,000)*2 = 1,900,000
        //   price idr = 100*17000 = 1,700,000
        //   fx idr    = 200,000
        // Re-buy 1 @ 200 fx 18000:
        //   avg resets to 200; avg_idr resets to 200*18000 = 3,600,000.
        // Realized figures must be RETAINED from the close.
        let txns = vec![
            tx_fx(TxnType::Buy,  dec!(2), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Sell, dec!(2), dec!(150), dec!(0), dec!(17000)),
            tx_fx(TxnType::Buy,  dec!(1), dec!(200), dec!(0), dec!(18000)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity,              dec!(1));
        assert_eq!(cb.avg_cost,              dec!(200));
        assert_eq!(cb.avg_cost_idr,          dec!(3600000));
        assert_eq!(cb.cost_basis_idr_total,  dec!(3600000));
        assert_eq!(cb.realized_pnl_idr,      dec!(1900000));
        assert_eq!(cb.realized_price_pnl_idr, dec!(1700000));
        assert_eq!(cb.realized_fx_pnl_idr,   dec!(200000));
    }
}
