use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ChatMessageRow {
    pub id: i64,
    pub role: String,
    pub content: String,
    pub channel: String,
    pub created_at: String,
}

pub async fn add(db: &Db, role: &str, content: &str, channel: &str) -> anyhow::Result<ChatMessageRow> {
    if !matches!(role, "user" | "assistant") {
        anyhow::bail!("invalid role '{}': must be 'user' or 'assistant'", role);
    }
    if !matches!(channel, "inapp" | "whatsapp" | "telegram") {
        anyhow::bail!("invalid channel '{}': must be 'inapp', 'whatsapp', or 'telegram'", channel);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO chat_message (role, content, channel, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(role)
    .bind(content)
    .bind(channel)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, ChatMessageRow>("SELECT * FROM chat_message WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn history(db: &Db, limit: i64) -> anyhow::Result<Vec<ChatMessageRow>> {
    let rows = sqlx::query_as::<_, ChatMessageRow>(
        "SELECT * FROM chat_message ORDER BY id ASC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn add_and_history_returns_in_order() {
        let db = mem_db().await;
        let user_msg = add(&db, "user", "What is my net worth?", "inapp").await.unwrap();
        assert_eq!(user_msg.role, "user");
        assert_eq!(user_msg.channel, "inapp");

        let assistant_msg = add(&db, "assistant", "Your net worth is Rp 4,875,000.", "inapp").await.unwrap();
        assert_eq!(assistant_msg.role, "assistant");

        let msgs = history(&db, 10).await.unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        // chronological order: user message id is lower
        assert!(msgs[0].id < msgs[1].id);
    }

    #[tokio::test]
    async fn invalid_role_returns_error() {
        let db = mem_db().await;
        let result = add(&db, "system", "hello", "inapp").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn telegram_channel_is_accepted() {
        let db = mem_db().await;
        let msg = add(&db, "user", "berapa net worth saya?", "telegram").await.unwrap();
        assert_eq!(msg.channel, "telegram");
    }

    #[tokio::test]
    async fn invalid_channel_returns_error() {
        let db = mem_db().await;
        let result = add(&db, "user", "hello", "email").await;
        assert!(result.is_err());
    }
}
