//! Single-row persistence for the Google connection (see migration 0014).
//! Tokens are stored already-encrypted by the caller (see google::crypto).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IntegrationRow {
    pub id: i64,
    #[serde(skip)]
    pub access_token: String,
    #[serde(skip)]
    pub refresh_token: String,
    pub expiry: String,
    pub scope: String,
    pub calendar_sync_token: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub last_synced_at: Option<String>,
}

pub async fn get(db: &Db) -> anyhow::Result<Option<IntegrationRow>> {
    let row = sqlx::query_as::<_, IntegrationRow>("SELECT * FROM google_integration WHERE id = 1")
        .fetch_optional(db)
        .await?;
    Ok(row)
}

/// Insert or replace the single connection row, resetting status to 'connected'.
pub async fn upsert(
    db: &Db,
    enc_access_token: &str,
    enc_refresh_token: &str,
    expiry: &str,
    scope: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO google_integration
            (id, access_token, refresh_token, expiry, scope, status, created_at, updated_at)
         VALUES (1, ?, ?, ?, ?, 'connected', ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            access_token = excluded.access_token,
            refresh_token = excluded.refresh_token,
            expiry = excluded.expiry,
            scope = excluded.scope,
            status = 'connected',
            last_error = NULL,
            updated_at = excluded.updated_at",
    )
    .bind(enc_access_token).bind(enc_refresh_token).bind(expiry).bind(scope)
    .bind(&now).bind(&now)
    .execute(db).await?;
    Ok(())
}

/// Persist a refreshed access token + expiry without touching the refresh token.
pub async fn update_access(db: &Db, enc_access_token: &str, expiry: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE google_integration SET access_token = ?, expiry = ?, updated_at = ? WHERE id = 1",
    )
    .bind(enc_access_token).bind(expiry).bind(&now)
    .execute(db).await?;
    Ok(())
}

pub async fn set_sync_token(db: &Db, token: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE google_integration SET calendar_sync_token = ? WHERE id = 1")
        .bind(token)
        .execute(db).await?;
    Ok(())
}

pub async fn clear_sync_token(db: &Db) -> anyhow::Result<()> {
    sqlx::query("UPDATE google_integration SET calendar_sync_token = NULL WHERE id = 1")
        .execute(db).await?;
    Ok(())
}

pub async fn set_status(db: &Db, status: &str, last_error: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE google_integration SET status = ?, last_error = ?, updated_at = ? WHERE id = 1")
        .bind(status).bind(last_error).bind(&now)
        .execute(db).await?;
    Ok(())
}

/// Record the timestamp of a completed successful sync pass.
pub async fn set_synced_at(db: &Db, ts: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE google_integration SET last_synced_at = ? WHERE id = 1")
        .bind(ts)
        .execute(db).await?;
    Ok(())
}

pub async fn delete(db: &Db) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM google_integration WHERE id = 1").execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn get_is_none_before_connect() {
        let db = mem_db().await;
        assert!(get(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips_and_stays_single_row() {
        let db = mem_db().await;
        upsert(&db, "enc-access", "enc-refresh", "2026-06-12T10:00:00+00:00", "calendar.events").await.unwrap();
        upsert(&db, "enc-access2", "enc-refresh2", "2026-06-12T11:00:00+00:00", "calendar.events").await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.access_token, "enc-access2");
        assert_eq!(row.status, "connected");
    }

    #[tokio::test]
    async fn sync_token_and_status_and_delete() {
        let db = mem_db().await;
        upsert(&db, "a", "r", "2026-06-12T10:00:00+00:00", "calendar.events").await.unwrap();
        set_sync_token(&db, "tok-123").await.unwrap();
        set_status(&db, "error", Some("invalid_grant")).await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.calendar_sync_token.as_deref(), Some("tok-123"));
        assert_eq!(row.status, "error");
        assert_eq!(row.last_error.as_deref(), Some("invalid_grant"));
        delete(&db).await.unwrap();
        assert!(get(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn set_synced_at_records_timestamp() {
        let db = mem_db().await;
        upsert(&db, "a", "r", "2026-06-12T10:00:00+00:00", "calendar.events").await.unwrap();
        assert!(get(&db).await.unwrap().unwrap().last_synced_at.is_none());
        set_synced_at(&db, "2026-06-13T03:00:00+00:00").await.unwrap();
        assert_eq!(get(&db).await.unwrap().unwrap().last_synced_at.as_deref(), Some("2026-06-13T03:00:00+00:00"));
    }
}
