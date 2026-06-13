//! Single-row persistence for the Upwork connection (see migration 0015).
//! Tokens are stored already-encrypted by the caller (see upwork::crypto).

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
    pub earnings_cursor: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get(db: &Db) -> anyhow::Result<Option<IntegrationRow>> {
    Ok(sqlx::query_as::<_, IntegrationRow>("SELECT * FROM upwork_integration WHERE id = 1")
        .fetch_optional(db)
        .await?)
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
        "INSERT INTO upwork_integration
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
    sqlx::query("UPDATE upwork_integration SET access_token = ?, expiry = ?, updated_at = ? WHERE id = 1")
        .bind(enc_access_token).bind(expiry).bind(&now)
        .execute(db).await?;
    Ok(())
}

pub async fn set_cursor(db: &Db, cursor: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE upwork_integration SET earnings_cursor = ? WHERE id = 1")
        .bind(cursor)
        .execute(db).await?;
    Ok(())
}

pub async fn set_status(db: &Db, status: &str, last_error: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE upwork_integration SET status = ?, last_error = ?, updated_at = ? WHERE id = 1")
        .bind(status).bind(last_error).bind(&now)
        .execute(db).await?;
    Ok(())
}

pub async fn delete(db: &Db) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM upwork_integration WHERE id = 1").execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn upsert_get_cursor_status_delete() {
        let db = mem_db().await;
        assert!(get(&db).await.unwrap().is_none());
        upsert(&db, "enc-a", "enc-r", "2026-06-12T10:00:00+00:00", "scope").await.unwrap();
        upsert(&db, "enc-a2", "enc-r2", "2026-06-12T11:00:00+00:00", "scope").await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.access_token, "enc-a2");
        assert_eq!(row.status, "connected");

        set_cursor(&db, "cur-9").await.unwrap();
        set_status(&db, "error", Some("boom")).await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.earnings_cursor.as_deref(), Some("cur-9"));
        assert_eq!(row.status, "error");
        assert_eq!(row.last_error.as_deref(), Some("boom"));

        delete(&db).await.unwrap();
        assert!(get(&db).await.unwrap().is_none());
    }
}
