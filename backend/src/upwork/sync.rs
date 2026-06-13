//! Pure reconciliation: turn a batch of Upwork transactions into the cashflow
//! income rows to insert. No DB, no network — fully unit-testable. Only
//! earning-type transactions are kept (Approach A); fees/withdrawals/refunds
//! are dropped.

use crate::repo::cashflow::NewCashflow;
use crate::upwork::UpworkTransaction;

/// A cashflow row to insert, paired with the Upwork transaction id for
/// idempotent persistence.
#[derive(Debug, PartialEq)]
pub struct PlannedEarning {
    pub external_ref: String,
    pub cashflow: NewCashflow,
}

/// Classify a transaction as earned income. Earnings are fixed-price/milestone
/// releases, hourly charges, and bonuses; fees, withdrawals, and refunds are
/// excluded.
pub fn is_earning(kind: &str) -> bool {
    let k = kind.to_ascii_lowercase();
    if k.contains("refund") || k.contains("withdraw") || k.contains("fee") {
        return false;
    }
    ["fixed", "milestone", "hourly", "bonus"].iter().any(|p| k.contains(p))
}

/// Map earning transactions to cashflow income rows under `category_id`.
pub fn plan_earnings(txns: &[UpworkTransaction], category_id: i64) -> Vec<PlannedEarning> {
    txns.iter()
        .filter(|t| is_earning(&t.kind))
        .map(|t| PlannedEarning {
            external_ref: t.external_id.clone(),
            cashflow: NewCashflow {
                account_id: None,
                occurred_on: t.date.clone(),
                direction: "in".to_string(),
                amount: t.amount.clone(),
                currency: t.currency.clone(),
                category_id: Some(category_id),
                note: t.contract.clone(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txn(id: &str, kind: &str, amount: &str) -> UpworkTransaction {
        UpworkTransaction {
            external_id: id.into(), date: "2026-06-10".into(), kind: kind.into(),
            amount: amount.into(), currency: "USD".into(), contract: Some("Acme".into()),
        }
    }

    #[test]
    fn keeps_only_earnings() {
        let batch = vec![
            txn("t1", "Fixed Price milestone", "500.00"),
            txn("t2", "Hourly", "120.00"),
            txn("t3", "Bonus", "50.00"),
            txn("t4", "Service Fee", "-50.00"),
            txn("t5", "Withdrawal", "-400.00"),
            txn("t6", "Refund", "-25.00"),
        ];
        let planned = plan_earnings(&batch, 7);
        let refs: Vec<&str> = planned.iter().map(|p| p.external_ref.as_str()).collect();
        assert_eq!(refs, vec!["t1", "t2", "t3"]);
    }

    #[test]
    fn maps_fields_to_income_cashflow() {
        let planned = plan_earnings(&[txn("t1", "Hourly", "120.00")], 7);
        let p = &planned[0];
        assert_eq!(p.external_ref, "t1");
        assert_eq!(p.cashflow.direction, "in");
        assert_eq!(p.cashflow.amount, "120.00");
        assert_eq!(p.cashflow.currency, "USD");
        assert_eq!(p.cashflow.category_id, Some(7));
        assert_eq!(p.cashflow.note.as_deref(), Some("Acme"));
        assert_eq!(p.cashflow.occurred_on, "2026-06-10");
    }
}
