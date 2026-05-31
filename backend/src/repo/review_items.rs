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
