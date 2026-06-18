use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AccountRow {
    pub id: i64,
    pub name: String,
    pub account_type: String,
    pub institution: Option<String>,
    pub native_currency: String,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewAccount {
    pub name: String,
    pub account_type: String,
    pub institution: Option<String>,
    pub native_currency: String,
    pub note: Option<String>,
}

pub async fn create(db: &Db, a: &NewAccount) -> anyhow::Result<AccountRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO account (name, account_type, institution, native_currency, note, created_at) VALUES (?,?,?,?,?,?)")
        .bind(&a.name).bind(&a.account_type).bind(&a.institution)
        .bind(&a.native_currency).bind(&a.note).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<AccountRow> {
    let row = sqlx::query_as::<_, AccountRow>("SELECT * FROM account WHERE id = ?")
        .bind(id).fetch_one(db).await?;
    Ok(row)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<AccountRow>> {
    let rows = sqlx::query_as::<_, AccountRow>("SELECT * FROM account ORDER BY id")
        .fetch_all(db).await?;
    Ok(rows)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM account WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

/// Find an account by exact name match, returning `None` if no account with
/// that name exists.
pub async fn find_by_name(db: &Db, name: &str) -> anyhow::Result<Option<AccountRow>> {
    Ok(sqlx::query_as::<_, AccountRow>(
        "SELECT id, name, account_type, institution, native_currency, note, created_at
         FROM account WHERE name = ? LIMIT 1",
    )
    .bind(name)
    .fetch_optional(db)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn create_and_list_account() {
        let db = mem_db().await;
        let a = NewAccount { name: "Binance".into(), account_type: "exchange".into(),
            institution: None, native_currency: "USD".into(), note: None };
        let created = create(&db, &a).await.unwrap();
        assert_eq!(created.name, "Binance");
        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn find_by_name_returns_created_account() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(find_by_name(&db, "Hyperliquid").await.unwrap().is_none());
        create(&db, &NewAccount {
            name: "Hyperliquid".into(),
            account_type: "exchange".into(),
            institution: None,
            native_currency: "USD".into(),
            note: None,
        }).await.unwrap();
        let found = find_by_name(&db, "Hyperliquid").await.unwrap().expect("found");
        assert_eq!(found.name, "Hyperliquid");
        assert_eq!(found.native_currency, "USD");
    }
}
