use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CategoryRow {
    pub id: i64,
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub sort_order: i64,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewCategory {
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub sort_order: Option<i64>,
    pub color: Option<String>,
}

pub async fn create(db: &Db, c: &NewCategory) -> anyhow::Result<CategoryRow> {
    let id = sqlx::query(
        "INSERT INTO category (name, target_pct, tolerance_band_pct, sort_order, color) VALUES (?,?,?,?,?)")
        .bind(&c.name).bind(&c.target_pct).bind(&c.tolerance_band_pct)
        .bind(c.sort_order.unwrap_or(0)).bind(&c.color)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<CategoryRow> {
    Ok(sqlx::query_as::<_, CategoryRow>("SELECT * FROM category WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<CategoryRow>> {
    Ok(sqlx::query_as::<_, CategoryRow>("SELECT * FROM category ORDER BY sort_order, id").fetch_all(db).await?)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM category WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}
