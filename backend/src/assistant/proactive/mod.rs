//! Proactive sends: morning briefing, weekly recap, financial alerts.
//! Deterministic gathering → LLM composition (with fallback) → Telegram.

pub mod alerts;
pub mod briefing;
pub mod compose;
pub mod evening_review;
pub mod monthly_recap;
pub mod plan;
pub mod recap;
pub mod tick;

use crate::repo::snapshots::SnapshotRow;
use rust_decimal::Decimal;

/// Net worth from the latest snapshot strictly BEFORE the given WIB date
/// (YYYY-MM-DD). Callers wanting a ~24h baseline must pass YESTERDAY's WIB
/// date: snapshot rows are keyed by UTC date, and the row labeled with
/// yesterday's UTC date gets its final hourly value at ~07:00 WIB today —
/// passing today's date would yield a one-hour-old "baseline".
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

    #[test]
    fn day_over_day_baseline_skips_the_utc_labeled_yesterday_row() {
        // Callers pass yesterday's WIB date; the row labeled with yesterday's
        // UTC date (finalized ~07:00 WIB today) must NOT be the baseline.
        let rows = vec![
            snap("2026-06-10", "1490000000"), // ~07:00 WIB Jun 11 value
            snap("2026-06-11", "1555000000"), // ~07:00 WIB Jun 12 value (1h old)
        ];
        assert_eq!(snapshot_before(&rows, "2026-06-11"), Some(dec!(1490000000)));
    }
}
