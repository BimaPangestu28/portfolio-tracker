use crate::db::Db;
use serde::{Deserialize, Serialize};

const VALID_KINDS: &[&str] = &["evm_wallet", "binance", "mock"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ConnectorRow {
    pub id: i64,
    pub account_id: i64,
    pub kind: String,
    pub label: String,
    #[serde(skip)]
    pub config_json: String,
    pub cursor: Option<String>,
    pub last_synced_at: Option<String>,
    pub enabled: i64,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewConnector {
    pub account_id: i64,
    pub kind: String,
    pub label: String,
    pub config_json: String,
}

pub async fn create(db: &Db, n: &NewConnector) -> anyhow::Result<ConnectorRow> {
    if !VALID_KINDS.contains(&n.kind.as_str()) {
        anyhow::bail!("invalid connector kind '{}'; must be one of: {}", n.kind, VALID_KINDS.join(", "));
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO connector (account_id, kind, label, config_json, created_at) VALUES (?,?,?,?,?)")
        .bind(n.account_id).bind(&n.kind).bind(&n.label).bind(&n.config_json).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ConnectorRow> {
    let row = sqlx::query_as::<_, ConnectorRow>("SELECT * FROM connector WHERE id = ?")
        .bind(id).fetch_one(db).await?;
    Ok(row)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<ConnectorRow>> {
    let rows = sqlx::query_as::<_, ConnectorRow>("SELECT * FROM connector ORDER BY id")
        .fetch_all(db).await?;
    Ok(rows)
}

pub async fn list_enabled(db: &Db) -> anyhow::Result<Vec<ConnectorRow>> {
    let rows = sqlx::query_as::<_, ConnectorRow>("SELECT * FROM connector WHERE enabled = 1 ORDER BY id")
        .fetch_all(db).await?;
    Ok(rows)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM connector WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

pub async fn update_cursor(db: &Db, id: i64, cursor: &str) -> anyhow::Result<ConnectorRow> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE connector SET cursor = ?, last_synced_at = ? WHERE id = ?")
        .bind(cursor).bind(&now).bind(id)
        .execute(db).await?;
    get(db, id).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::accounts;

    #[tokio::test]
    async fn create_and_list() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount {
            name: "Wallet".into(), account_type: "wallet".into(),
            institution: None, native_currency: "USD".into(), note: None,
        }).await.unwrap();

        let conn = create(&db, &NewConnector {
            account_id: acc.id,
            kind: "evm_wallet".into(),
            label: "My ETH Wallet".into(),
            config_json: r#"{"address":"0xabc"}"#.into(),
        }).await.unwrap();

        assert_eq!(conn.kind, "evm_wallet");
        assert_eq!(conn.label, "My ETH Wallet");
        assert!(conn.cursor.is_none());
        assert!(conn.last_synced_at.is_none());
        assert_eq!(conn.enabled, 1);

        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);

        let enabled = list_enabled(&db).await.unwrap();
        assert_eq!(enabled.len(), 1);

        let updated = update_cursor(&db, conn.id, "cursor_val").await.unwrap();
        assert_eq!(updated.cursor.as_deref(), Some("cursor_val"));
        assert!(updated.last_synced_at.is_some());

        delete(&db, conn.id).await.unwrap();
        assert_eq!(list(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn create_rejects_invalid_kind() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount {
            name: "X".into(), account_type: "manual".into(),
            institution: None, native_currency: "USD".into(), note: None,
        }).await.unwrap();
        let result = create(&db, &NewConnector {
            account_id: acc.id,
            kind: "unknown_kind".into(),
            label: "Bad".into(),
            config_json: "{}".into(),
        }).await;
        assert!(result.is_err());
    }
}
