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
    pub source: Option<String>,
    pub external_ref: Option<String>,
    pub created_at: String,
}

/// User-supplied fields for a new cashflow entry. Provenance (`source`,
/// `external_ref`) is set by the connector via `insert_sourced`, not the user.
#[derive(Debug, Deserialize, PartialEq)]
pub struct NewCashflow {
    pub account_id: Option<i64>,
    pub occurred_on: String,
    pub direction: String,
    pub amount: String,
    pub currency: String,
    pub category_id: Option<i64>,
    pub note: Option<String>,
}

/// Validate the user-supplied fields of a cashflow entry (direction + amount).
fn validate_new_cashflow(c: &NewCashflow) -> anyhow::Result<()> {
    if c.direction != "in" && c.direction != "out" {
        anyhow::bail!("direction must be 'in' or 'out', got '{}'", c.direction);
    }
    crate::repo::dec(&c.amount)?;
    Ok(())
}

pub async fn create(db: &Db, c: &NewCashflow) -> anyhow::Result<CashflowRow> {
    validate_new_cashflow(c)?;
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cashflow (account_id, occurred_on, direction, amount, currency, category_id, note, created_at) VALUES (?,?,?,?,?,?,?,?)")
        .bind(c.account_id).bind(&c.occurred_on).bind(&c.direction)
        .bind(&c.amount).bind(&c.currency).bind(c.category_id)
        .bind(&c.note).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

/// Insert a cashflow row tagged with provenance, deduplicated by
/// (source, external_ref). Returns true if a new row was inserted, false if it
/// already existed (idempotent re-sync). Mirrors `create`'s validation.
pub async fn insert_sourced(
    db: &Db,
    c: &NewCashflow,
    source: &str,
    external_ref: &str,
) -> anyhow::Result<bool> {
    validate_new_cashflow(c)?;
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query(
        "INSERT INTO cashflow
            (account_id, occurred_on, direction, amount, currency, category_id, note, source, external_ref, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(source, external_ref) DO NOTHING",
    )
    .bind(c.account_id).bind(&c.occurred_on).bind(&c.direction)
    .bind(&c.amount).bind(&c.currency).bind(c.category_id)
    .bind(&c.note).bind(source).bind(external_ref).bind(&now)
    .execute(db).await?;
    Ok(res.rows_affected() > 0)
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

    #[tokio::test]
    async fn insert_sourced_is_idempotent_on_external_ref() {
        let db = mem_db().await;
        let c = NewCashflow {
            account_id: None, occurred_on: "2026-06-10".into(), direction: "in".into(),
            amount: "500.00".into(), currency: "USD".into(), category_id: None,
            note: Some("Acme contract".into()),
        };
        let first = insert_sourced(&db, &c, "upwork", "txn-1").await.unwrap();
        let second = insert_sourced(&db, &c, "upwork", "txn-1").await.unwrap();
        assert!(first, "first insert should create a row");
        assert!(!second, "duplicate external_ref must be a no-op");
        let rows = list_all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        let row = &rows[0];
        assert_eq!(row.source.as_deref(), Some("upwork"));
        assert_eq!(row.external_ref.as_deref(), Some("txn-1"));
        assert_eq!(row.direction, "in");
    }

    #[tokio::test]
    async fn insert_sourced_rejects_invalid_direction() {
        let db = mem_db().await;
        let c = NewCashflow {
            account_id: None, occurred_on: "2026-06-01".into(), direction: "sideways".into(),
            amount: "10.00".into(), currency: "USD".into(), category_id: None, note: None,
        };
        assert!(insert_sourced(&db, &c, "upwork", "x1").await.is_err());
    }

    #[tokio::test]
    async fn insert_sourced_rejects_bad_decimal() {
        let db = mem_db().await;
        let c = NewCashflow {
            account_id: None, occurred_on: "2026-06-01".into(), direction: "in".into(),
            amount: "notanumber".into(), currency: "USD".into(), category_id: None, note: None,
        };
        assert!(insert_sourced(&db, &c, "upwork", "x1").await.is_err());
    }
}
