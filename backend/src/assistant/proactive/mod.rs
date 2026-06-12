//! Proactive sends: morning briefing, weekly recap, financial alerts.
//! Deterministic gathering → LLM composition (with fallback) → Telegram.

pub mod alerts;
pub mod compose;
pub mod tick;

use crate::repo::snapshots::SnapshotRow;
use rust_decimal::Decimal;

/// Net worth from the latest snapshot strictly BEFORE `today_wib`
/// (YYYY-MM-DD). The hourly scheduler overwrites today's row, so "the last
/// row" is today's value, not a usable baseline for day-over-day deltas.
pub fn snapshot_before(rows: &[SnapshotRow], today_wib: &str) -> Option<Decimal> {
    use std::str::FromStr;
    rows.iter()
        .rev()
        .find(|r| r.as_of.as_str() < today_wib)
        .and_then(|r| Decimal::from_str(&r.total_idr).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn snap(as_of: &str, total: &str) -> SnapshotRow {
        SnapshotRow {
            as_of: as_of.into(),
            total_idr: total.into(),
            total_usd: "0".into(),
            breakdown_json: "{}".into(),
            price_pnl_idr: None,
            fx_pnl_idr: None,
        }
    }

    #[test]
    fn picks_yesterday_not_today() {
        let rows = vec![snap("2026-06-11", "1490000000"), snap("2026-06-12", "1560000000")];
        assert_eq!(snapshot_before(&rows, "2026-06-12"), Some(dec!(1490000000)));
    }

    #[test]
    fn none_when_only_today_exists() {
        let rows = vec![snap("2026-06-12", "1560000000")];
        assert_eq!(snapshot_before(&rows, "2026-06-12"), None);
    }

    #[test]
    fn none_when_empty() {
        assert_eq!(snapshot_before(&[], "2026-06-12"), None);
    }
}
