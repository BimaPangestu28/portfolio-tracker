use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CashflowRow {
    pub id: i64,
    pub account_id: Option<i64>,
    pub occurred_on: String,
    pub direction: String,
    pub amount: String,
    pub currency: String,
    pub category_id: Option<i64>,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewCashflow {
    pub account_id: Option<i64>,
    pub occurred_on: String,
    pub direction: String,
    pub amount: String,
    pub currency: String,
    pub category_id: Option<i64>,
    pub note: Option<String>,
}

pub async fn create(db: &Db, c: &NewCashflow) -> anyhow::Result<CashflowRow> {
    if c.direction != "in" && c.direction != "out" {
        anyhow::bail!("direction must be 'in' or 'out', got '{}'", c.direction);
    }
    crate::repo::dec(&c.amount)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cashflow (account_id, occurred_on, direction, amount, currency, category_id, note, created_at) VALUES (?,?,?,?,?,?,?,?)")
        .bind(c.account_id).bind(&c.occurred_on).bind(&c.direction)
        .bind(&c.amount).bind(&c.currency).bind(c.category_id)
        .bind(&c.note).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<CashflowRow> {
    Ok(sqlx::query_as::<_, CashflowRow>("SELECT * FROM cashflow WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

pub async fn list_all(db: &Db) -> anyhow::Result<Vec<CashflowRow>> {
    Ok(sqlx::query_as::<_, CashflowRow>("SELECT * FROM cashflow ORDER BY occurred_on DESC, id DESC")
        .fetch_all(db).await?)
}

pub async fn list_for_month(db: &Db, year_month: &str) -> anyhow::Result<Vec<CashflowRow>> {
    Ok(sqlx::query_as::<_, CashflowRow>(
        "SELECT * FROM cashflow WHERE occurred_on LIKE ?||'%' ORDER BY occurred_on DESC, id DESC")
        .bind(year_month).fetch_all(db).await?)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM cashflow WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    fn make(occurred_on: &str) -> NewCashflow {
        NewCashflow {
            account_id: None,
            occurred_on: occurred_on.into(),
            direction: "out".into(),
            amount: "50.00".into(),
            currency: "USD".into(),
            category_id: None,
            note: None,
        }
    }

    #[tokio::test]
    async fn list_for_month_returns_correct_rows() {
        let db = mem_db().await;
        create(&db, &make("2026-06-01")).await.unwrap();
        create(&db, &make("2026-06-15")).await.unwrap();
        create(&db, &make("2026-05-20")).await.unwrap();
        let june = list_for_month(&db, "2026-06").await.unwrap();
        assert_eq!(june.len(), 2);
        let may = list_for_month(&db, "2026-05").await.unwrap();
        assert_eq!(may.len(), 1);
    }

    #[tokio::test]
    async fn rejects_invalid_direction() {
        let db = mem_db().await;
        let c = NewCashflow {
            account_id: None, occurred_on: "2026-06-01".into(), direction: "sideways".into(),
            amount: "10.00".into(), currency: "USD".into(), category_id: None, note: None,
        };
        assert!(create(&db, &c).await.is_err());
    }

    #[tokio::test]
    async fn rejects_bad_decimal() {
        let db = mem_db().await;
        let c = NewCashflow {
            account_id: None, occurred_on: "2026-06-01".into(), direction: "in".into(),
            amount: "notanumber".into(), currency: "USD".into(), category_id: None, note: None,
        };
        assert!(create(&db, &c).await.is_err());
    }
}
