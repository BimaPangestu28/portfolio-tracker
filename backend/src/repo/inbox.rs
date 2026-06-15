//! Persistence for the GTD quick-capture inbox (see migration 0019).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InboxRow {
    pub id: i64,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

pub async fn create(db: &Db, content: &str) -> anyhow::Result<InboxRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query("INSERT INTO inbox (content, status, created_at) VALUES (?, 'pending', ?)")
        .bind(content)
        .bind(&now)
        .execute(db)
        .await?
        .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<InboxRow> {
    let row = sqlx::query_as::<_, InboxRow>("SELECT * FROM inbox WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Pending captures, oldest first.
pub async fn list_pending(db: &Db) -> anyhow::Result<Vec<InboxRow>> {
    let rows = sqlx::query_as::<_, InboxRow>(
        "SELECT * FROM inbox WHERE status = 'pending' ORDER BY id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Move the given pending items to `status` ('sorted' or 'dropped'), stamping
/// resolved_at. Only pending rows change. Returns the number of rows affected.
pub async fn resolve(db: &Db, ids: &[i64], status: &str) -> anyhow::Result<u64> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut affected = 0u64;
    for id in ids {
        let result = sqlx::query(
            "UPDATE inbox SET status = ?, resolved_at = ? WHERE id = ? AND status = 'pending'",
        )
        .bind(status)
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
        affected += result.rows_affected();
    }
    Ok(affected)
}

/// List inbox items by status: "pending", "sorted", or "all".
pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<InboxRow>> {
    let rows = match status {
        "all" => {
            sqlx::query_as::<_, InboxRow>("SELECT * FROM inbox ORDER BY id DESC")
                .fetch_all(db)
                .await?
        }
        other => {
            sqlx::query_as::<_, InboxRow>("SELECT * FROM inbox WHERE status = ? ORDER BY id DESC")
                .bind(other)
                .fetch_all(db)
                .await?
        }
    };
    Ok(rows)
}

/// Move a sorted inbox item back to pending. False if not currently sorted.
pub async fn unresolve(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE inbox SET status = 'pending', resolved_at = NULL WHERE id = ? AND status = 'sorted'",
    )
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
    async fn create_then_list_pending() {
        let db = mem_db().await;
        let a = create(&db, "beli kado").await.unwrap();
        let _b = create(&db, "meeting senin").await.unwrap();
        assert_eq!(a.status, "pending");
        assert!(a.resolved_at.is_none());
        let pending = list_pending(&db).await.unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].content, "beli kado");
    }

    #[tokio::test]
    async fn resolve_marks_only_listed_pending_rows() {
        let db = mem_db().await;
        let a = create(&db, "a").await.unwrap();
        let b = create(&db, "b").await.unwrap();
        let c = create(&db, "c").await.unwrap();
        let affected = resolve(&db, &[a.id, b.id], "sorted").await.unwrap();
        assert_eq!(affected, 2);
        let pending = list_pending(&db).await.unwrap();
        assert_eq!(pending.iter().map(|r| r.id).collect::<Vec<_>>(), vec![c.id]);
        let again = resolve(&db, &[a.id], "sorted").await.unwrap();
        assert_eq!(again, 0);
        assert!(get(&db, a.id).await.unwrap().resolved_at.is_some());
    }

    #[tokio::test]
    async fn resolve_dropped_removes_from_pending() {
        let db = mem_db().await;
        let a = create(&db, "junk").await.unwrap();
        resolve(&db, &[a.id], "dropped").await.unwrap();
        assert!(list_pending(&db).await.unwrap().is_empty());
        assert_eq!(get(&db, a.id).await.unwrap().status, "dropped");
    }
}
