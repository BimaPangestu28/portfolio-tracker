//! Read-side helpers over the synthetic Hyperliquid equity instrument.
//!
//! Exposes [`build_hyperliquid_view`] which assembles the equity TWR curve,
//! current open positions, recent closed trades, and aggregate realized stats
//! into a single [`HyperliquidView`] for the `GET /portfolio/hyperliquid` endpoint.

use crate::db::Db;
use crate::domain::performance::{compute, PerfMetrics};
use crate::repo::hl::{list_positions, list_trades, HlPosition, HlTrade};
use crate::setup::HL_SYMBOL;
use chrono::NaiveDate;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

/// A single point on the Hyperliquid equity TWR curve.
#[derive(Debug, Clone, Serialize)]
pub struct HlPoint {
    /// ISO date string (YYYY-MM-DD).
    pub date: String,
    /// Cumulative return from the start of the series (0.10 = +10%).
    pub cum_return: f64,
    /// NAV (equity in USD) at this date.
    pub nav: f64,
}

/// Full read-side view for the Hyperliquid portfolio endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct HyperliquidView {
    /// TWR curve points — empty when fewer than 2 equity quotes exist.
    pub points: Vec<HlPoint>,
    /// Performance metrics derived from the TWR curve.
    pub metrics: PerfMetrics,
    /// Current equity as a string (last known quote), e.g. `"1100"`.
    pub current_value_usd: String,
    /// Snapshot of currently open perp positions.
    pub positions: Vec<HlPosition>,
    /// Recent closed trades (newest first, capped at 200).
    pub trades: Vec<HlTrade>,
    /// Sum of `realized_pnl` across all fetched closed trades.
    pub realized_pnl_total: String,
    /// Fraction of profitable closed trades; `None` when there are no trades.
    pub win_rate: Option<f64>,
    /// `true` when the equity series has fewer than 2 data points — TWR is
    /// not meaningful in that case and `points` / `metrics` will be empty/zero.
    pub insufficient_data: bool,
}

/// Assemble the [`HyperliquidView`] from the database.
///
/// TWR uses the equity price series as NAV with **no external flows** — deposit
/// and withdrawal analytics live in the separate transaction layer.
///
/// @param db - Database connection pool
/// @returns Populated `HyperliquidView`; never errors on missing data (returns
///          zeroed metrics with `insufficient_data = true` instead).
pub async fn build_hyperliquid_view(db: &Db) -> anyhow::Result<HyperliquidView> {
    let trades = list_trades(db, 200).await?;
    let positions = list_positions(db).await?;

    // Aggregate realized PnL and count winning trades from stored closed trades.
    let mut realized_total = Decimal::ZERO;
    let mut winning_trade_count = 0i64;
    for trade in &trades {
        if let Ok(pnl) = Decimal::from_str(&trade.realized_pnl) {
            realized_total += pnl;
            if pnl > Decimal::ZERO {
                winning_trade_count += 1;
            }
        }
    }
    let win_rate = if trades.is_empty() {
        None
    } else {
        Some(winning_trade_count as f64 / trades.len() as f64)
    };

    // Build the NAV series from the equity price history (ascending by date).
    let equity_instrument = crate::repo::instruments::find_by_symbol(db, HL_SYMBOL).await?;
    let price_series = match &equity_instrument {
        Some(instrument) => crate::repo::prices::series(db, instrument.id).await?,
        None => Vec::new(),
    };
    let navs: Vec<(NaiveDate, f64)> = price_series
        .iter()
        .filter_map(|(date_str, price)| {
            let parsed_date = NaiveDate::parse_from_str(date_str, "%Y-%m-%d").ok()?;
            let price_f64 = price.to_f64()?;
            Some((parsed_date, price_f64))
        })
        .collect();

    // Equity-curve TWR with no external flows (deposit/withdraw analytics are
    // handled separately in the transaction layer).
    let flows: Vec<(NaiveDate, f64)> = Vec::new();
    let (perf_points, metrics) = compute(&navs, &flows);

    let current_value_usd = price_series
        .last()
        .map(|(_, price)| price.normalize().to_string())
        .unwrap_or_else(|| "0".into());

    let curve_points: Vec<HlPoint> = perf_points
        .into_iter()
        .map(|point| HlPoint {
            date: point.date.format("%Y-%m-%d").to_string(),
            cum_return: point.cum_return,
            nav: point.nav,
        })
        .collect();

    let insufficient_data = navs.len() < 2;

    Ok(HyperliquidView {
        points: curve_points,
        metrics,
        current_value_usd,
        positions,
        trades,
        realized_pnl_total: realized_total.normalize().to_string(),
        win_rate,
        insufficient_data,
    })
}

/// Snapshot of the synthetic Hyperliquid equity instrument.
#[derive(Debug, Clone)]
pub struct HlEquitySummary {
    pub equity_usd: Decimal,
    pub change_pct: Option<f64>,
}

/// Current equity and percent change since the latest quote on or before
/// `since_date`. Returns `None` when the instrument or any quote is absent.
///
/// The baseline is the most-recent quote whose `as_of` date is on or before
/// `since_date`. If no such quote exists the first available quote is used.
///
/// @param db - Database connection pool
/// @param since_date - ISO date string (YYYY-MM-DD) used as the baseline anchor
/// @returns `Some(HlEquitySummary)` with the current equity and percent change,
///          or `None` when the HL instrument or its price history is absent.
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

/// Format the equity summary as a single display line.
///
/// Returns "Hyperliquid: $1234.50 (+2.3%)" when change is known,
/// or "Hyperliquid: $1234.50" when it is absent.
pub fn format_hyperliquid_line(summary: &HlEquitySummary) -> String {
    let pct_suffix = summary
        .change_pct
        .map(|pct| format!(" ({pct:+.1}%)"))
        .unwrap_or_default();
    format!("Hyperliquid: ${:.2}{}", summary.equity_usd, pct_suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn formats_line_with_and_without_pct() {
        let with_pct = HlEquitySummary { equity_usd: dec!(1234.5), change_pct: Some(2.34) };
        assert_eq!(format_hyperliquid_line(&with_pct), "Hyperliquid: $1234.50 (+2.3%)");
        let without_pct = HlEquitySummary { equity_usd: dec!(1000), change_pct: None };
        assert_eq!(format_hyperliquid_line(&without_pct), "Hyperliquid: $1000.00");
    }

    #[tokio::test]
    async fn equity_and_change_computes_pct_since_baseline() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db).await.unwrap();
        let instrument = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await
            .unwrap()
            .unwrap();
        crate::repo::prices::upsert_latest(&db, instrument.id, dec!(100), "USD", "hyperliquid", "2026-06-01")
            .await
            .unwrap();
        crate::repo::prices::upsert_latest(&db, instrument.id, dec!(110), "USD", "hyperliquid", "2026-06-05")
            .await
            .unwrap();
        let summary = equity_and_change(&db, "2026-06-01").await.unwrap().expect("summary");
        assert_eq!(summary.equity_usd, dec!(110));
        assert!((summary.change_pct.unwrap() - 10.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn build_view_produces_curve_positions_and_stats() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db).await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await
            .unwrap()
            .unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1000), "USD", "hyperliquid", "2026-06-01")
            .await
            .unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1100), "USD", "hyperliquid", "2026-06-02")
            .await
            .unwrap();
        let view = build_hyperliquid_view(&db).await.unwrap();
        assert!(!view.insufficient_data);
        assert_eq!(view.current_value_usd, "1100");
        assert!((view.metrics.total_return - 0.10).abs() < 1e-9);
    }
}
