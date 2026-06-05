use chrono::{DateTime, Duration, Utc};

/// A price is stale if older than `max_age_hours`.
pub fn is_stale(as_of: &str, now: DateTime<Utc>, max_age_hours: i64) -> bool {
    match DateTime::parse_from_rfc3339(as_of).or_else(|_| DateTime::parse_from_rfc3339(&format!("{as_of}T00:00:00+00:00"))) {
        Ok(t) => now.signed_duration_since(t.with_timezone(&Utc)) > Duration::hours(max_age_hours),
        Err(_) => true,
    }
}

use crate::db::Db;
use crate::pricing::{coingecko::CoinGecko, fx::FxClient, PriceProvider};
use crate::repo::{instruments, prices};

/// Refresh latest prices for all instruments whose price_source is "coingecko:<id>".
/// Also refreshes USD/IDR FX. Failures are logged, not fatal.
pub async fn refresh_all(db: &Db) -> anyhow::Result<()> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let cg = CoinGecko::new();
    let fx = FxClient::new();

    match fx.usd_to_idr().await {
        Ok(rate) => { let _ = prices::upsert_fx(db, "USD", "IDR", rate, &today).await; }
        Err(e) => tracing::warn!("fx refresh failed: {e}"),
    }

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
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
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
}
