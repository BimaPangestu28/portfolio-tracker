use crate::db::Db;
use crate::domain::models::{Transaction, TxnType};
use crate::repo::dec;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
pub struct NewTransaction {
    pub account_id: i64,
    pub instrument_id: i64,
    pub txn_type: String,
    pub executed_at: DateTime<Utc>,
    pub quantity: String,
    pub price_native: String,
    pub fee_native: Option<String>,
    pub currency: String,
    pub fx_to_idr: String,
    pub fx_to_usd: String,
    pub note: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TxnRowRaw {
    id: i64, account_id: i64, instrument_id: i64, txn_type: String,
    executed_at: String, quantity: String, price_native: String, fee_native: String,
    currency: String, fx_to_idr: String, fx_to_usd: String, note: Option<String>,
}

impl TxnRowRaw {
    fn into_domain(self) -> anyhow::Result<Transaction> {
        Ok(Transaction {
            id: self.id, account_id: self.account_id, instrument_id: self.instrument_id,
            txn_type: TxnType::from_str(&self.txn_type).map_err(|e| anyhow::anyhow!(e))?,
            executed_at: DateTime::parse_from_rfc3339(&self.executed_at)?.with_timezone(&Utc),
            quantity: dec(&self.quantity)?, price_native: dec(&self.price_native)?,
            fee_native: dec(&self.fee_native)?, currency: self.currency,
            fx_to_idr: dec(&self.fx_to_idr)?, fx_to_usd: dec(&self.fx_to_usd)?, note: self.note,
        })
    }
}

pub async fn create(db: &Db, t: &NewTransaction) -> anyhow::Result<Transaction> {
    TxnType::from_str(&t.txn_type).map_err(|e| anyhow::anyhow!(e))?;
    // Validate all decimal fields up-front so a malformed value never persists.
    crate::repo::dec(&t.quantity)?;
    crate::repo::dec(&t.price_native)?;
    if let Some(f) = t.fee_native.as_deref() { crate::repo::dec(f)?; }
    crate::repo::dec(&t.fx_to_idr)?;
    crate::repo::dec(&t.fx_to_usd)?;
    let now = Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO txn (account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(t.account_id).bind(t.instrument_id).bind(&t.txn_type)
        .bind(t.executed_at.to_rfc3339()).bind(&t.quantity).bind(&t.price_native)
        .bind(t.fee_native.clone().unwrap_or_else(|| "0".into()))
        .bind(&t.currency).bind(&t.fx_to_idr).bind(&t.fx_to_usd).bind(&t.note).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<Transaction> {
    let raw = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE id = ?")
        .bind(id).fetch_one(db).await?;
    raw.into_domain()
}

pub async fn list_all(db: &Db) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn ORDER BY executed_at")
        .fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

pub async fn list_for_instrument(db: &Db, instrument_id: i64) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE instrument_id = ? ORDER BY executed_at")
        .bind(instrument_id).fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM txn WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments};
    use rust_decimal_macros::dec as d;

    #[tokio::test]
    async fn insert_and_load_transactions_as_domain() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"Bitcoin".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        let nt = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"0.5".into(), price_native:"100".into(),
            fee_native: Some("1".into()), currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None };
        create(&db, &nt).await.unwrap();
        let all = list_for_instrument(&db, ins.id).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].quantity, d!(0.5));
        assert_eq!(all[0].fee_native, d!(1));
    }

    #[tokio::test]
    async fn create_rejects_bad_decimal_without_persisting() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"B".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        let bad = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"not_a_number".into(), price_native:"100".into(),
            fee_native: None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None };
        assert!(create(&db, &bad).await.is_err());
        // No row must have been persisted.
        assert_eq!(list_all(&db).await.unwrap().len(), 0);
    }
}
