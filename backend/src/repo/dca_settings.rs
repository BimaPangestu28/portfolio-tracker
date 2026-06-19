use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DcaSettingRow {
    pub id: i64,
    pub monthly_budget: String,
    pub frequency: String,
    pub anchor_day: i64,
    pub rounding_step: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveDcaSetting {
    pub monthly_budget: String,
    pub frequency: String,
    pub anchor_day: i64,
    pub rounding_step: String,
}

pub async fn get(db: &Db) -> anyhow::Result<DcaSettingRow> {
    if let Some(row) = sqlx::query_as::<_, DcaSettingRow>("SELECT * FROM dca_setting WHERE id = 1")
        .fetch_optional(db)
        .await?
    {
        return Ok(row);
    }
    Ok(DcaSettingRow {
        id: 1,
        monthly_budget: "0".to_string(),
        frequency: "monthly".to_string(),
        anchor_day: 1,
        rounding_step: "10000".to_string(),
        updated_at: String::new(),
    })
}

pub async fn upsert(db: &Db, s: &SaveDcaSetting) -> anyhow::Result<DcaSettingRow> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO dca_setting (id, monthly_budget, frequency, anchor_day, rounding_step, updated_at) \
         VALUES (1, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
           monthly_budget = excluded.monthly_budget, \
           frequency = excluded.frequency, \
           anchor_day = excluded.anchor_day, \
           rounding_step = excluded.rounding_step, \
           updated_at = excluded.updated_at",
    )
    .bind(&s.monthly_budget)
    .bind(&s.frequency)
    .bind(s.anchor_day)
    .bind(&s.rounding_step)
    .bind(&now)
    .execute(db)
    .await?;
    get(db).await
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn get_returns_defaults_when_empty() {
        let db = mem_db().await;
        let row = get(&db).await.unwrap();
        assert_eq!(row.monthly_budget, "0");
        assert_eq!(row.frequency, "monthly");
        assert_eq!(row.anchor_day, 1);
        assert_eq!(row.rounding_step, "10000");
    }

    #[tokio::test]
    async fn upsert_then_get_roundtrips_and_is_singleton() {
        let db = mem_db().await;
        upsert(&db, &SaveDcaSetting {
            monthly_budget: "55000000".into(),
            frequency: "weekly".into(),
            anchor_day: 12,
            rounding_step: "10000".into(),
        }).await.unwrap();
        // second upsert must update the same row, not insert a new one
        let row = upsert(&db, &SaveDcaSetting {
            monthly_budget: "60000000".into(),
            frequency: "monthly".into(),
            anchor_day: 1,
            rounding_step: "100000".into(),
        }).await.unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.monthly_budget, "60000000");
        let again = get(&db).await.unwrap();
        assert_eq!(again.frequency, "monthly");
        assert_eq!(again.rounding_step, "100000");
    }
}
