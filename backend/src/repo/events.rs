//! Persistence for agenda events (see migration 0013).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EventRow {
    pub id: i64,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub start_at: String,
    pub status: String,
    pub created_at: String,
}

pub async fn create(
    db: &Db,
    title: &str,
    location: Option<&str>,
    notes: Option<&str>,
    start_at: &str,
) -> anyhow::Result<EventRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO events (title, location, notes, start_at, status, created_at)
         VALUES (?, ?, ?, ?, 'scheduled', ?)",
    )
    .bind(title)
    .bind(location)
    .bind(notes)
    .bind(start_at)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<EventRow> {
    let row = sqlx::query_as::<_, EventRow>("SELECT * FROM events WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Scheduled events with start_at in [from_z, to_z), ordered by start time.
/// Bounds must use the Z format so string compare is time compare.
pub async fn list_between(db: &Db, from_z: &str, to_z: &str) -> anyhow::Result<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT * FROM events
         WHERE status = 'scheduled' AND start_at >= ? AND start_at < ?
         ORDER BY start_at",
    )
    .bind(from_z)
    .bind(to_z)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Cancel a scheduled event. False when missing or already cancelled.
pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result =
        sqlx::query("UPDATE events SET status = 'cancelled' WHERE id = ? AND status = 'scheduled'")
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
        let event = create(&db, "meeting vendor", Some("kantor"), None, "2026-06-13T07:00:00Z")
            .await
            .unwrap();
        assert_eq!(event.title, "meeting vendor");
        assert_eq!(event.location.as_deref(), Some("kantor"));
        assert!(event.notes.is_none());
        assert_eq!(event.start_at, "2026-06-13T07:00:00Z");
        assert_eq!(event.status, "scheduled");
        assert_eq!(get(&db, event.id).await.unwrap().id, event.id);
    }

    #[tokio::test]
    async fn list_between_is_inclusive_from_exclusive_to_and_skips_cancelled() {
        let db = mem_db().await;
        let at_from = create(&db, "at from", None, None, "2026-06-13T00:00:00Z").await.unwrap();
        let inside = create(&db, "inside", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        create(&db, "at to", None, None, "2026-06-14T00:00:00Z").await.unwrap();
        create(&db, "before", None, None, "2026-06-12T23:59:59Z").await.unwrap();
        let gone = create(&db, "cancelled", None, None, "2026-06-13T08:00:00Z").await.unwrap();
        cancel(&db, gone.id).await.unwrap();

        let events = list_between(&db, "2026-06-13T00:00:00Z", "2026-06-14T00:00:00Z")
            .await
            .unwrap();
        let ids: Vec<i64> = events.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![at_from.id, inside.id]);
    }

    #[tokio::test]
    async fn cancel_only_works_once_on_scheduled() {
        let db = mem_db().await;
        let event = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        assert!(cancel(&db, event.id).await.unwrap());
        assert_eq!(get(&db, event.id).await.unwrap().status, "cancelled");
        assert!(!cancel(&db, event.id).await.unwrap());
        assert!(!cancel(&db, 999).await.unwrap());
    }
}
