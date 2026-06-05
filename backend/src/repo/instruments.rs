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

/// Find an existing instrument by case-insensitive symbol, or create a new one.
///
/// Used by the ingest confirm flow where two review rows for the same symbol
/// (e.g. two `ASII` trades from one screenshot) must collapse onto a single
/// instrument rather than spawning duplicates. Matching by symbol mirrors
/// `ingestion::matching::suggest_instrument`, which already treats symbol as the
/// instrument identity for this single-user tracker.
pub async fn find_or_create(db: &Db, i: &NewInstrument) -> anyhow::Result<InstrumentRow> {
    if let Some(existing) = find_by_symbol(db, &i.symbol).await? {
        return Ok(existing);
    }
    create(db, i).await
}

/// Look up an instrument by case-insensitive symbol.
pub async fn find_by_symbol(db: &Db, symbol: &str) -> anyhow::Result<Option<InstrumentRow>> {
    Ok(sqlx::query_as::<_, InstrumentRow>("SELECT * FROM instrument WHERE LOWER(symbol) = LOWER(?) LIMIT 1")
        .bind(symbol).fetch_optional(db).await?)
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

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    fn new_asii() -> NewInstrument {
        NewInstrument {
            symbol: "ASII".into(),
            name: "Astra International".into(),
            instrument_type: "stock".into(),
            native_currency: "IDR".into(),
            category_id: None,
            price_source: "manual".into(),
            decimals: Some(2),
            note: None,
        }
    }

    #[tokio::test]
    async fn find_or_create_reuses_existing_symbol_case_insensitive() {
        let db = mem_db().await;
        let first = find_or_create(&db, &new_asii()).await.unwrap();

        // A second confirm for the same symbol (different case) must not duplicate.
        let mut lower = new_asii();
        lower.symbol = "asii".into();
        let second = find_or_create(&db, &lower).await.unwrap();

        assert_eq!(first.id, second.id);
        assert_eq!(list(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn find_or_create_inserts_when_absent() {
        let db = mem_db().await;
        assert!(find_by_symbol(&db, "ASII").await.unwrap().is_none());
        let created = find_or_create(&db, &new_asii()).await.unwrap();
        assert_eq!(created.symbol, "ASII");
        assert_eq!(list(&db).await.unwrap().len(), 1);
    }
}
