//! Loads NAV snapshots + external cashflows, windows them by period, and builds
//! the TWR performance view for the requested base currency.

use crate::db::Db;
use crate::domain::models::TxnType;
use crate::domain::performance::{compute, PerfMetrics};
use chrono::{Datelike, NaiveDate, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

#[derive(Serialize)]
pub struct PerfPointOut {
    pub date: String,
    pub cum_return: f64,
    pub nav: f64,
}

#[derive(Serialize)]
pub struct PerformanceView {
    pub base: String,
    pub points: Vec<PerfPointOut>,
    pub metrics: PerfMetrics,
    pub insufficient_data: bool,
}

/// Resolve a period string to an inclusive start date. `all` => None (no floor).
fn period_start(period: &str, today: NaiveDate) -> Option<NaiveDate> {
    match period {
        "all" => None,
        "ytd" => NaiveDate::from_ymd_opt(today.year(), 1, 1),
        "1m" => today.checked_sub_months(chrono::Months::new(1)),
        "3m" => today.checked_sub_months(chrono::Months::new(3)),
        "6m" => today.checked_sub_months(chrono::Months::new(6)),
        "1y" => today.checked_sub_months(chrono::Months::new(12)),
        _ => today.checked_sub_months(chrono::Months::new(12)), // default 1y
    }
}

pub async fn build_performance(
    db: &Db,
    base: &str,
    period: &str,
) -> anyhow::Result<PerformanceView> {
    let usd = base == "usd";
    let today = Utc::now().date_naive();
    let floor = period_start(period, today);

    // NAV series from snapshots, parsed to (date, f64) in the chosen base.
    let snaps = crate::repo::snapshots::history(db).await?;
    let mut navs: Vec<(NaiveDate, f64)> = Vec::new();
    for s in &snaps {
        let date = NaiveDate::parse_from_str(&s.as_of, "%Y-%m-%d")?;
        if let Some(f) = floor {
            if date < f {
                continue;
            }
        }
        let raw = if usd { &s.total_usd } else { &s.total_idr };
        let v = Decimal::from_str(raw)
            .unwrap_or_default()
            .to_f64()
            .unwrap_or(0.0);
        navs.push((date, v));
    }

    // External flows: Deposit (+) / Withdrawal (-), valued in the chosen base.
    let txns = crate::repo::transactions::list_all(db).await?;
    let mut flows: Vec<(NaiveDate, f64)> = Vec::new();
    for t in &txns {
        let sign = match t.txn_type {
            TxnType::Deposit => Decimal::ONE,
            TxnType::Withdrawal => Decimal::NEGATIVE_ONE,
            _ => continue,
        };
        let date = t.executed_at.date_naive();
        if let Some(f) = floor {
            if date < f {
                continue;
            }
        }
        let fx = if usd { t.fx_to_usd } else { t.fx_to_idr };
        let value = (t.quantity * t.price_native * fx * sign)
            .to_f64()
            .unwrap_or(0.0);
        flows.push((date, value));
    }

    let (points, metrics) = compute(&navs, &flows);
    let insufficient_data = points.is_empty();

    Ok(PerformanceView {
        base: base.to_string(),
        points: points
            .into_iter()
            .map(|p| PerfPointOut {
                date: p.date.format("%Y-%m-%d").to_string(),
                cum_return: p.cum_return,
                nav: p.nav,
            })
            .collect(),
        metrics,
        insufficient_data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::accounts::NewAccount;
    use crate::repo::instruments::NewInstrument;
    use crate::repo::transactions::NewTransaction;
    use chrono::DateTime;

    fn new_cash_instrument() -> NewInstrument {
        NewInstrument {
            symbol: "IDR".into(),
            name: "Cash IDR".into(),
            instrument_type: "cash".into(),
            native_currency: "IDR".into(),
            category_id: None,
            price_source: "manual".into(),
            decimals: Some(2),
            note: None,
        }
    }

    fn deposit_txn(
        account_id: i64,
        instrument_id: i64,
        date: &str,
        amount: &str,
    ) -> NewTransaction {
        NewTransaction {
            account_id,
            instrument_id,
            txn_type: "deposit".into(),
            executed_at: DateTime::parse_from_rfc3339(&format!("{date}T00:00:00Z"))
                .unwrap()
                .with_timezone(&chrono::Utc),
            quantity: amount.into(),
            price_native: "1".into(),
            fee_native: None,
            currency: "IDR".into(),
            fx_to_idr: "1".into(),
            fx_to_usd: "0.000065".into(),
            note: None,
            source: None,
            external_id: None,
        }
    }

    #[tokio::test]
    async fn deposit_does_not_create_return() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // Two snapshots: 1,000,000 -> 2,000,000 IDR, but caused by a deposit.
        crate::repo::snapshots::upsert(&db, "2026-01-01", "1000000", "65", "{}")
            .await
            .unwrap();
        crate::repo::snapshots::upsert(&db, "2026-01-02", "2000000", "130", "{}")
            .await
            .unwrap();
        // Need an account + instrument to satisfy FKs for the txn.
        let acc = crate::repo::accounts::create(
            &db,
            &NewAccount {
                name: "Cash".into(),
                account_type: "bank".into(),
                institution: None,
                native_currency: "IDR".into(),
                note: None,
            },
        )
        .await
        .unwrap();
        let inst = crate::repo::instruments::create(&db, &new_cash_instrument())
            .await
            .unwrap();
        crate::repo::transactions::create(
            &db,
            &deposit_txn(acc.id, inst.id, "2026-01-02", "1000000"),
        )
        .await
        .unwrap();

        let view = build_performance(&db, "idr", "all").await.unwrap();
        assert!(!view.insufficient_data);
        assert_eq!(view.base, "idr");
        assert!(view.points.last().unwrap().cum_return.abs() < 1e-9);
        assert!(view.metrics.total_return.abs() < 1e-9);
    }

    #[tokio::test]
    async fn insufficient_when_one_snapshot() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::repo::snapshots::upsert(&db, "2026-01-01", "1000000", "65", "{}")
            .await
            .unwrap();
        let view = build_performance(&db, "idr", "all").await.unwrap();
        assert!(view.insufficient_data);
        assert!(view.points.is_empty());
    }
}
