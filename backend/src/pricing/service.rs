use chrono::{DateTime, Duration, Utc};

/// A price is stale if older than `max_age_hours`.
///
/// `as_of` is either a full RFC3339 timestamp or a date-only string
/// ("2026-06-18"). Date-only quotes are daily buckets — the live connectors
/// stamp every intraday refresh with today's UTC date, and funds publish a
/// date-only NAV — so a date-only value represents the quote *for that whole
/// day*. We anchor its freshness to the END of the day (next midnight UTC),
/// not the start: anchoring to the start backdated an afternoon refresh by up
/// to 24h, so the quote read as a full day old the instant UTC crossed midnight
/// (~07:00 WIB), spuriously flagging every auto-sourced position as stale each
/// morning before that day's first refresh re-stamped it.
pub fn is_stale(as_of: &str, now: DateTime<Utc>, max_age_hours: i64) -> bool {
    // Full timestamp (carries a time component): compare against it directly.
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(as_of) {
        return now.signed_duration_since(timestamp.with_timezone(&Utc)) > Duration::hours(max_age_hours);
    }
    // Date-only: anchor to the end of that UTC day (start of day + 24h).
    match DateTime::parse_from_rfc3339(&format!("{as_of}T00:00:00+00:00")) {
        Ok(start_of_day) => {
            let end_of_day = start_of_day.with_timezone(&Utc) + Duration::hours(24);
            now.signed_duration_since(end_of_day) > Duration::hours(max_age_hours)
        }
        Err(_) => true,
    }
}

/// Staleness window per quote source, in hours.
const FUND_STALE_HOURS: i64 = 144; // 6 days
const DEFAULT_STALE_HOURS: i64 = 24;

/// Fund NAV (bibit) is published T-1 with date-only as_of (midnight UTC) and
/// pauses over weekends, so a 6-day window keeps long weekends (e.g. a Monday
/// exchange holiday) from false-flagging. Week-long closures (Lebaran) will
/// still flag stale — by design, the data really is old. Everything else: 24h.
pub fn stale_window_hours(source: &str) -> i64 {
    if source == "bibit" { FUND_STALE_HOURS } else { DEFAULT_STALE_HOURS }
}

use crate::db::Db;
use crate::pricing::{coingecko::CoinGecko, fx::FxClient, PriceProvider};
use crate::repo::{instruments, prices};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Grams per troy ounce — gold futures (GC=F) quote in USD per troy oz,
/// while Indonesian gold (Antam/Pluang) is held in grams.
const GRAMS_PER_TROY_OUNCE: Decimal = dec!(31.1034768);

/// Convert a USD-per-troy-ounce gold quote into IDR per gram.
pub fn gold_idr_per_gram(usd_per_oz: Decimal, usd_idr: Decimal) -> Decimal {
    usd_per_oz / GRAMS_PER_TROY_OUNCE * usd_idr
}

