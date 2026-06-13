//! Persistence for generated invoices (see migration 0016). Write-once: line
//! items are stored as JSON, not a separate table.

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InvoiceRow {
    pub id: i64,
    pub number: String,
    pub client_id: i64,
    pub issue_date: String,
    pub due_date: String,
    pub subtotal: String,
    pub total: String,
    pub line_items_json: String,
    pub created_at: String,
}

pub struct NewInvoice<'a> {
    pub number: &'a str,
    pub client_id: i64,
    pub issue_date: &'a str,
    pub due_date: &'a str,
    pub subtotal: &'a str,
    pub total: &'a str,
    pub line_items_json: &'a str,
}

pub async fn insert(db: &Db, inv: &NewInvoice<'_>) -> anyhow::Result<InvoiceRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO invoice (number, client_id, issue_date, due_date, subtotal, total, line_items_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(inv.number).bind(inv.client_id).bind(inv.issue_date).bind(inv.due_date)
    .bind(inv.subtotal).bind(inv.total).bind(inv.line_items_json).bind(&now)
    .execute(db).await?.last_insert_rowid();
    Ok(sqlx::query_as::<_, InvoiceRow>("SELECT * FROM invoice WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

/// Highest NNN among invoices whose number starts with `prefix`
/// (e.g. "INV/2026/VI/"); None when the month has no invoices yet.
pub async fn max_seq_for_prefix(db: &Db, prefix: &str) -> anyhow::Result<Option<u32>> {
    // Prefix is app-built (`INV/<year>/<roman>/`) and never contains LIKE
    // wildcards, so no ESCAPE handling is needed.
    let pattern = format!("{prefix}%");
    let numbers: Vec<(String,)> =
        sqlx::query_as("SELECT number FROM invoice WHERE number LIKE ?")
            .bind(&pattern).fetch_all(db).await?;
    let max = numbers
        .iter()
        .filter_map(|(n,)| n.rsplit('/').next())
        .filter_map(|tail| tail.parse::<u32>().ok())
        .max();
    Ok(max)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_client(db: &Db) -> i64 {
        crate::repo::clients::create(db, &crate::repo::clients::NewClient {
            name: "PT AIS", sub_name: None, website: None,
        }).await.unwrap().id
    }

    #[tokio::test]
    async fn insert_then_max_seq_tracks_the_month_prefix() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let client_id = seed_client(&db).await;
        assert_eq!(max_seq_for_prefix(&db, "INV/2026/VI/").await.unwrap(), None);

        insert(&db, &NewInvoice {
            number: "INV/2026/VI/001", client_id, issue_date: "2026-06-11", due_date: "2026-06-25",
            subtotal: "12000000", total: "12000000", line_items_json: "[]",
        }).await.unwrap();
        insert(&db, &NewInvoice {
            number: "INV/2026/VI/002", client_id, issue_date: "2026-06-12", due_date: "2026-06-26",
            subtotal: "2000000", total: "2000000", line_items_json: "[]",
        }).await.unwrap();
        insert(&db, &NewInvoice {
            number: "INV/2026/VII/001", client_id, issue_date: "2026-07-01", due_date: "2026-07-15",
            subtotal: "500000", total: "500000", line_items_json: "[]",
        }).await.unwrap();

        assert_eq!(max_seq_for_prefix(&db, "INV/2026/VI/").await.unwrap(), Some(2));
        assert_eq!(max_seq_for_prefix(&db, "INV/2026/VII/").await.unwrap(), Some(1));
    }
}
