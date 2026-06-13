//! Maps a synced Upwork contract to the ClickUp List created for it (migration
//! 0018). Row presence = "already synced"; written only after a successful
//! ClickUp create (claim-after-success).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LinkRow {
    pub contract_id: String,
    pub clickup_list_id: String,
    pub name: String,
    pub created_at: String,
}

pub async fn get(db: &Db, contract_id: &str) -> anyhow::Result<Option<LinkRow>> {
    Ok(sqlx::query_as::<_, LinkRow>("SELECT * FROM upwork_project_link WHERE contract_id = ?")
        .bind(contract_id)
        .fetch_optional(db)
        .await?)
}

pub async fn link(db: &Db, contract_id: &str, clickup_list_id: &str, name: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO upwork_project_link (contract_id, clickup_list_id, name, created_at) VALUES (?,?,?,?)",
    )
    .bind(contract_id).bind(clickup_list_id).bind(name).bind(&now)
    .execute(db).await?;
    Ok(())
}

/// Used by tests now; reserved for a future task-sync sub-project (spec §7).
#[cfg_attr(not(test), allow(dead_code))]
pub async fn list_all(db: &Db) -> anyhow::Result<Vec<LinkRow>> {
    Ok(sqlx::query_as::<_, LinkRow>("SELECT * FROM upwork_project_link ORDER BY created_at")
        .fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn link_then_get_round_trips_and_unknown_is_none() {
        let db = mem_db().await;
        assert!(get(&db, "c1").await.unwrap().is_none());
        link(&db, "c1", "list-9", "Acme — Build API").await.unwrap();
        let row = get(&db, "c1").await.unwrap().unwrap();
        assert_eq!(row.clickup_list_id, "list-9");
        assert_eq!(row.name, "Acme — Build API");
        assert_eq!(list_all(&db).await.unwrap().len(), 1);
    }
}