/// Refresh latest prices for all instruments whose price_source is "coingecko:<id>".
/// Also refreshes USD/IDR FX. Failures are logged, not fatal.
pub async fn refresh_all(db: &Db) -> anyhow::Result<()> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let cg = CoinGecko::new();
    let fx = FxClient::new();
    let bibit = crate::pricing::bibit::BibitNav::new();

    // Keep the fresh rate around: the gold-derived source needs it below.
    // On fetch failure, fall back to the last stored rate.
    let usd_idr = match fx.usd_to_idr().await {
        Ok(rate) => {
            let _ = prices::upsert_fx(db, "USD", "IDR", rate, &today).await;
            Some(rate)
        }
        Err(e) => {
            tracing::warn!("fx refresh failed: {e}");
            prices::latest_fx(db, "USD", "IDR").await.ok().flatten()
        }
    };

    for ins in instruments::list(db).await? {
        if let Some(ext) = ins.price_source.strip_prefix("coingecko:") {
            // Quote in the instrument's native currency so cost basis and
            // latest price stay comparable (IDR for local-exchange crypto).
            let vs = crate::pricing::coingecko::vs_currency(&ins.native_currency);
            match cg.latest_in(ext, vs).await {
                Ok(q) => { let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "coingecko", &today).await; }
                Err(e) => tracing::warn!("price refresh failed for {}: {e}", ins.symbol),
            }
        }
        if let Some(ext) = ins.price_source.strip_prefix("yahoo:") {
            match crate::pricing::yahoo::Yahoo::new().latest(ext).await {
                Ok(q) => { let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "yahoo", &today).await; }
                Err(e) => tracing::warn!("yahoo price refresh failed for {}: {e}", ins.symbol),
            }
        }
        // Indonesian mutual fund NAV scraped from Bibit's public product page.
        // Store under the page's NAV date (T-1), not today — keeps staleness honest
        // and avoids duplicating the same NAV under multiple dates.
        if let Some(code) = ins.price_source.strip_prefix("bibit:") {
            match bibit.latest(code).await {
                Ok(q) => { let _ = prices::upsert_latest(db, ins.id, q.price, "IDR", "bibit", &q.as_of).await; }
                Err(e) => tracing::warn!("bibit nav refresh failed for {}: {e}", ins.symbol),
            }
        }
        // Derived source: gold futures (USD/oz via Yahoo) × USD/IDR → IDR/gram.
        if ins.price_source == "gold:idr_gram" {
            match (crate::pricing::yahoo::Yahoo::new().latest("GC=F").await, usd_idr) {
                (Ok(q), Some(rate)) => {
                    let price = gold_idr_per_gram(q.price, rate);
                    let _ = prices::upsert_latest(db, ins.id, price, "IDR", "gold-derived", &today).await;
                }
                (Err(e), _) => tracing::warn!("gold price refresh failed for {}: {e}", ins.symbol),
                (_, None) => tracing::warn!("gold price refresh for {} skipped: no USD/IDR rate", ins.symbol),
            }
        }
        // Hyperliquid account equity: price of the synthetic 1-unit instrument
        // equals the account's USD equity, pulled read-only from the bot API.
        if ins.price_source.starts_with("hyperliquid:") {
            if let Some(client) = crate::pricing::hyperliquid::BotClient::from_env() {
                match client.account_equity().await {
                    Ok(q) => {
                        let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "hyperliquid", &today).await;
                    }
                    Err(e) => tracing::warn!("hyperliquid equity refresh failed for {}: {e}", ins.symbol),
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use rust_decimal_macros::dec;

    #[test]
    fn gold_conversion_derives_idr_per_gram() {
        // 3380 USD/oz at 16300 IDR/USD → 3380 / 31.1034768 × 16300 ≈ Rp 1.771.313/gram
        let price = gold_idr_per_gram(dec!(3380), dec!(16300));
        assert_eq!(price.round_dp(0), dec!(1771313));
    }

    #[test]
    fn gold_conversion_zero_rate_gives_zero() {
        assert_eq!(gold_idr_per_gram(dec!(3380), dec!(0)), dec!(0));
    }

    #[test]
    fn fresh_price_not_stale() {
        let now = Utc.with_ymd_and_hms(2026,5,31,12,0,0).unwrap();
        assert!(!is_stale("2026-05-31T10:00:00+00:00", now, 24));
    }
    #[test]
    fn old_price_is_stale() {
        let now = Utc.with_ymd_and_hms(2026,5,31,12,0,0).unwrap();
        assert!(is_stale("2026-05-29", now, 24));
    }

    #[test]
    fn daily_quote_stamped_today_is_fresh_through_the_next_morning() {
        // Regression: the live connectors stamp every intraday refresh with the
        // current UTC *date* (date-only). A quote dated 2026-06-18 (fetched
        // mid-afternoon UTC) must NOT flag stale at ~07:00 WIB the next day —
        // 00:30 UTC on 2026-06-19 — before that day's first hourly refresh
        // re-stamps it. Anchoring a date-only as_of to the start of the day made
        // it read as a full 24h old at exactly UTC midnight, false-flagging every
        // auto-sourced position each morning even though the connector was fine.
        let next_morning_wib = Utc.with_ymd_and_hms(2026, 6, 19, 0, 30, 0).unwrap();
        assert!(!is_stale("2026-06-18", next_morning_wib, stale_window_hours("yahoo")));
        // A genuinely dead connector — no successful refresh for 2+ days — still flags.
        let two_days_later = Utc.with_ymd_and_hms(2026, 6, 20, 1, 0, 0).unwrap();
        assert!(is_stale("2026-06-18", two_days_later, stale_window_hours("yahoo")));
    }

    #[test]
    fn bibit_quotes_get_a_six_day_stale_window() {
        assert_eq!(stale_window_hours("bibit"), 144);
        assert_eq!(stale_window_hours("yahoo"), 24);
        assert_eq!(stale_window_hours("coingecko"), 24);
        assert_eq!(stale_window_hours("manual"), 24);
    }

    #[test]
    fn fund_nav_survives_a_monday_holiday_but_not_a_week() {
        // Friday NAV, single Monday holiday: next refresh lands Tuesday evening
        // WIB (~Tue 11:00 UTC) — 4d11h old, must NOT be stale for funds.
        let tue = Utc.with_ymd_and_hms(2026, 6, 9, 11, 0, 0).unwrap();
        assert!(!is_stale("2026-06-05", tue, stale_window_hours("bibit")));
        // Same quote under the default window — stale.
        assert!(is_stale("2026-06-05", tue, stale_window_hours("yahoo")));
        // Boundary: 144h measured from the END of the NAV's day (2026-06-06
        // 00:00 UTC), so exactly 2026-06-12 00:00 is fresh, just past it is stale.
        let exactly = Utc.with_ymd_and_hms(2026, 6, 12, 0, 0, 0).unwrap();
        assert!(!is_stale("2026-06-05", exactly, stale_window_hours("bibit")));
        let past = Utc.with_ymd_and_hms(2026, 6, 12, 0, 0, 1).unwrap();
        assert!(is_stale("2026-06-05", past, stale_window_hours("bibit")));
    }
}
