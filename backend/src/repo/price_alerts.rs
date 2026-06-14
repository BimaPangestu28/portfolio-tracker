//! Per-instrument price alerts (migration 0020).
use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PriceAlertRow {
    pub id: i64,
    pub instrument_id: i64,
    pub target_price: String,
    pub direction: String,
    pub status: String,
    pub created_at: String,
    pub triggered_at: Option<String>,
}

pub async fn create(
    db: &Db,
    instrument_id: i64,
    target_price: &str,
    direction: &str,
) -> anyhow::Result<PriceAlertRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO price_alerts (instrument_id, target_price, direction, status, created_at) VALUES (?, ?, ?, 'active', ?)",
    )
    .bind(instrument_id)
    .bind(target_price)
    .bind(direction)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<PriceAlertRow> {
    Ok(
        sqlx::query_as::<_, PriceAlertRow>("SELECT * FROM price_alerts WHERE id = ?")
            .bind(id)
            .fetch_one(db)
            .await?,
    )
}

pub async fn list_active(db: &Db) -> anyhow::Result<Vec<PriceAlertRow>> {
    Ok(sqlx::query_as::<_, PriceAlertRow>(
        "SELECT * FROM price_alerts WHERE status = 'active' ORDER BY id",
    )
    .fetch_all(db)
    .await?)
}

pub async fn mark_triggered(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE price_alerts SET status = 'triggered', triggered_at = ? WHERE id = ? AND status = 'active'",
    )
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let rows_affected = sqlx::query(
        "UPDATE price_alerts SET status = 'cancelled' WHERE id = ? AND status = 'active'",
    )
    .bind(id)
    .execute(db)
    .await?
    .rows_affected();
    Ok(rows_affected > 0)
}

/// Returns true when `price` has reached the target in the alert's direction.
pub fn is_triggered(
    direction: &str,
    target: rust_decimal::Decimal,
    price: rust_decimal::Decimal,
) -> bool {
    match direction {
        "below" => price <= target,
        "above" => price >= target,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    /// Insert a minimal instrument row and return its id.
    ///
    /// SQLite enforces FK constraints (PRAGMA foreign_keys = ON is set in
    /// `db::connect`), so price_alerts.instrument_id must reference a real
    /// instruments row.
    async fn insert_instrument(db: &Db) -> i64 {
        crate::repo::instruments::create(
            db,
            &crate::repo::instruments::NewInstrument {
                symbol: "TEST".into(),
                name: "Test Instrument".into(),
                instrument_type: "stock".into(),
                native_currency: "IDR".into(),
                category_id: None,
                price_source: "manual".into(),
                decimals: Some(2),
                note: None,
            },
        )
        .await
        .unwrap()
        .id
    }

    #[test]
    fn trigger_predicate() {
        assert!(is_triggered("below", dec!(9000), dec!(8999)));
        assert!(!is_triggered("below", dec!(9000), dec!(9001)));
        assert!(is_triggered("above", dec!(11000), dec!(11000)));
        assert!(!is_triggered("above", dec!(11000), dec!(10999)));
        assert!(!is_triggered("sideways", dec!(1), dec!(1)));
    }

    #[tokio::test]
    async fn create_list_trigger_cancel() {
        let db = mem_db().await;
        // FKs are enforced — insert a real instrument first.
        let iid = insert_instrument(&db).await;

        let alert_a = create(&db, iid, "9000", "below").await.unwrap();
        assert_eq!(list_active(&db).await.unwrap().len(), 1);

        mark_triggered(&db, alert_a.id).await.unwrap();
        assert!(list_active(&db).await.unwrap().is_empty());

        let alert_b = create(&db, iid, "11000", "above").await.unwrap();
        assert!(cancel(&db, alert_b.id).await.unwrap());
        assert!(list_active(&db).await.unwrap().is_empty());
        // already cancelled → rows_affected = 0 → false
        assert!(!cancel(&db, alert_b.id).await.unwrap());
    }
}
