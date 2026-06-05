use crate::db::Db;

pub async fn suggest_instrument(db: &Db, symbol: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(symbol) = LOWER(?) LIMIT 1")
        .bind(symbol).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
}

/// Suggest an instrument for an extracted entry: exact symbol match first, then
/// exact name match. Mutual funds (e.g. Bibit) carry a fund name but no ticker,
/// so the name fallback is what makes their suggestions work at all.
pub async fn suggest_instrument_for_entry(db: &Db, symbol: Option<&str>, name: Option<&str>) -> anyhow::Result<Option<i64>> {
    if let Some(s) = symbol {
        if let Some(id) = suggest_instrument(db, s).await? {
            return Ok(Some(id));
        }
    }
    if let Some(n) = name {
        let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(name) = LOWER(?) LIMIT 1")
            .bind(n).fetch_optional(db).await?;
        return Ok(row.map(|(id,)| id));
    }
    Ok(None)
}

pub async fn suggest_account(db: &Db, name: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM account WHERE LOWER(name) = LOWER(?) LIMIT 1")
        .bind(name).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments};

    #[tokio::test]
    async fn suggests_instrument_by_symbol_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"Bitcoin".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        assert_eq!(suggest_instrument(&db, "btc").await.unwrap(), Some(ins.id));
        assert_eq!(suggest_instrument(&db, "ETH").await.unwrap(), None);
    }

    #[tokio::test]
    async fn suggests_account_by_name_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let a = accounts::create(&db, &accounts::NewAccount { name:"Binance".into(), account_type:"exchange".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        assert_eq!(suggest_account(&db, "binance").await.unwrap(), Some(a.id));
        assert_eq!(suggest_account(&db, "Indodax").await.unwrap(), None);
    }

    #[tokio::test]
    async fn suggest_instrument_for_entry_falls_back_to_name() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol:"SBF".into(), name:"Sucorinvest Bond Fund".into(),
            instrument_type:"mutual_fund".into(), native_currency:"IDR".into(),
            category_id:None, price_source:"manual".into(), decimals:Some(4), note:None,
        }).await.unwrap();
        // mutual funds: no symbol extracted -> exact name match, case-insensitive
        assert_eq!(suggest_instrument_for_entry(&db, None, Some("sucorinvest bond fund")).await.unwrap(), Some(ins.id));
        // symbol match still takes precedence
        assert_eq!(suggest_instrument_for_entry(&db, Some("sbf"), None).await.unwrap(), Some(ins.id));
        // unmatched symbol falls through to the name
        assert_eq!(suggest_instrument_for_entry(&db, Some("XXXX"), Some("Sucorinvest Bond Fund")).await.unwrap(), Some(ins.id));
        // nothing matches
        assert_eq!(suggest_instrument_for_entry(&db, Some("XXXX"), Some("Unknown Fund")).await.unwrap(), None);
        assert_eq!(suggest_instrument_for_entry(&db, None, None).await.unwrap(), None);
    }
}
