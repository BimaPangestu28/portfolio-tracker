//! Persistence for assistant todos (see migration 0010).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TodoRow {
    pub id: i64,
    pub title: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub async fn create(
    db: &Db,
    title: &str,
    notes: Option<&str>,
    due_at: Option<&str>,
) -> anyhow::Result<TodoRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO todos (title, notes, due_at, status, created_at) VALUES (?, ?, ?, 'open', ?)",
    )
    .bind(title)
    .bind(notes)
    .bind(due_at)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<TodoRow> {
    let row = sqlx::query_as::<_, TodoRow>("SELECT * FROM todos WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Open todos, earliest due first, undated last, then insertion order.
pub async fn list_open(db: &Db) -> anyhow::Result<Vec<TodoRow>> {
    let rows = sqlx::query_as::<_, TodoRow>(
        "SELECT * FROM todos WHERE status = 'open' ORDER BY due_at IS NULL, due_at, id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Done todos whose completion is at/after `since` (RFC3339 +00:00 format,
/// the same format `complete` writes).
pub async fn completed_since(db: &Db, since_rfc3339: &str) -> anyhow::Result<Vec<TodoRow>> {
    let rows = sqlx::query_as::<_, TodoRow>(
        "SELECT * FROM todos WHERE status = 'done' AND completed_at >= ? ORDER BY completed_at",
    )
    .bind(since_rfc3339)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// How many todos were created at/after `since` (RFC3339 +00:00 format).
pub async fn created_count_since(db: &Db, since_rfc3339: &str) -> anyhow::Result<i64> {
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM todos WHERE created_at >= ?")
        .bind(since_rfc3339)
        .fetch_one(db)
        .await?;
    Ok(row.0)
}

/// Mark a todo done. Returns false when the id doesn't exist or is already done.
pub async fn complete(db: &Db, id: i64) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE todos SET status = 'done', completed_at = ? WHERE id = ? AND status = 'open'",
    )
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let db = mem_db().await;
        let todo = create(&db, "bayar listrik", Some("token PLN"), Some("2026-06-12T02:00:00Z"))
            .await
            .unwrap();
        assert_eq!(todo.title, "bayar listrik");
        assert_eq!(todo.notes.as_deref(), Some("token PLN"));
        assert_eq!(todo.due_at.as_deref(), Some("2026-06-12T02:00:00Z"));
        assert_eq!(todo.status, "open");
        assert!(todo.completed_at.is_none());
        let fetched = get(&db, todo.id).await.unwrap();
        assert_eq!(fetched.id, todo.id);
    }

    #[tokio::test]
    async fn list_open_orders_by_due_then_id_and_excludes_done() {
        let db = mem_db().await;
        let no_due = create(&db, "no due", None, None).await.unwrap();
        let later = create(&db, "later", None, Some("2026-06-20T00:00:00Z")).await.unwrap();
        let sooner = create(&db, "sooner", None, Some("2026-06-12T00:00:00Z")).await.unwrap();
        let finished = create(&db, "done already", None, None).await.unwrap();
        complete(&db, finished.id).await.unwrap();

        let open = list_open(&db).await.unwrap();
        let ids: Vec<i64> = open.iter().map(|t| t.id).collect();
        // Dated todos first (earliest first), undated last; done excluded.
        assert_eq!(ids, vec![sooner.id, later.id, no_due.id]);
    }

    #[tokio::test]
    async fn complete_marks_done_once() {
        let db = mem_db().await;
        let todo = create(&db, "x", None, None).await.unwrap();
        assert!(complete(&db, todo.id).await.unwrap());
        let done = get(&db, todo.id).await.unwrap();
        assert_eq!(done.status, "done");
        assert!(done.completed_at.is_some());
        // Second completion is a no-op signalled by false.
        assert!(!complete(&db, todo.id).await.unwrap());
    }

    #[tokio::test]
    async fn complete_unknown_id_returns_false() {
        let db = mem_db().await;
        assert!(!complete(&db, 999).await.unwrap());
    }

    #[tokio::test]
    async fn completed_since_and_created_count() {
        let db = mem_db().await;
        let a = create(&db, "old done", None, None).await.unwrap();
        complete(&db, a.id).await.unwrap();
        let b = create(&db, "new open", None, None).await.unwrap();
        let _ = b;
        // Everything above happened "now", so a since-bound in the past
        // includes them and a future bound excludes them.
        let past = (chrono::Utc::now() - chrono::Duration::days(7)).to_rfc3339();
        let future = (chrono::Utc::now() + chrono::Duration::days(1)).to_rfc3339();
        assert_eq!(completed_since(&db, &past).await.unwrap().len(), 1);
        assert_eq!(completed_since(&db, &future).await.unwrap().len(), 0);
        assert_eq!(created_count_since(&db, &past).await.unwrap(), 2);
        assert_eq!(created_count_since(&db, &future).await.unwrap(), 0);
    }
}
