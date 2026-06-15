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
    pub priority: Option<String>,
    pub estimate_minutes: Option<i64>,
}

pub async fn create(
    db: &Db,
    title: &str,
    notes: Option<&str>,
    due_at: Option<&str>,
    priority: Option<&str>,
    estimate_minutes: Option<i64>,
) -> anyhow::Result<TodoRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO todos (title, notes, due_at, status, created_at, priority, estimate_minutes) \
         VALUES (?, ?, ?, 'open', ?, ?, ?)",
    )
    .bind(title)
    .bind(notes)
    .bind(due_at)
    .bind(&now)
    .bind(priority)
    .bind(estimate_minutes)
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

/// Move open todos forward one day. With `ids = None`, rolls every open todo
/// whose due date (WIB) is today or earlier. With explicit `ids`, rolls only
/// those — still skipping undated or future-dated todos. Time-of-day and the
/// stored Z-format are preserved. Returns the moved rows (in id order).
pub async fn rollover(
    db: &Db,
    ids: Option<&[i64]>,
    now_utc: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Vec<TodoRow>> {
    let today_wib = now_utc
        .with_timezone(&crate::assistant::time::wib())
        .format("%Y-%m-%d")
        .to_string();
    let mut moved = Vec::new();
    for todo in list_open(db).await? {
        if let Some(allow) = ids {
            if !allow.contains(&todo.id) {
                continue;
            }
        }
        let Some(due_at) = &todo.due_at else { continue };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(due_at) else { continue };
        let due_date_wib = parsed
            .with_timezone(&crate::assistant::time::wib())
            .format("%Y-%m-%d")
            .to_string();
        if due_date_wib.as_str() > today_wib.as_str() {
            continue; // future due dates are left untouched
        }
        let new_due = parsed.with_timezone(&chrono::Utc) + chrono::Duration::days(1);
        let new_due_db = crate::assistant::time::to_db_utc(new_due);
        sqlx::query("UPDATE todos SET due_at = ? WHERE id = ? AND status = 'open'")
            .bind(&new_due_db)
            .bind(todo.id)
            .execute(db)
            .await?;
        moved.push(get(db, todo.id).await?);
    }
    moved.sort_by_key(|t| t.id);
    Ok(moved)
}

/// List todos by status: "open", "done", or "all".
pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<TodoRow>> {
    let rows = match status {
        "all" => {
            sqlx::query_as::<_, TodoRow>(
                "SELECT * FROM todos ORDER BY (status = 'done'), due_at IS NULL, due_at, id",
            )
            .fetch_all(db)
            .await?
        }
        other => {
            sqlx::query_as::<_, TodoRow>(
                "SELECT * FROM todos WHERE status = ? ORDER BY due_at IS NULL, due_at, id",
            )
            .bind(other)
            .fetch_all(db)
            .await?
        }
    };
    Ok(rows)
}

/// Full-replace editable fields of a todo. Returns false if the id is absent.
pub async fn update(
    db: &Db,
    id: i64,
    title: &str,
    notes: Option<&str>,
    due_at: Option<&str>,
    priority: Option<&str>,
    estimate_minutes: Option<i64>,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE todos SET title = ?, notes = ?, due_at = ?, priority = ?, estimate_minutes = ? WHERE id = ?",
    )
    .bind(title)
    .bind(notes)
    .bind(due_at)
    .bind(priority)
    .bind(estimate_minutes)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Reopen a done todo (done -> open, clear completed_at). False if not currently done.
pub async fn reopen(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE todos SET status = 'open', completed_at = NULL WHERE id = ? AND status = 'done'",
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
    async fn create_then_get_round_trips() {
        let db = mem_db().await;
        let todo = create(
            &db,
            "bayar listrik",
            Some("token PLN"),
            Some("2026-06-12T02:00:00Z"),
            Some("high"),
            Some(30),
        )
        .await
        .unwrap();
        assert_eq!(todo.title, "bayar listrik");
        assert_eq!(todo.notes.as_deref(), Some("token PLN"));
        assert_eq!(todo.due_at.as_deref(), Some("2026-06-12T02:00:00Z"));
        assert_eq!(todo.status, "open");
        assert!(todo.completed_at.is_none());
        assert_eq!(todo.priority.as_deref(), Some("high"));
        assert_eq!(todo.estimate_minutes, Some(30));
        let fetched = get(&db, todo.id).await.unwrap();
        assert_eq!(fetched.id, todo.id);
    }

    #[tokio::test]
    async fn list_open_orders_by_due_then_id_and_excludes_done() {
        let db = mem_db().await;
        let no_due = create(&db, "no due", None, None, None, None).await.unwrap();
        let later = create(&db, "later", None, Some("2026-06-20T00:00:00Z"), None, None).await.unwrap();
        let sooner = create(&db, "sooner", None, Some("2026-06-12T00:00:00Z"), None, None).await.unwrap();
        let finished = create(&db, "done already", None, None, None, None).await.unwrap();
        complete(&db, finished.id).await.unwrap();

        let open = list_open(&db).await.unwrap();
        let ids: Vec<i64> = open.iter().map(|t| t.id).collect();
        // Dated todos first (earliest first), undated last; done excluded.
        assert_eq!(ids, vec![sooner.id, later.id, no_due.id]);
    }

    #[tokio::test]
    async fn complete_marks_done_once() {
        let db = mem_db().await;
        let todo = create(&db, "x", None, None, None, None).await.unwrap();
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
    async fn rollover_shifts_overdue_and_today_by_one_day_only() {
        let db = mem_db().await;
        // "now" = 2026-06-12T05:00:00Z == 12:00 WIB.
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let overdue = create(&db, "overdue", None, Some("2026-06-10T02:00:00Z"), None, None).await.unwrap();
        let today = create(&db, "today", None, Some("2026-06-12T02:00:00Z"), None, None).await.unwrap();
        let future = create(&db, "future", None, Some("2026-06-20T02:00:00Z"), None, None).await.unwrap();
        let undated = create(&db, "undated", None, None, None, None).await.unwrap();

        let moved = rollover(&db, None, now).await.unwrap();
        let moved_ids: Vec<i64> = moved.iter().map(|t| t.id).collect();
        assert_eq!(moved_ids, vec![overdue.id, today.id]);

        // due_at advanced by exactly one day, time-of-day preserved.
        let today_after = get(&db, today.id).await.unwrap();
        assert_eq!(today_after.due_at.as_deref(), Some("2026-06-13T02:00:00Z"));
        // future + undated untouched.
        assert_eq!(get(&db, future.id).await.unwrap().due_at.as_deref(), Some("2026-06-20T02:00:00Z"));
        assert_eq!(get(&db, undated.id).await.unwrap().due_at, None);
    }

    #[tokio::test]
    async fn rollover_with_explicit_ids_skips_others_and_future() {
        let db = mem_db().await;
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let a = create(&db, "a", None, Some("2026-06-10T02:00:00Z"), None, None).await.unwrap();
        let b = create(&db, "b", None, Some("2026-06-10T02:00:00Z"), None, None).await.unwrap();
        let future = create(&db, "future", None, Some("2026-06-20T02:00:00Z"), None, None).await.unwrap();

        let moved = rollover(&db, Some(&[a.id, future.id]), now).await.unwrap();
        let moved_ids: Vec<i64> = moved.iter().map(|t| t.id).collect();
        // a moved; future skipped (future due); b not in id list.
        assert_eq!(moved_ids, vec![a.id]);
        assert_eq!(get(&db, b.id).await.unwrap().due_at.as_deref(), Some("2026-06-10T02:00:00Z"));
    }

    #[tokio::test]
    async fn completed_since_and_created_count() {
        let db = mem_db().await;
        let a = create(&db, "old done", None, None, None, None).await.unwrap();
        complete(&db, a.id).await.unwrap();
        let b = create(&db, "new open", None, None, None, None).await.unwrap();
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
