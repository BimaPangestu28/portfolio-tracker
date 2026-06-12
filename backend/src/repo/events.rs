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
    pub source: String,
    pub google_event_id: Option<String>,
    pub google_etag: Option<String>,
    pub synced_at: Option<String>,
    pub updated_at: Option<String>,
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
        "INSERT INTO events (title, location, notes, start_at, status, source, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'scheduled', 'local', ?, ?)",
    )
    .bind(title)
    .bind(location)
    .bind(notes)
    .bind(start_at)
    .bind(&now)
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

/// Cancel a scheduled app-owned event. False when missing, already cancelled,
/// or foreign (source='google' rows are read-only to the assistant).
pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE events SET status = 'cancelled', updated_at = ?
         WHERE id = ? AND status = 'scheduled' AND source = 'local'",
    )
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// App-owned events whose local edits are not yet pushed: never synced, or
/// edited since the last successful sync.
pub async fn pending_push(db: &Db) -> anyhow::Result<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT * FROM events
         WHERE source = 'local'
           AND (google_event_id IS NULL OR synced_at IS NULL OR updated_at > synced_at)
         ORDER BY id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Record a successful push: store the Google id/etag and advance synced_at to now.
pub async fn mark_synced(db: &Db, id: i64, google_event_id: &str, etag: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE events SET google_event_id = ?, google_etag = ?, synced_at = ? WHERE id = ?",
    )
    .bind(google_event_id)
    .bind(etag)
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

/// Find an app row by Google event id (either local or foreign).
pub async fn get_by_google_id(db: &Db, google_event_id: &str) -> anyhow::Result<Option<EventRow>> {
    let row = sqlx::query_as::<_, EventRow>("SELECT * FROM events WHERE google_event_id = ?")
        .bind(google_event_id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

/// Insert or update a foreign (read-only) Google event, keyed by google id.
/// Returns the app row id.
pub async fn upsert_foreign(
    db: &Db,
    google_event_id: &str,
    title: &str,
    location: Option<&str>,
    notes: Option<&str>,
    start_at: &str,
    etag: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(existing) = get_by_google_id(db, google_event_id).await? {
        sqlx::query(
            "UPDATE events SET title = ?, location = ?, notes = ?, start_at = ?,
             google_etag = ?, synced_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(title).bind(location).bind(notes).bind(start_at)
        .bind(etag).bind(&now).bind(&now).bind(existing.id)
        .execute(db).await?;
        return Ok(existing.id);
    }
    let id = sqlx::query(
        "INSERT INTO events (title, location, notes, start_at, status, source,
            google_event_id, google_etag, synced_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'scheduled', 'google', ?, ?, ?, ?, ?)",
    )
    .bind(title).bind(location).bind(notes).bind(start_at)
    .bind(google_event_id).bind(etag).bind(&now).bind(&now).bind(&now)
    .execute(db).await?.last_insert_rowid();
    Ok(id)
}

/// Mark a row cancelled regardless of source — used by inbound sync when the
/// Google event was deleted. Distinct from the agent-facing `cancel`.
pub async fn cancel_by_sync(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE events SET status = 'cancelled', synced_at = ?, updated_at = ? WHERE id = ?")
        .bind(&now).bind(&now).bind(id)
        .execute(db).await?;
    Ok(())
}

/// Update an app-owned row from an inbound Google change (Google won this turn).
pub async fn update_from_google(
    db: &Db, id: i64, title: &str, location: Option<&str>, notes: Option<&str>,
    start_at: &str, etag: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE events SET title = ?, location = ?, notes = ?, start_at = ?,
         google_etag = ?, synced_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(title).bind(location).bind(notes).bind(start_at)
    .bind(etag).bind(&now).bind(&now).bind(id)
    .execute(db).await?;
    Ok(())
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

    #[tokio::test]
    async fn create_sets_local_source_and_updated_at() {
        let db = mem_db().await;
        let e = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        assert_eq!(e.source, "local");
        assert_eq!(e.updated_at.as_deref(), Some(e.created_at.as_str()));
        assert!(e.google_event_id.is_none());
    }

    #[tokio::test]
    async fn unsynced_local_then_marked_synced_drops_out_of_pending() {
        let db = mem_db().await;
        let e = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        assert_eq!(pending_push(&db).await.unwrap().len(), 1);
        mark_synced(&db, e.id, "gcal-1", "etag-1").await.unwrap();
        assert!(pending_push(&db).await.unwrap().is_empty());
        let got = get(&db, e.id).await.unwrap();
        assert_eq!(got.google_event_id.as_deref(), Some("gcal-1"));
        assert_eq!(got.google_etag.as_deref(), Some("etag-1"));
    }

    #[tokio::test]
    async fn upsert_foreign_inserts_then_updates_by_google_id() {
        let db = mem_db().await;
        let id = upsert_foreign(&db, "gid-9", "rapat A", None, None, "2026-06-13T03:00:00Z", "etag-a").await.unwrap();
        let again = upsert_foreign(&db, "gid-9", "rapat A (edit)", Some("zoom"), None, "2026-06-13T03:00:00Z", "etag-b").await.unwrap();
        assert_eq!(id, again, "same google id updates the same row");
        let row = get(&db, id).await.unwrap();
        assert_eq!(row.source, "google");
        assert_eq!(row.title, "rapat A (edit)");
        assert_eq!(row.location.as_deref(), Some("zoom"));
    }

    #[tokio::test]
    async fn cancel_refuses_foreign_events() {
        let db = mem_db().await;
        let id = upsert_foreign(&db, "gid-1", "foreign", None, None, "2026-06-13T03:00:00Z", "etag").await.unwrap();
        assert!(!cancel(&db, id).await.unwrap());
        assert_eq!(get(&db, id).await.unwrap().status, "scheduled");
    }
}
