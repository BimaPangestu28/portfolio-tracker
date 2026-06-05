use crate::db::Db;
use crate::repo::dec;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct LatestPrice { pub price: Decimal, pub as_of: String, pub source: String }

pub async fn upsert_latest(db: &Db, instrument_id: i64, price: Decimal, currency: &str, source: &str, as_of: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO price_quote (instrument_id, as_of, price_native, currency, source, kind) VALUES (?,?,?,?,?, 'latest')
         ON CONFLICT(instrument_id, as_of, kind) DO UPDATE SET price_native=excluded.price_native, source=excluded.source")
        .bind(instrument_id).bind(as_of).bind(price.to_string()).bind(currency).bind(source)
        .execute(db).await?;
    Ok(())
}

pub async fn latest(db: &Db, instrument_id: i64) -> anyhow::Result<Option<LatestPrice>> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT price_native, as_of, source FROM price_quote WHERE instrument_id = ? AND kind='latest' ORDER BY as_of DESC LIMIT 1")
        .bind(instrument_id).fetch_optional(db).await?;
    match row {
        Some((p, as_of, source)) => Ok(Some(LatestPrice { price: dec(&p)?, as_of, source })),
        None => Ok(None),
    }
}

/// The latest two daily quotes (newest first) — the basis for day-change.
pub async fn last_two(db: &Db, instrument_id: i64) -> anyhow::Result<Vec<LatestPrice>> {
    let rows = sqlx::query_as::<_, (String, String, String)>(
        "SELECT price_native, as_of, source FROM price_quote WHERE instrument_id = ? AND kind='latest' ORDER BY as_of DESC LIMIT 2")
        .bind(instrument_id).fetch_all(db).await?;
    rows.into_iter()
        .map(|(p, as_of, source)| Ok(LatestPrice { price: dec(&p)?, as_of, source }))
        .collect()
}

pub async fn upsert_fx(db: &Db, base: &str, quote: &str, rate: Decimal, as_of: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO fx_rate (as_of, base, quote, rate) VALUES (?,?,?,?)
         ON CONFLICT(as_of, base, quote) DO UPDATE SET rate=excluded.rate")
        .bind(as_of).bind(base).bind(quote).bind(rate.to_string())
        .execute(db).await?;
    Ok(())
}

pub async fn latest_fx(db: &Db, base: &str, quote: &str) -> anyhow::Result<Option<Decimal>> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT rate FROM fx_rate WHERE base=? AND quote=? ORDER BY as_of DESC LIMIT 1")
        .bind(base).bind(quote).fetch_optional(db).await?;
    match row { Some((r,)) => Ok(Some(dec(&r)?)), None => Ok(None) }
}

pub async fn fx_on(db: &Db, base: &str, quote: &str, as_of: &str) -> anyhow::Result<Option<Decimal>> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT rate FROM fx_rate WHERE base=? AND quote=? AND as_of<=? ORDER BY as_of DESC LIMIT 1")
        .bind(base).bind(quote).bind(as_of).fetch_optional(db).await?;
    match row { Some((r,)) => Ok(Some(dec(&r)?)), None => Ok(None) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::instruments;
    use rust_decimal_macros::dec as d;

    #[tokio::test]
    async fn upsert_then_read_latest_price() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // price_quote has a FK to instrument(id), so we must create an instrument first
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
            native_currency: "USD".into(), category_id: None,
            price_source: "coingecko:bitcoin".into(), decimals: Some(8), note: None,
        }).await.unwrap();
        upsert_latest(&db, ins.id, d!(123.45), "USD", "coingecko", "2026-05-31").await.unwrap();
        upsert_latest(&db, ins.id, d!(130), "USD", "coingecko", "2026-06-01").await.unwrap();
        let p = latest(&db, ins.id).await.unwrap().unwrap();
        assert_eq!(p.price, d!(130));
    }

    #[tokio::test]
    async fn last_two_returns_newest_first() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "TLKM".into(), name: "Telkom".into(), instrument_type: "stock_id".into(),
            native_currency: "IDR".into(), category_id: None,
            price_source: "yahoo:TLKM.JK".into(), decimals: Some(0), note: None,
        }).await.unwrap();
        upsert_latest(&db, ins.id, d!(2800), "IDR", "yahoo", "2026-06-03").await.unwrap();
        upsert_latest(&db, ins.id, d!(2870), "IDR", "yahoo", "2026-06-04").await.unwrap();
        upsert_latest(&db, ins.id, d!(2900), "IDR", "yahoo", "2026-06-05").await.unwrap();
        let two = last_two(&db, ins.id).await.unwrap();
        assert_eq!(two.len(), 2);
        assert_eq!(two[0].price, d!(2900));
        assert_eq!(two[1].price, d!(2870));
    }

    #[tokio::test]
    async fn fx_round_trip() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert_fx(&db, "USD", "IDR", d!(16250), "2026-05-31").await.unwrap();
        assert_eq!(latest_fx(&db, "USD", "IDR").await.unwrap().unwrap(), d!(16250));
    }

    #[tokio::test]
    async fn fx_on_returns_rate_at_or_before_date() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert_fx(&db, "USD", "IDR", d!(15000), "2026-01-01").await.unwrap();
        upsert_fx(&db, "USD", "IDR", d!(16000), "2026-03-01").await.unwrap();
        // Exact date
        assert_eq!(fx_on(&db, "USD", "IDR", "2026-03-01").await.unwrap(), Some(d!(16000)));
        // Between rows -> most recent before
        assert_eq!(fx_on(&db, "USD", "IDR", "2026-02-15").await.unwrap(), Some(d!(15000)));
        // Before any row -> None
        assert_eq!(fx_on(&db, "USD", "IDR", "2025-12-31").await.unwrap(), None);
    }
}
