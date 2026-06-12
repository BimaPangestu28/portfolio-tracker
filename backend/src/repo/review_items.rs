use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReviewItemRow {
    pub id: i64,
    pub batch_id: String,
    pub source_kind: String,
    pub source_filename: String,
    pub source_path: String,
    pub doc_type: String,
    pub status: String,
    pub needs_attention: i64,
    pub payload_json: String,
    pub raw_llm_json: String,
    pub suggested_instrument_id: Option<i64>,
    pub suggested_account_id: Option<i64>,
    pub created_txn_id: Option<i64>,
    pub created_at: String,
    pub confirmed_at: Option<String>,
}

pub struct NewReviewItem<'a> {
    pub batch_id: &'a str,
    pub source_kind: &'a str,
    pub source_filename: &'a str,
    pub source_path: &'a str,
    pub doc_type: &'a str,
    pub needs_attention: bool,
    pub payload_json: &'a str,
    pub raw_llm_json: &'a str,
    pub suggested_instrument_id: Option<i64>,
    pub suggested_account_id: Option<i64>,
}

pub async fn create(db: &Db, n: &NewReviewItem<'_>) -> anyhow::Result<ReviewItemRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO review_item (batch_id, source_kind, source_filename, source_path, doc_type, status, needs_attention, payload_json, raw_llm_json, suggested_instrument_id, suggested_account_id, created_at)
         VALUES (?,?,?,?,?, 'pending', ?,?,?,?,?,?)")
        .bind(n.batch_id).bind(n.source_kind).bind(n.source_filename).bind(n.source_path)
        .bind(n.doc_type).bind(n.needs_attention as i64).bind(n.payload_json).bind(n.raw_llm_json)
        .bind(n.suggested_instrument_id).bind(n.suggested_account_id).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ReviewItemRow> {
    Ok(sqlx::query_as::<_, ReviewItemRow>("SELECT * FROM review_item WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<ReviewItemRow>> {
    Ok(sqlx::query_as::<_, ReviewItemRow>("SELECT * FROM review_item WHERE status = ? ORDER BY batch_id, id").bind(status).fetch_all(db).await?)
}

pub async fn update_payload(db: &Db, id: i64, payload_json: &str) -> anyhow::Result<ReviewItemRow> {
    sqlx::query("UPDATE review_item SET payload_json = ? WHERE id = ?").bind(payload_json).bind(id).execute(db).await?;
    get(db, id).await
}

/// Fill in a previously-unmatched account and/or instrument suggestion. A
/// `None` argument leaves that column untouched. Returns the refreshed row.
pub async fn set_suggestions(
    db: &Db,
    id: i64,
    account_id: Option<i64>,
    instrument_id: Option<i64>,
) -> anyhow::Result<ReviewItemRow> {
    if let Some(aid) = account_id {
        sqlx::query("UPDATE review_item SET suggested_account_id = ? WHERE id = ?")
            .bind(aid).bind(id).execute(db).await?;
    }
    if let Some(iid) = instrument_id {
        sqlx::query("UPDATE review_item SET suggested_instrument_id = ? WHERE id = ?")
            .bind(iid).bind(id).execute(db).await?;
    }
    get(db, id).await
}

pub async fn mark_confirmed(db: &Db, id: i64, created_txn_id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE review_item SET status='confirmed', created_txn_id=?, confirmed_at=? WHERE id=?")
        .bind(created_txn_id).bind(&now).bind(id).execute(db).await?;
    Ok(())
}

pub async fn mark_rejected(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE review_item SET status='rejected' WHERE id=?").bind(id).execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn set_suggestions_fills_only_provided_ids() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        // Insert prerequisite rows to satisfy FK constraints on review_item
        let account_a_id = sqlx::query(
            "INSERT INTO account (name, account_type, native_currency, created_at) VALUES (?,?,?,?)",
        )
        .bind("Acct A").bind("brokerage").bind("IDR").bind(&now)
        .execute(&db).await.unwrap().last_insert_rowid();

        let account_b_id = sqlx::query(
            "INSERT INTO account (name, account_type, native_currency, created_at) VALUES (?,?,?,?)",
        )
        .bind("Acct B").bind("brokerage").bind("IDR").bind(&now)
        .execute(&db).await.unwrap().last_insert_rowid();

        let instrument_a_id = sqlx::query(
            "INSERT INTO instrument (symbol, name, instrument_type, native_currency, price_source) VALUES (?,?,?,?,?)",
        )
        .bind("AAA").bind("Instrument A").bind("stock").bind("IDR").bind("manual")
        .execute(&db).await.unwrap().last_insert_rowid();

        let instrument_b_id = sqlx::query(
            "INSERT INTO instrument (symbol, name, instrument_type, native_currency, price_source) VALUES (?,?,?,?,?)",
        )
        .bind("BBB").bind("Instrument B").bind("stock").bind("IDR").bind("manual")
        .execute(&db).await.unwrap().last_insert_rowid();

        let item = create(&db, &NewReviewItem {
            batch_id: "b1", source_kind: "image", source_filename: "f.jpg",
            source_path: "", doc_type: "txn_history", needs_attention: false,
            payload_json: "{}", raw_llm_json: "{}",
            suggested_instrument_id: Some(instrument_a_id), suggested_account_id: None,
        }).await.unwrap();

        let updated = set_suggestions(&db, item.id, Some(account_a_id), None).await.unwrap();
        assert_eq!(updated.suggested_account_id, Some(account_a_id));
        assert_eq!(updated.suggested_instrument_id, Some(instrument_a_id), "instrument left unchanged");

        let updated = set_suggestions(&db, item.id, None, Some(instrument_b_id)).await.unwrap();
        assert_eq!(updated.suggested_account_id, Some(account_a_id), "account left unchanged");
        assert_eq!(updated.suggested_instrument_id, Some(instrument_b_id));

        // Verify account can also be updated independently
        let updated = set_suggestions(&db, item.id, Some(account_b_id), None).await.unwrap();
        assert_eq!(updated.suggested_account_id, Some(account_b_id));
        assert_eq!(updated.suggested_instrument_id, Some(instrument_b_id), "instrument still unchanged");
    }

    #[tokio::test]
    async fn create_list_and_status_transitions() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let row = create(&db, &NewReviewItem {
            batch_id: "b1", source_kind: "image", source_filename: "s.png", source_path: "data/uploads/b1/s.png",
            doc_type: "holdings_snapshot", needs_attention: false, payload_json: "{}", raw_llm_json: "{}",
            suggested_instrument_id: None, suggested_account_id: None,
        }).await.unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(list_by_status(&db, "pending").await.unwrap().len(), 1);
        update_payload(&db, row.id, "{\"x\":1}").await.unwrap();
        assert_eq!(get(&db, row.id).await.unwrap().payload_json, "{\"x\":1}");
        mark_rejected(&db, row.id).await.unwrap();
        assert_eq!(get(&db, row.id).await.unwrap().status, "rejected");
        assert_eq!(list_by_status(&db, "pending").await.unwrap().len(), 0);
    }
}
