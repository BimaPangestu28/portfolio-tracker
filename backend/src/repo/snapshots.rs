use crate::db::Db;

pub async fn upsert(db: &Db, as_of: &str, total_idr: &str, total_usd: &str, breakdown_json: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO valuation_snapshot (as_of, total_idr, total_usd, breakdown_json) VALUES (?,?,?,?)
         ON CONFLICT(as_of) DO UPDATE SET total_idr=excluded.total_idr, total_usd=excluded.total_usd, breakdown_json=excluded.breakdown_json")
        .bind(as_of).bind(total_idr).bind(total_usd).bind(breakdown_json)
        .execute(db).await?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct SnapshotRow { pub as_of: String, pub total_idr: String, pub total_usd: String, pub breakdown_json: String }

pub async fn history(db: &Db) -> anyhow::Result<Vec<SnapshotRow>> {
    Ok(sqlx::query_as::<_, SnapshotRow>("SELECT * FROM valuation_snapshot ORDER BY as_of").fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn snapshot_upsert_and_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert(&db, "2026-05-31", "1000", "0.06", "{}").await.unwrap();
        upsert(&db, "2026-05-31", "1100", "0.07", "{}").await.unwrap();
        assert_eq!(history(&db).await.unwrap().len(), 1);
        assert_eq!(history(&db).await.unwrap()[0].total_idr, "1100");
    }
}
