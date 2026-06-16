use crate::db::Db;
use serde::Serialize;

#[cfg(test)]
mod tests {
    use crate::db::Db;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn migration_creates_cs_tables() {
        let db = mem_db().await;
        // If the migration applied, this query against an empty table succeeds.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cs_conversation")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
