use crate::db::Db;

pub async fn suggest_instrument(db: &Db, symbol: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(symbol) = LOWER(?) LIMIT 1")
        .bind(symbol).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
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
}
