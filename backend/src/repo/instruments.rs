use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InstrumentRow {
    pub id: i64,
    pub symbol: String,
    pub name: String,
    pub instrument_type: String,
    pub native_currency: String,
    pub category_id: Option<i64>,
    pub price_source: String,
    pub decimals: i64,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewInstrument {
    pub symbol: String,
    pub name: String,
    pub instrument_type: String,
    pub native_currency: String,
    pub category_id: Option<i64>,
    pub price_source: String,
    pub decimals: Option<i64>,
    pub note: Option<String>,
}

pub async fn create(db: &Db, i: &NewInstrument) -> anyhow::Result<InstrumentRow> {
    let id = sqlx::query(
        "INSERT INTO instrument (symbol, name, instrument_type, native_currency, category_id, price_source, decimals, note) VALUES (?,?,?,?,?,?,?,?)")
        .bind(&i.symbol).bind(&i.name).bind(&i.instrument_type).bind(&i.native_currency)
        .bind(i.category_id).bind(&i.price_source).bind(i.decimals.unwrap_or(8)).bind(&i.note)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<InstrumentRow> {
    Ok(sqlx::query_as::<_, InstrumentRow>("SELECT * FROM instrument WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<InstrumentRow>> {
    Ok(sqlx::query_as::<_, InstrumentRow>("SELECT * FROM instrument ORDER BY id").fetch_all(db).await?)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM instrument WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}
