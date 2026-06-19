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

/// Partial update for an instrument. `symbol` and `native_currency` are
/// deliberately not editable: symbol is the instrument's identity (dedup key)
/// and changing the native currency would silently break cost-basis math.
///
/// `category_id` is a double Option: absent = leave unchanged, explicit null =
/// clear the category, value = assign it.
#[derive(Debug, Deserialize)]
pub struct UpdateInstrument {
    pub name: Option<String>,
    pub instrument_type: Option<String>,
    pub price_source: Option<String>,
    pub decimals: Option<i64>,
    #[serde(default, deserialize_with = "double_option")]
    pub category_id: Option<Option<i64>>,
}

/// Distinguish "field absent" (outer None, via serde default) from "field
/// explicitly null" (Some(None)): when the field IS present, wrap whatever was
/// deserialized — null or value — in the outer Some.
fn double_option<'de, D>(de: D) -> Result<Option<Option<i64>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    serde::Deserialize::deserialize(de).map(Some)
}

pub async fn update(db: &Db, id: i64, u: &UpdateInstrument) -> anyhow::Result<InstrumentRow> {
    let cur = get(db, id).await?;
    let name = u.name.clone().unwrap_or(cur.name);
    let instrument_type = u.instrument_type.clone().unwrap_or(cur.instrument_type);
    let price_source = u.price_source.clone().unwrap_or(cur.price_source);
    let decimals = u.decimals.unwrap_or(cur.decimals);
    let category_id = match u.category_id {
        Some(v) => v,
        None => cur.category_id,
    };
    sqlx::query("UPDATE instrument SET name=?, instrument_type=?, price_source=?, decimals=?, category_id=? WHERE id=?")
        .bind(&name).bind(&instrument_type).bind(&price_source).bind(decimals).bind(category_id).bind(id)
        .execute(db).await?;
    get(db, id).await
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // Instruments from ingest are referenced by review_item.suggested_instrument_id;
    // clear the suggestion first or the FK constraint rejects the delete.
    let mut tx = db.begin().await?;
    sqlx::query("UPDATE review_item SET suggested_instrument_id = NULL WHERE suggested_instrument_id = ?")
        .bind(id).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM instrument WHERE id = ?").bind(id).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(())
}

/// Number of ledger transactions referencing this instrument.
pub async fn txn_count(db: &Db, id: i64) -> anyhow::Result<i64> {
    let (n,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM txn WHERE instrument_id = ?")
        .bind(id).fetch_one(db).await?;
    Ok(n)
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
    async fn update_sets_category_and_price_source_keeping_other_fields() {
        let db = mem_db().await;
        let cat = crate::repo::categories::create(&db, &crate::repo::categories::NewCategory {
            name: "Saham IDX".into(), target_pct: "40".into(), tolerance_band_pct: None, sort_order: None, color: None,
        }).await.unwrap();
        let ins = create(&db, &new_asii()).await.unwrap();

        let updated = update(&db, ins.id, &UpdateInstrument {
            name: None,
            instrument_type: Some("stock_id".into()),
            price_source: Some("yahoo:ASII.JK".into()),
            decimals: None,
            category_id: Some(Some(cat.id)),
        }).await.unwrap();

        assert_eq!(updated.category_id, Some(cat.id));
        assert_eq!(updated.price_source, "yahoo:ASII.JK");
        assert_eq!(updated.instrument_type, "stock_id");
        // untouched fields keep their values
        assert_eq!(updated.symbol, "ASII");
        assert_eq!(updated.name, "Astra International");
        assert_eq!(updated.decimals, 2);
    }

    #[tokio::test]
    async fn update_can_clear_category() {
        let db = mem_db().await;
        let cat = crate::repo::categories::create(&db, &crate::repo::categories::NewCategory {
            name: "Saham".into(), target_pct: "40".into(), tolerance_band_pct: None, sort_order: None, color: None,
        }).await.unwrap();
        let mut n = new_asii();
        n.category_id = Some(cat.id);
        let ins = create(&db, &n).await.unwrap();

        let updated = update(&db, ins.id, &UpdateInstrument {
            name: None, instrument_type: None, price_source: None, decimals: None,
            category_id: Some(None), // explicit null clears it
        }).await.unwrap();
        assert_eq!(updated.category_id, None);

        // absent field leaves category untouched
        let again = update(&db, ins.id, &UpdateInstrument {
            name: Some("Astra".into()), instrument_type: None, price_source: None,
            decimals: None, category_id: None,
        }).await.unwrap();
        assert_eq!(again.name, "Astra");
        assert_eq!(again.category_id, None);
    }

    #[tokio::test]
    async fn delete_clears_review_item_suggestion() {
        // Instruments created via ingest are referenced by
        // review_item.suggested_instrument_id; delete must clear the
        // suggestion instead of failing the FK constraint.
        let db = mem_db().await;
        let ins = create(&db, &new_asii()).await.unwrap();
        crate::repo::review_items::create(&db, &crate::repo::review_items::NewReviewItem {
            batch_id: "b", source_kind: "image", source_filename: "f.png", source_path: "p",
            doc_type: "txn_history", needs_attention: false, payload_json: "{}", raw_llm_json: "{}",
            suggested_instrument_id: Some(ins.id), suggested_account_id: None,
            external_ref: None,
        }).await.unwrap();

        delete(&db, ins.id).await.unwrap();

        let rows = sqlx::query_as::<_, (Option<i64>,)>("SELECT suggested_instrument_id FROM review_item")
            .fetch_all(&db).await.unwrap();
        assert_eq!(rows, vec![(None,)]);
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
