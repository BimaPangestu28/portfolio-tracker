//! Read-side helpers over the synthetic Hyperliquid equity instrument.

use crate::db::Db;
use crate::domain::models::TxnType;
use crate::domain::performance::{compute, PerfMetrics};
use crate::setup::{HL_ACCOUNT_NAME, HL_SYMBOL};
use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Serialize)]
pub struct HlPoint {
    pub date: String,
    pub cum_return: f64,
    pub nav: f64,
}

#[derive(Serialize)]
pub struct HyperliquidView {
    pub points: Vec<HlPoint>,
    pub metrics: PerfMetrics,
    pub current_value_usd: String,
    pub insufficient_data: bool,
}

const EMPTY_METRICS: PerfMetrics = PerfMetrics {
    total_return: 0.0,
    annualized: None,
    max_drawdown: 0.0,
    volatility: 0.0,
};

/// TWR equity curve for the Hyperliquid account, in USD. NAV series is the
/// equity price quotes; flows are USD deposits/withdrawals on the HL account.
pub async fn build_hyperliquid_view(db: &Db) -> anyhow::Result<HyperliquidView> {
    let instrument = match crate::repo::instruments::find_by_symbol(db, HL_SYMBOL).await? {
        Some(i) => i,
        None => {
            return Ok(HyperliquidView {
                points: Vec::new(),
                metrics: EMPTY_METRICS,
                current_value_usd: "0".into(),
                insufficient_data: true,
            })
        }
    };
    let series = crate::repo::prices::series(db, instrument.id).await?;
    let mut navs: Vec<(NaiveDate, f64)> = Vec::new();
    for (as_of, price) in &series {
        if let (Ok(date), Some(v)) = (NaiveDate::parse_from_str(as_of, "%Y-%m-%d"), price.to_f64()) {
            navs.push((date, v));
        }
    }
    let mut flows: Vec<(NaiveDate, f64)> = Vec::new();
    if let Some(account) = crate::repo::accounts::find_by_name(db, HL_ACCOUNT_NAME).await? {
        for t in crate::repo::transactions::list_all(db).await? {
            if t.account_id != account.id {
                continue;
            }
            let sign = match t.txn_type {
                TxnType::Deposit => 1.0,
                TxnType::Withdrawal => -1.0,
                _ => continue,
            };
            let value = (t.quantity * t.price_native * t.fx_to_usd).to_f64().unwrap_or(0.0) * sign;
            flows.push((t.executed_at.date_naive(), value));
        }
    }
    let (points, metrics) = compute(&navs, &flows);
    let current_value_usd = series.last().map(|(_, p)| p.to_string()).unwrap_or_else(|| "0".into());
    Ok(HyperliquidView {
        points: points
            .into_iter()
            .map(|p| HlPoint {
                date: p.date.format("%Y-%m-%d").to_string(),
                cum_return: p.cum_return,
                nav: p.nav,
            })
            .collect(),
        metrics,
        current_value_usd,
        insufficient_data: navs.len() < 2,
    })
}

#[derive(Debug, Clone)]
pub struct HlEquitySummary {
    pub equity_usd: Decimal,
    pub change_pct: Option<f64>,
}

/// Current equity and its percent change since the latest quote on or before
/// `since_date` (falls back to the earliest quote). `None` when the Hyperliquid
/// instrument or any price quote is absent.
pub async fn equity_and_change(db: &Db, since_date: &str) -> anyhow::Result<Option<HlEquitySummary>> {
    let instrument = match crate::repo::instruments::find_by_symbol(db, HL_SYMBOL).await? {
        Some(i) => i,
        None => return Ok(None),
    };
    let series = crate::repo::prices::series(db, instrument.id).await?;
    let current = match series.last() {
        Some((_, price)) => *price,
        None => return Ok(None),
    };
    let baseline = series
        .iter()
        .rev()
        .find(|(date, _)| date.as_str() <= since_date)
        .or_else(|| series.first())
        .map(|(_, price)| *price);
    let change_pct = baseline.and_then(|b| {
        if b.is_zero() {
            None
        } else {
            ((current - b) / b * Decimal::from(100)).to_f64()
        }
    });
    Ok(Some(HlEquitySummary { equity_usd: current, change_pct }))
}

