//! 5-minute loop: claim due schedules, run jobs, evaluate alerts.

use crate::db::Db;
use crate::telegram::client::TelegramClient;
use chrono::{DateTime, Datelike, FixedOffset, Timelike};

/// How long after the scheduled hour a send is still useful. The window does
/// NOT wrap past midnight: hours configured >= 19 get a clamped grace ending
/// at 23:59 — acceptable for a morning/evening schedule.
const GRACE_HOURS: u32 = 5;
/// Monday-morning cutoff for the weekly recap grace window.
const RECAP_MONDAY_GRACE_END_HOUR: u32 = 9;

#[derive(Debug, Clone)]
pub struct ProactiveConfig {
    pub briefing_hour: Option<u32>,
    pub recap_hour: Option<u32>,
    pub mover_alert_pct: f64,
    pub milestone_step_idr: i64,
}

/// "off" disables; unparseable or out-of-range values fall back to default.
fn parse_hour(raw: Option<String>, default: u32) -> Option<u32> {
    match raw {
        None => Some(default),
        Some(v) if v.eq_ignore_ascii_case("off") => None,
        Some(v) => match v.parse().ok().filter(|h| *h < 24) {
            Some(hour) => Some(hour),
            None => {
                tracing::warn!("unparseable schedule hour '{v}'; using default {default}");
                Some(default)
            }
        },
    }
}

impl ProactiveConfig {
    pub fn from_env() -> Self {
        Self {
            briefing_hour: parse_hour(std::env::var("BRIEFING_HOUR_WIB").ok(), 7),
            recap_hour: parse_hour(std::env::var("RECAP_HOUR_WIB").ok(), 17),
            mover_alert_pct: std::env::var("MOVER_ALERT_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5.0),
            milestone_step_idr: std::env::var("MILESTONE_STEP_IDR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50_000_000),
        }
    }
}

/// Dedup key when the morning briefing is currently due, else None. Due from
/// the configured hour for GRACE_HOURS; past that the day is forfeited.
pub fn briefing_due(now_wib: DateTime<FixedOffset>, briefing_hour: Option<u32>) -> Option<String> {
    let hour = briefing_hour?;
    let h = now_wib.hour();
    if h >= hour && h < hour + GRACE_HOURS {
        Some(format!("briefing:{}", now_wib.format("%Y-%m-%d")))
    } else {
        None
    }
}

/// Dedup key when the weekly recap is due: Sunday from the configured hour,
/// with grace until Monday 09:00 (keyed to the week that ended on Sunday).
pub fn recap_due(now_wib: DateTime<FixedOffset>, recap_hour: Option<u32>) -> Option<String> {
    let hour = recap_hour?;
    let due = match now_wib.weekday() {
        chrono::Weekday::Sun => now_wib.hour() >= hour,
        chrono::Weekday::Mon => now_wib.hour() < RECAP_MONDAY_GRACE_END_HOUR,
        _ => false,
    };
    if !due {
        return None;
    }
    // On Monday the recapped week is the one that ended yesterday.
    let anchor = if now_wib.weekday() == chrono::Weekday::Mon {
        now_wib - chrono::Duration::days(1)
    } else {
        now_wib
    };
    let week = anchor.iso_week();
    Some(format!("recap:{}-W{:02}", week.year(), week.week()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wib(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<FixedOffset> {
        use chrono::TimeZone;
        crate::assistant::time::wib().with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    // 2026-06-12 is a Friday; 2026-06-14 is a Sunday; 2026-06-15 a Monday.

    #[test]
    fn briefing_due_inside_the_window_only() {
        assert_eq!(briefing_due(wib(2026, 6, 12, 6, 55), Some(7)), None);
        assert_eq!(
            briefing_due(wib(2026, 6, 12, 7, 0), Some(7)),
            Some("briefing:2026-06-12".to_string())
        );
        assert_eq!(
            briefing_due(wib(2026, 6, 12, 11, 59), Some(7)),
            Some("briefing:2026-06-12".to_string())
        );
        // Past the 5-hour grace window the day is forfeited.
        assert_eq!(briefing_due(wib(2026, 6, 12, 12, 0), Some(7)), None);
        // Disabled.
        assert_eq!(briefing_due(wib(2026, 6, 12, 8, 0), None), None);
    }

    #[test]
    fn recap_due_sunday_evening_with_monday_grace() {
        // Friday: never.
        assert_eq!(recap_due(wib(2026, 6, 12, 18, 0), Some(17)), None);
        // Sunday before the hour: not yet.
        assert_eq!(recap_due(wib(2026, 6, 14, 16, 59), Some(17)), None);
        // Sunday at/after the hour: due, keyed by the ISO week ending that Sunday.
        assert_eq!(
            recap_due(wib(2026, 6, 14, 17, 0), Some(17)),
            Some("recap:2026-W24".to_string())
        );
        // Monday before 09:00: grace, SAME key (the week that ended yesterday).
        assert_eq!(
            recap_due(wib(2026, 6, 15, 8, 30), Some(17)),
            Some("recap:2026-W24".to_string())
        );
        // Monday 09:00: forfeited.
        assert_eq!(recap_due(wib(2026, 6, 15, 9, 0), Some(17)), None);
        // Disabled.
        assert_eq!(recap_due(wib(2026, 6, 14, 18, 0), None), None);
    }

    #[test]
    fn config_defaults_are_sane() {
        // Tests never set the env vars, so this exercises the default path.
        let config = ProactiveConfig::from_env();
        assert_eq!(config.briefing_hour, Some(7));
        assert_eq!(config.recap_hour, Some(17));
        assert!((config.mover_alert_pct - 5.0).abs() < f64::EPSILON);
        assert_eq!(config.milestone_step_idr, 50_000_000);
    }

    #[test]
    fn hour_parsing_handles_off_and_garbage() {
        assert_eq!(parse_hour(None, 7), Some(7));
        assert_eq!(parse_hour(Some("off".into()), 7), None);
        assert_eq!(parse_hour(Some("OFF".into()), 7), None);
        assert_eq!(parse_hour(Some("9".into()), 7), Some(9));
        // Garbage and out-of-range fall back to the default.
        assert_eq!(parse_hour(Some("banana".into()), 7), Some(7));
        assert_eq!(parse_hour(Some("25".into()), 7), Some(7));
    }
}
