use crate::domain::models::{Transaction, TxnType};
use rust_decimal::Decimal;

#[derive(Debug, Clone, PartialEq)]
pub struct CostBasis {
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub cost_basis_total: Decimal,
    pub realized_pnl: Decimal,
    pub income: Decimal,
}

/// Average-cost engine. `txns` MUST be sorted ascending by `executed_at`.
pub fn compute_cost_basis(txns: &[Transaction]) -> CostBasis {
    let mut qty = Decimal::ZERO;
    let mut avg = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut income = Decimal::ZERO;

    for t in txns {
        match t.txn_type {
            TxnType::Buy | TxnType::OpeningBalance | TxnType::Deposit => {
                let added_cost = t.quantity * t.price_native + t.fee_native;
                let new_qty = qty + t.quantity;
                if new_qty.is_zero() {
                    avg = Decimal::ZERO;
                } else {
                    avg = (qty * avg + added_cost) / new_qty;
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
                realized += (t.price_native - avg) * sold - t.fee_native;
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

    fn tx(t: TxnType, qty: Decimal, price: Decimal, fee: Decimal) -> Transaction {
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
            fx_to_idr: dec!(16000),
            fx_to_usd: dec!(1),
            note: None,
        }
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
}
