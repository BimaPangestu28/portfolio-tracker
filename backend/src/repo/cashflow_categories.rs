use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CashflowCategoryRow {
    pub id: i64,
    pub name: String,
    pub kind: String,
    pub monthly_budget: Option<String>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewCashflowCategory {
    pub name: String,
    pub kind: String,
    pub monthly_budget: Option<String>,
    pub color: Option<String>,
}

pub async fn create(db: &Db, c: &NewCashflowCategory) -> anyhow::Result<CashflowCategoryRow> {
    if c.kind != "income" && c.kind != "expense" {
        anyhow::bail!("kind must be 'income' or 'expense', got '{}'", c.kind);
    }
    let id = sqlx::query(
        "INSERT INTO cashflow_category (name, kind, monthly_budget, color) VALUES (?,?,?,?)")
        .bind(&c.name).bind(&c.kind).bind(&c.monthly_budget).bind(&c.color)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<CashflowCategoryRow> {
    Ok(sqlx::query_as::<_, CashflowCategoryRow>("SELECT * FROM cashflow_category WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<CashflowCategoryRow>> {
    Ok(sqlx::query_as::<_, CashflowCategoryRow>("SELECT * FROM cashflow_category ORDER BY id")
        .fetch_all(db).await?)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM cashflow_category WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn create_and_list() {
        let db = mem_db().await;
        let c = NewCashflowCategory {
            name: "Food".into(),
            kind: "expense".into(),
            monthly_budget: Some("100.00".into()),
            color: None,
        };
        let created = create(&db, &c).await.unwrap();
        assert_eq!(created.name, "Food");
        assert_eq!(created.kind, "expense");
        assert_eq!(created.monthly_budget.as_deref(), Some("100.00"));
        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }

    #[tokio::test]
    async fn rejects_invalid_kind() {
        let db = mem_db().await;
        let c = NewCashflowCategory {
            name: "Bad".into(),
            kind: "invalid".into(),
            monthly_budget: None,
            color: None,
        };
        assert!(create(&db, &c).await.is_err());
    }
}
