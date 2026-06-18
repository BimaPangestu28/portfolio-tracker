//! Read-side helpers over the synthetic Hyperliquid equity instrument.

use crate::db::Db;
use crate::setup::HL_SYMBOL;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

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
}
