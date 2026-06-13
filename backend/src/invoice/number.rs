//! Invoice numbers: `INV/<year>/<roman-month>/<NNN>`, NNN reset per month (WIB).

use crate::db::Db;
use chrono::{DateTime, Datelike, Utc};

pub fn roman_month(month: u32) -> &'static str {
    const ROMAN: [&str; 12] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ];
    ROMAN[(month.clamp(1, 12) - 1) as usize]
}

/// Pure: format the number for a given year/month and the highest existing
/// sequence that month (None when it's the first).
pub fn compute_number(year: i32, month: u32, last_seq: Option<u32>) -> String {
    let seq = last_seq.unwrap_or(0) + 1;
    format!("INV/{year}/{}/{seq:03}", roman_month(month))
}

/// Next invoice number for `now` (interpreted in WIB), reading the month's
/// current max sequence from the DB.
pub async fn next_number(db: &Db, now: DateTime<Utc>) -> anyhow::Result<String> {
    let wib = now.with_timezone(&crate::assistant::time::wib());
    let (year, month) = (wib.year(), wib.month());
    let prefix = format!("INV/{year}/{}/", roman_month(month));
    let last_seq = crate::repo::invoices::max_seq_for_prefix(db, &prefix).await?;
    Ok(compute_number(year, month, last_seq))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn roman_months_cover_all_twelve() {
        let expected = [
            "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
        ];
        for (i, want) in expected.iter().enumerate() {
            assert_eq!(roman_month(i as u32 + 1), *want);
        }
        // Out-of-range clamps instead of panicking.
        assert_eq!(roman_month(0), "I");
        assert_eq!(roman_month(13), "XII");
    }

    #[test]
    fn compute_number_resets_per_month() {
        assert_eq!(compute_number(2026, 6, None), "INV/2026/VI/001");
        assert_eq!(compute_number(2026, 6, Some(2)), "INV/2026/VI/003");
        assert_eq!(compute_number(2026, 7, None), "INV/2026/VII/001");
    }

    #[tokio::test]
    async fn next_number_uses_the_wib_month_and_db_state() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // 2026-06-30 20:00 UTC == 2026-07-01 03:00 WIB → July, not June.
        let now = chrono::Utc.with_ymd_and_hms(2026, 6, 30, 20, 0, 0).unwrap();
        assert_eq!(next_number(&db, now).await.unwrap(), "INV/2026/VII/001");
    }
}
