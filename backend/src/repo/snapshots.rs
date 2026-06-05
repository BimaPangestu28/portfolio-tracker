use crate::db::Db;

pub async fn upsert(db: &Db, as_of: &str, total_idr: &str, total_usd: &str, breakdown_json: &str, price_pnl_idr: Option<&str>, fx_pnl_idr: Option<&str>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO valuation_snapshot (as_of, total_idr, total_usd, breakdown_json, price_pnl_idr, fx_pnl_idr) VALUES (?,?,?,?,?,?)
         ON CONFLICT(as_of) DO UPDATE SET total_idr=excluded.total_idr, total_usd=excluded.total_usd, breakdown_json=excluded.breakdown_json, price_pnl_idr=excluded.price_pnl_idr, fx_pnl_idr=excluded.fx_pnl_idr")
        .bind(as_of).bind(total_idr).bind(total_usd).bind(breakdown_json).bind(price_pnl_idr).bind(fx_pnl_idr)
        .execute(db).await?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct SnapshotRow { pub as_of: String, pub total_idr: String, pub total_usd: String, pub breakdown_json: String, pub price_pnl_idr: Option<String>, pub fx_pnl_idr: Option<String> }

pub async fn history(db: &Db) -> anyhow::Result<Vec<SnapshotRow>> {
    Ok(sqlx::query_as::<_, SnapshotRow>("SELECT * FROM valuation_snapshot ORDER BY as_of").fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn snapshot_upsert_and_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert(&db, "2026-05-31", "1000", "0.06", "{}", None, None).await.unwrap();
        upsert(&db, "2026-05-31", "1100", "0.07", "{}", Some("900"), Some("200")).await.unwrap();
        let rows = history(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_idr, "1100");
        assert_eq!(rows[0].price_pnl_idr.as_deref(), Some("900"));
        assert_eq!(rows[0].fx_pnl_idr.as_deref(), Some("200"));
    }

    #[tokio::test]
    async fn snapshot_decomposition_is_nullable() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert(&db, "2026-05-31", "1000", "0.06", "{}", None, None).await.unwrap();
        let rows = history(&db).await.unwrap();
        assert_eq!(rows[0].price_pnl_idr, None);
        assert_eq!(rows[0].fx_pnl_idr, None);
    }
}
