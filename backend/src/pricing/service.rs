use chrono::{DateTime, Duration, Utc};

/// A price is stale if older than `max_age_hours`.
pub fn is_stale(as_of: &str, now: DateTime<Utc>, max_age_hours: i64) -> bool {
    match DateTime::parse_from_rfc3339(as_of).or_else(|_| DateTime::parse_from_rfc3339(&format!("{as_of}T00:00:00+00:00"))) {
        Ok(t) => now.signed_duration_since(t.with_timezone(&Utc)) > Duration::hours(max_age_hours),
        Err(_) => true,
    }
}

/// Staleness window per quote source. Fund NAV (bibit) is T-1 and pauses over
/// weekends/market holidays, so it gets 4 days; everything else 24h.
pub fn stale_window_hours(source: &str) -> i64 {
    if source == "bibit" { 96 } else { 24 }
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
    fn bibit_quotes_get_a_four_day_stale_window() {
        assert_eq!(stale_window_hours("bibit"), 96);
        assert_eq!(stale_window_hours("yahoo"), 24);
        assert_eq!(stale_window_hours("coingecko"), 24);
        assert_eq!(stale_window_hours("manual"), 24);
    }

    #[test]
    fn fund_nav_three_days_old_is_fresh_five_days_is_stale() {
        let now = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap(); // Monday
        // NAV dated Friday-ish (3 days back) — fine under the 96h fund window.
        assert!(!is_stale("2026-06-05", now, stale_window_hours("bibit")));
        // 5 days back — stale even for funds.
        assert!(is_stale("2026-06-03", now, stale_window_hours("bibit")));
        // Same age under the default window — stale.
        assert!(is_stale("2026-06-05", now, stale_window_hours("yahoo")));
    }
}
