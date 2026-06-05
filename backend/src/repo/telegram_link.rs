//! Persistence for the single Telegram owner link (see migration 0008).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TelegramLinkRow {
    pub chat_id: i64,
    pub username: Option<String>,
    pub linked_at: String,
}

/// The current owner link, or None when no Telegram chat is linked.
pub async fn get(db: &Db) -> anyhow::Result<Option<TelegramLinkRow>> {
    let row = sqlx::query_as::<_, TelegramLinkRow>(
        "SELECT chat_id, username, linked_at FROM telegram_link WHERE id = 1",
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Link (or re-link) the owner chat. Replaces any existing link.
pub async fn set(db: &Db, chat_id: i64, username: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO telegram_link (id, chat_id, username, linked_at) VALUES (1, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET chat_id = excluded.chat_id,
                                       username = excluded.username,
                                       linked_at = excluded.linked_at",
    )
    .bind(chat_id)
    .bind(username)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

/// Remove the owner link (unlink).
pub async fn clear(db: &Db) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM telegram_link WHERE id = 1")
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn get_returns_none_before_linking() {
        let db = mem_db().await;
        assert!(get(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn set_then_get_returns_the_link() {
        let db = mem_db().await;
        set(&db, 12345, Some("bima")).await.unwrap();
        let row = get(&db).await.unwrap().expect("link row");
        assert_eq!(row.chat_id, 12345);
        assert_eq!(row.username.as_deref(), Some("bima"));
        assert!(!row.linked_at.is_empty());
    }

    #[tokio::test]
    async fn set_replaces_an_existing_link() {
        let db = mem_db().await;
        set(&db, 111, Some("old")).await.unwrap();
        set(&db, 222, None).await.unwrap();
        let row = get(&db).await.unwrap().expect("link row");
        assert_eq!(row.chat_id, 222);
        assert_eq!(row.username, None);
    }

    #[tokio::test]
    async fn clear_removes_the_link() {
        let db = mem_db().await;
        set(&db, 111, None).await.unwrap();
        clear(&db).await.unwrap();
        assert!(get(&db).await.unwrap().is_none());
    }
}