/// "Hyperliquid: $1234.50 (+2.3%)" — pct omitted when unknown.
pub fn format_hyperliquid_line(s: &HlEquitySummary) -> String {
    use rust_decimal::prelude::ToPrimitive;
    let pct = s
        .change_pct
        .map(|p| format!(" ({p:+.1}%)"))
        .unwrap_or_default();
    let equity_f64 = s.equity_usd.to_f64().unwrap_or(0.0);
    format!("Hyperliquid: ${:.2}{}", equity_f64, pct)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[tokio::test]
    async fn build_view_produces_twr_points_from_equity_series() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db, "0x").await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await.unwrap().unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1000), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1100), "USD", "hyperliquid", "2026-06-02").await.unwrap();
        let view = build_hyperliquid_view(&db).await.unwrap();
        assert!(!view.insufficient_data);
        assert_eq!(view.current_value_usd, "1100");
        assert!((view.metrics.total_return - 0.10).abs() < 1e-9);
    }

    /// A pure deposit on the Hyperliquid account (HL-USDC txn) must not register
    /// as a return in the per-account TWR. Mirrors the global-path test
    /// `service::performance::tests::hl_usdc_deposit_does_not_create_return`
    /// but scoped to `build_hyperliquid_view`.
    ///
    /// Setup: NAV doubles 1000 → 2000 in one step, but a 1000 USD deposit lands
    /// on the same day as the second quote. TWR formula:
    ///   r = (2000 - 1000) / 1000 - 1 = 0 → total_return must be ≈ 0.
    #[tokio::test]
    async fn hl_usdc_deposit_does_not_inflate_per_account_twr() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db, "0x").await.unwrap();

        // Look up the real instrument ids provisioned by setup.
        let hl_equity_ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await.unwrap().expect("HL-EQUITY instrument must exist after setup");
        let hl_usdc_ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_FLOW_SYMBOL)
            .await.unwrap().expect("HL-USDC flow instrument must exist after setup");
        let hl_account = crate::repo::accounts::find_by_name(&db, crate::setup::HL_ACCOUNT_NAME)
            .await.unwrap().expect("Hyperliquid account must exist after setup");

        // Two price quotes for HL-EQUITY: equity doubles from 1000 to 2000 USD.
        crate::repo::prices::upsert_latest(
            &db, hl_equity_ins.id, dec!(1000), "USD", "hyperliquid", "2026-06-01",
        ).await.unwrap();
        crate::repo::prices::upsert_latest(
            &db, hl_equity_ins.id, dec!(2000), "USD", "hyperliquid", "2026-06-02",
        ).await.unwrap();

        // Deposit of 1000 USD into the Hyperliquid account on the date of the second
        // quote. The full NAV rise must be attributed to this flow, not to trading gains.
        crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction {
            account_id: hl_account.id,
            instrument_id: hl_usdc_ins.id,
            txn_type: "deposit".into(),
            executed_at: chrono::DateTime::parse_from_rfc3339("2026-06-02T00:00:00Z")
                .unwrap()
                .with_timezone(&chrono::Utc),
            quantity: "1000".into(),
            price_native: "1".into(),
            fee_native: None,
            currency: "USD".into(),
            fx_to_idr: "16000".into(),
            fx_to_usd: "1".into(),
            note: None,
            source: None,
            external_id: None,
        }).await.unwrap();

        let view = build_hyperliquid_view(&db).await.unwrap();
        assert!(!view.insufficient_data, "two price quotes must mark data as sufficient");
        assert!(
            view.metrics.total_return.abs() < 1e-9,
            "pure deposit must net per-account TWR to ≈0, got {}",
            view.metrics.total_return,
        );
    }

    #[test]
    fn formats_line_with_and_without_pct() {
        let with = HlEquitySummary { equity_usd: dec!(1234.5), change_pct: Some(2.34) };
        assert_eq!(format_hyperliquid_line(&with), "Hyperliquid: $1234.50 (+2.3%)");
        let without = HlEquitySummary { equity_usd: dec!(1000), change_pct: None };
        assert_eq!(format_hyperliquid_line(&without), "Hyperliquid: $1000.00");
    }

    #[tokio::test]
    async fn equity_and_change_computes_pct_since_baseline() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db, "0x").await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await.unwrap().unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(100), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(110), "USD", "hyperliquid", "2026-06-05").await.unwrap();
        // Baseline = latest quote on/before 2026-06-01 → 100; current → 110 → +10%.
        let s = equity_and_change(&db, "2026-06-01").await.unwrap().expect("summary");
        assert_eq!(s.equity_usd, dec!(110));
        assert!((s.change_pct.unwrap() - 10.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn equity_and_change_none_when_no_instrument() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(equity_and_change(&db, "2026-06-01").await.unwrap().is_none());
    }
}
