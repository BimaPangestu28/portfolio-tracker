//! Persistence for assistant reminders (see migration 0010).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReminderRow {
    pub id: i64,
    pub todo_id: Option<i64>,
    pub message: String,
    pub remind_at: String,
    pub recurrence: String,
    pub status: String,
    pub sent_at: Option<String>,
    /// Set when this reminder is the automatic pre-event reminder of an
    /// agenda event; cancelled together with the event.
    pub event_id: Option<i64>,
}

pub async fn create(
    db: &Db,
    todo_id: Option<i64>,
    message: &str,
    remind_at: &str,
    recurrence: &str,
) -> anyhow::Result<ReminderRow> {
    let id = sqlx::query(
        "INSERT INTO reminders (todo_id, message, remind_at, recurrence, status)
         VALUES (?, ?, ?, ?, 'pending')",
    )
    .bind(todo_id)
    .bind(message)
    .bind(remind_at)
    .bind(recurrence)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ReminderRow> {
    let row = sqlx::query_as::<_, ReminderRow>("SELECT * FROM reminders WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn list_pending(db: &Db) -> anyhow::Result<Vec<ReminderRow>> {
    let rows = sqlx::query_as::<_, ReminderRow>(
        "SELECT * FROM reminders WHERE status = 'pending' ORDER BY remind_at",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Pending reminders due at or before `now`. `now` must use the same
/// "%Y-%m-%dT%H:%M:%SZ" format as stored values so string <= is time <=.
pub async fn due(db: &Db, now: &str) -> anyhow::Result<Vec<ReminderRow>> {
    let rows = sqlx::query_as::<_, ReminderRow>(
        "SELECT * FROM reminders WHERE status = 'pending' AND remind_at <= ? ORDER BY remind_at",
    )
    .bind(now)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Cancel a pending reminder. Returns false when missing or not pending.
pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result =
        sqlx::query("UPDATE reminders SET status = 'cancelled' WHERE id = ? AND status = 'pending'")
            .bind(id)
            .execute(db)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Finalize a delivered one-shot reminder.
pub async fn mark_sent(db: &Db, id: i64, sent_at: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE reminders SET status = 'sent', sent_at = ? WHERE id = ?")
        .bind(sent_at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// How many reminders were delivered at/after `since` ("%Y-%m-%dT%H:%M:%SZ",
/// the format the delivery tick writes to sent_at).
pub async fn sent_count_since(db: &Db, since_z: &str) -> anyhow::Result<i64> {
    let row: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM reminders WHERE sent_at IS NOT NULL AND sent_at >= ?")
            .bind(since_z)
            .fetch_one(db)
            .await?;
    Ok(row.0)
}

/// Recurring delivery: stay pending, advance remind_at, record sent_at.
pub async fn reschedule(db: &Db, id: i64, next_remind_at: &str, sent_at: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE reminders SET remind_at = ?, sent_at = ? WHERE id = ?")
        .bind(next_remind_at)
        .bind(sent_at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// A pre-event reminder: one-shot, linked to its event for cascade-cancel.
pub async fn create_for_event(
    db: &Db,
    event_id: i64,
    message: &str,
    remind_at: &str,
) -> anyhow::Result<ReminderRow> {
    let id = sqlx::query(
        "INSERT INTO reminders (todo_id, message, remind_at, recurrence, status, event_id)
         VALUES (NULL, ?, ?, 'none', 'pending', ?)",
    )
    .bind(message)
    .bind(remind_at)
    .bind(event_id)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

/// Cancel the pending reminder(s) linked to an event. False when none were.
pub async fn cancel_by_event(db: &Db, event_id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE reminders SET status = 'cancelled' WHERE event_id = ? AND status = 'pending'",
    )
    .bind(event_id)
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
        let r = create(&db, None, "bayar listrik", "2026-06-12T02:00:00Z", "none")
            .await
            .unwrap();
        assert_eq!(r.message, "bayar listrik");
        assert_eq!(r.remind_at, "2026-06-12T02:00:00Z");
        assert_eq!(r.recurrence, "none");
        assert_eq!(r.status, "pending");
        assert!(r.todo_id.is_none() && r.sent_at.is_none());
        assert_eq!(get(&db, r.id).await.unwrap().id, r.id);
    }

    #[tokio::test]
    async fn due_returns_only_pending_at_or_before_now() {
        let db = mem_db().await;
        let past = create(&db, None, "past", "2026-06-10T00:00:00Z", "none").await.unwrap();
        let exact = create(&db, None, "exact", "2026-06-11T00:00:00Z", "none").await.unwrap();
        create(&db, None, "future", "2026-06-12T00:00:00Z", "none").await.unwrap();
        let cancelled = create(&db, None, "cancelled", "2026-06-10T00:00:00Z", "none").await.unwrap();
        cancel(&db, cancelled.id).await.unwrap();

        let due_rows = due(&db, "2026-06-11T00:00:00Z").await.unwrap();
        let ids: Vec<i64> = due_rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![past.id, exact.id]);
    }

    #[tokio::test]
    async fn cancel_only_works_on_pending() {
        let db = mem_db().await;
        let r = create(&db, None, "x", "2026-06-12T00:00:00Z", "none").await.unwrap();
        assert!(cancel(&db, r.id).await.unwrap());
        assert_eq!(get(&db, r.id).await.unwrap().status, "cancelled");
        assert!(!cancel(&db, r.id).await.unwrap());
        assert!(!cancel(&db, 999).await.unwrap());
    }

    #[tokio::test]
    async fn mark_sent_finalizes_a_one_shot() {
        let db = mem_db().await;
        let r = create(&db, None, "x", "2026-06-11T00:00:00Z", "none").await.unwrap();
        mark_sent(&db, r.id, "2026-06-11T00:01:00Z").await.unwrap();
        let row = get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "sent");
        assert_eq!(row.sent_at.as_deref(), Some("2026-06-11T00:01:00Z"));
    }

    #[tokio::test]
    async fn reschedule_keeps_recurring_pending_with_new_time() {
        let db = mem_db().await;
        let r = create(&db, None, "daily", "2026-06-11T00:00:00Z", "daily").await.unwrap();
        reschedule(&db, r.id, "2026-06-12T00:00:00Z", "2026-06-11T00:01:00Z").await.unwrap();
        let row = get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.remind_at, "2026-06-12T00:00:00Z");
        assert_eq!(row.sent_at.as_deref(), Some("2026-06-11T00:01:00Z"));
    }

    #[tokio::test]
    async fn list_pending_orders_by_remind_at() {
        let db = mem_db().await;
        let later = create(&db, None, "later", "2026-06-13T00:00:00Z", "none").await.unwrap();
        let sooner = create(&db, None, "sooner", "2026-06-12T00:00:00Z", "none").await.unwrap();
        let sent = create(&db, None, "sent", "2026-06-10T00:00:00Z", "none").await.unwrap();
        mark_sent(&db, sent.id, "2026-06-10T00:01:00Z").await.unwrap();

        let pending = list_pending(&db).await.unwrap();
        let ids: Vec<i64> = pending.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![sooner.id, later.id]);
    }

    #[tokio::test]
    async fn sent_count_since_counts_delivered_reminders() {
        let db = mem_db().await;
        let r = create(&db, None, "x", "2026-06-10T00:00:00Z", "none").await.unwrap();
        mark_sent(&db, r.id, "2026-06-10T08:00:00Z").await.unwrap();
        create(&db, None, "pending", "2099-01-01T00:00:00Z", "none").await.unwrap();
        assert_eq!(sent_count_since(&db, "2026-06-09T00:00:00Z").await.unwrap(), 1);
        assert_eq!(sent_count_since(&db, "2026-06-11T00:00:00Z").await.unwrap(), 0);
    }

    #[tokio::test]
    async fn create_for_event_links_and_flows_through_due() {
        let db = mem_db().await;
        let event = crate::repo::events::create(&db, "meeting", None, None, "2026-06-13T07:00:00Z")
            .await
            .unwrap();
        let r = create_for_event(&db, event.id, "📅 meeting — 30 menit lagi", "2026-06-13T06:30:00Z")
            .await
            .unwrap();
        assert_eq!(r.event_id, Some(event.id));
        assert!(r.todo_id.is_none());
        assert_eq!(r.recurrence, "none");
        let due_rows = due(&db, "2026-06-13T06:30:00Z").await.unwrap();
        assert_eq!(due_rows.len(), 1);
        assert_eq!(due_rows[0].event_id, Some(event.id));
    }

    #[tokio::test]
    async fn cancel_by_event_cancels_only_pending_linked_reminders() {
        let db = mem_db().await;
        let event = crate::repo::events::create(&db, "m", None, None, "2026-06-13T07:00:00Z")
            .await
            .unwrap();
        let linked = create_for_event(&db, event.id, "x", "2026-06-13T06:30:00Z").await.unwrap();
        let unlinked = create(&db, None, "y", "2026-06-13T06:30:00Z", "none").await.unwrap();
        assert!(cancel_by_event(&db, event.id).await.unwrap());
        assert_eq!(get(&db, linked.id).await.unwrap().status, "cancelled");
        assert_eq!(get(&db, unlinked.id).await.unwrap().status, "pending");
        // Second cancel finds nothing pending.
        assert!(!cancel_by_event(&db, event.id).await.unwrap());
    }
}
