//! Persistence for invoice clients (see migration 0017).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ClientRow {
    pub id: i64,
    pub name: String,
    pub sub_name: Option<String>,
    pub website: Option<String>,
    pub created_at: String,
}

pub struct NewClient<'a> {
    pub name: &'a str,
    pub sub_name: Option<&'a str>,
    pub website: Option<&'a str>,
}

pub async fn create(db: &Db, c: &NewClient<'_>) -> anyhow::Result<ClientRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO client (name, sub_name, website, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(c.name).bind(c.sub_name).bind(c.website).bind(&now)
    .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ClientRow> {
    Ok(sqlx::query_as::<_, ClientRow>("SELECT * FROM client WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

pub async fn get_by_name(db: &Db, name: &str) -> anyhow::Result<Option<ClientRow>> {
    Ok(sqlx::query_as::<_, ClientRow>("SELECT * FROM client WHERE name = ? COLLATE NOCASE LIMIT 1")
        .bind(name).fetch_optional(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<ClientRow>> {
    Ok(sqlx::query_as::<_, ClientRow>("SELECT * FROM client ORDER BY name")
        .fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_then_get_by_name_is_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let made = create(&db, &NewClient { name: "PT AIS", sub_name: Some("AIS Helicopter"), website: Some("www.aishelicopter.com") }).await.unwrap();
        assert_eq!(made.name, "PT AIS");
        let found = get_by_name(&db, "pt ais").await.unwrap().expect("case-insensitive match");
        assert_eq!(found.id, made.id);
        assert_eq!(found.sub_name.as_deref(), Some("AIS Helicopter"));
        assert!(get_by_name(&db, "Unknown Co").await.unwrap().is_none());
        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
