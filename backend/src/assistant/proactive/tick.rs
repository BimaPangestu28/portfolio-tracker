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
    pub evening_review_hour: Option<u32>,
    pub monthly_recap_hour: Option<u32>,
    pub news_digest_hour: Option<u32>,
    pub mover_alert_pct: f64,
    pub milestone_step_idr: i64,
    pub hl_drawdown_pct: f64,
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
            evening_review_hour: parse_hour(std::env::var("EVENING_REVIEW_HOUR_WIB").ok(), 21),
            monthly_recap_hour: parse_hour(std::env::var("MONTHLY_RECAP_HOUR_WIB").ok(), 8),
            news_digest_hour: parse_hour(std::env::var("NEWS_DIGEST_HOUR_WIB").ok(), 6),
            mover_alert_pct: std::env::var("MOVER_ALERT_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5.0),
            milestone_step_idr: std::env::var("MILESTONE_STEP_IDR")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(50_000_000),
            hl_drawdown_pct: std::env::var("HL_DRAWDOWN_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15.0),
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

/// Dedup key when the morning news digest is due (its own hour, default 6 WIB),
/// using the same fixed-hour grace window as the briefing.
pub fn news_digest_due(now_wib: DateTime<FixedOffset>, news_hour: Option<u32>) -> Option<String> {
    let hour = news_hour?;
    let h = now_wib.hour();
    if h >= hour && h < hour + GRACE_HOURS {
        Some(format!("news_digest:{}", now_wib.format("%Y-%m-%d")))
    } else {
        None
    }
}

/// Dedup key when the evening review is due, else None. Same fixed-hour grace
/// window as the briefing; the day is forfeited past the window.
pub fn evening_review_due(
    now_wib: DateTime<FixedOffset>,
    review_hour: Option<u32>,
) -> Option<String> {
    let hour = review_hour?;
    let h = now_wib.hour();
    if h >= hour && h < hour + GRACE_HOURS {
        Some(format!("evening_review:{}", now_wib.format("%Y-%m-%d")))
    } else {
        None
    }
}

const TICK: std::time::Duration = std::time::Duration::from_secs(300);

/// Spawn the proactive loop when TELEGRAM_BOT_TOKEN is configured.
pub fn spawn(db: Db) {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else {
        tracing::info!("TELEGRAM_BOT_TOKEN not set; proactive sends disabled");
        return;
    };
    tokio::spawn(async move {
        let client = TelegramClient::new(token);
        let config = ProactiveConfig::from_env();
        loop {
            if let Err(e) = run_once(&db, &client, &config).await {
                tracing::warn!("proactive tick failed: {e:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// One pass: claim-then-send for whatever is due. Claiming BEFORE sending
/// makes every send at-most-once (a duplicate briefing annoys more than a
/// missing one — the inverse of the reminder loop's trade-off).
pub async fn run_once(
    db: &Db,
    client: &TelegramClient,
    config: &ProactiveConfig,
) -> anyhow::Result<()> {
    let now_wib = chrono::Utc::now().with_timezone(&crate::assistant::time::wib());

    // The news digest generates regardless of the Telegram chat link — the web
    // page consumes it too. ensure_today is idempotent and claims internally.
    if news_digest_due(now_wib, config.news_digest_hour).is_some() {
        if let Err(e) = super::news::digest::ensure_today(db).await {
            tracing::warn!("news digest tick failed: {e:#}");
        }
    }

    let Some(link) = crate::repo::telegram_link::get(db).await? else {
        return Ok(());
    };
    let today = now_wib.format("%Y-%m-%d").to_string();

    if let Some(key) = briefing_due(now_wib, config.briefing_hour) {
        if crate::repo::proactive_log::try_claim(db, "briefing", &key).await? {
            if let Err(e) = super::briefing::run(db, client, link.chat_id).await {
                tracing::warn!("briefing for {key} forfeited: {e:#}");
            }
        }
    }

    if let Some(key) = recap_due(now_wib, config.recap_hour) {
        if crate::repo::proactive_log::try_claim(db, "recap", &key).await? {
            if let Err(e) = super::recap::run(db, client, link.chat_id).await {
                tracing::warn!("recap for {key} forfeited: {e:#}");
            }
        }
    }

    if let Some(key) = evening_review_due(now_wib, config.evening_review_hour) {
        if crate::repo::proactive_log::try_claim(db, "evening_review", &key).await? {
            if let Err(e) = super::evening_review::run(db, client, link.chat_id).await {
                tracing::warn!("evening review for {key} forfeited: {e:#}");
            }
        }
    }

    if let Some(key) = monthly_recap_due(now_wib, config.monthly_recap_hour) {
        if crate::repo::proactive_log::try_claim(db, "monthly_recap", &key).await? {
            if let Err(e) = super::monthly_recap::run(db, client, link.chat_id).await {
                tracing::warn!("monthly_recap for {key} forfeited: {e:#}");
            }
        }
    }

    for alert in
        super::alerts::evaluate(db, config.mover_alert_pct, config.milestone_step_idr, config.hl_drawdown_pct, &today).await
    {
        if crate::repo::proactive_log::try_claim(db, "alert", &alert.dedup_key).await? {
            if let Err(e) = client.send_message(link.chat_id, &alert.message).await {
                tracing::warn!("alert {} forfeited: {e:#}", alert.dedup_key);
            }
        }
    }

    Ok(())
}

/// Dedup key when the monthly recap is due: the 1st of the month from the
/// configured hour for GRACE_HOURS. The key encodes the PRIOR month so the
/// same send is never attempted twice for the same billing period.
pub fn monthly_recap_due(
    now_wib: DateTime<FixedOffset>,
    hour: Option<u32>,
) -> Option<String> {
    let hour = hour?;
    let h = now_wib.hour();
    if now_wib.day() != 1 || h < hour || h >= hour + GRACE_HOURS {
        return None;
    }
    // Prior month: subtract one day from the 1st to land in the prior month.
    let prior = (now_wib - chrono::Duration::days(1)).format("%Y-%m").to_string();
    Some(format!("monthly_recap:{prior}"))
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
    fn news_digest_due_inside_the_window_only() {
        assert_eq!(news_digest_due(wib(2026, 6, 12, 5, 59), Some(6)), None);
        assert_eq!(news_digest_due(wib(2026, 6, 12, 6, 0), Some(6)), Some("news_digest:2026-06-12".to_string()));
        // Last valid hour: window is [6, 11), so 10:59 is still in.
        assert_eq!(news_digest_due(wib(2026, 6, 12, 10, 59), Some(6)), Some("news_digest:2026-06-12".to_string()));
        assert_eq!(news_digest_due(wib(2026, 6, 12, 11, 0), Some(6)), None);
        assert_eq!(news_digest_due(wib(2026, 6, 12, 6, 0), None), None);
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
        assert_eq!(config.monthly_recap_hour, Some(8));
        assert!((config.mover_alert_pct - 5.0).abs() < f64::EPSILON);
        assert_eq!(config.milestone_step_idr, 50_000_000);
        assert_eq!(config.evening_review_hour, Some(21));
        assert_eq!(config.news_digest_hour, Some(6));
    }

    #[test]
    fn monthly_recap_due_fires_on_first_in_window_only() {
        // 2026-07-01 is a Wednesday; prior month is 2026-06.
        assert_eq!(
            monthly_recap_due(wib(2026, 7, 1, 8, 0), Some(8)),
            Some("monthly_recap:2026-06".to_string())
        );
        assert_eq!(
            monthly_recap_due(wib(2026, 7, 1, 12, 59), Some(8)),
            Some("monthly_recap:2026-06".to_string())
        );
        // Before hour: not yet.
        assert_eq!(monthly_recap_due(wib(2026, 7, 1, 7, 59), Some(8)), None);
        // Past grace window.
        assert_eq!(monthly_recap_due(wib(2026, 7, 1, 13, 0), Some(8)), None);
        // Day 2: never.
        assert_eq!(monthly_recap_due(wib(2026, 7, 2, 8, 0), Some(8)), None);
        // Disabled.
        assert_eq!(monthly_recap_due(wib(2026, 7, 1, 8, 0), None), None);
    }

    #[test]
    fn monthly_recap_due_january_first_keys_to_december() {
        // 2027-01-01; prior month is 2026-12.
        assert_eq!(
            monthly_recap_due(wib(2027, 1, 1, 8, 0), Some(8)),
            Some("monthly_recap:2026-12".to_string())
        );
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

    #[test]
    fn evening_review_due_inside_the_window_only() {
        // default hour 21, grace 5h → due 21:00..02:00-clamped (window does not wrap).
        assert_eq!(evening_review_due(wib(2026, 6, 12, 20, 59), Some(21)), None);
        assert_eq!(
            evening_review_due(wib(2026, 6, 12, 21, 0), Some(21)),
            Some("evening_review:2026-06-12".to_string())
        );
        assert_eq!(
            evening_review_due(wib(2026, 6, 12, 23, 59), Some(21)),
            Some("evening_review:2026-06-12".to_string())
        );
        // Disabled.
        assert_eq!(evening_review_due(wib(2026, 6, 12, 21, 30), None), None);
    }

    #[tokio::test]
    async fn run_once_claims_and_survives_an_empty_db_without_a_client() {
        // With no telegram link, run_once must be a clean no-op.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let config = ProactiveConfig {
            briefing_hour: Some(0), // "always due" for this test
            recap_hour: Some(0),
            evening_review_hour: Some(0),
            monthly_recap_hour: Some(0),
            // None: keep this no-link smoke test from doing a network digest fetch
            news_digest_hour: None,
            mover_alert_pct: 5.0,
            milestone_step_idr: 50_000_000,
            hl_drawdown_pct: 15.0,
        };
        let client = TelegramClient::new("dummy-token".into());
        run_once(&db, &client, &config).await.unwrap();
        // No link -> nothing claimed.
        assert!(crate::repo::proactive_log::try_claim(&db, "briefing", &format!(
            "briefing:{}",
            chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m-%d")
        ))
        .await
        .unwrap());
    }
}
