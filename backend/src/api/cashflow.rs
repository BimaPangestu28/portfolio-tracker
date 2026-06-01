use crate::error::AppError;
use crate::repo::{cashflow, cashflow_categories, dec};
use crate::service::cashflow::{month_summary, CatRow, CfRow, MonthSummary};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

// ── Cashflow CRUD ──────────────────────────────────────────────────────────────

pub async fn list_cashflow(
    State(s): State<AppState>,
) -> Result<Json<Vec<cashflow::CashflowRow>>, AppError> {
    Ok(Json(cashflow::list_all(&s.db).await.map_err(AppError::Other)?))
}

pub async fn create_cashflow(
    State(s): State<AppState>,
    Json(b): Json<cashflow::NewCashflow>,
) -> Result<Json<cashflow::CashflowRow>, AppError> {
    Ok(Json(
        cashflow::create(&s.db, &b)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?,
    ))
}

pub async fn delete_cashflow(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    cashflow::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

// ── Cashflow Categories CRUD ───────────────────────────────────────────────────

pub async fn list_categories(
    State(s): State<AppState>,
) -> Result<Json<Vec<cashflow_categories::CashflowCategoryRow>>, AppError> {
    Ok(Json(cashflow_categories::list(&s.db).await.map_err(AppError::Other)?))
}

pub async fn create_category(
    State(s): State<AppState>,
    Json(b): Json<cashflow_categories::NewCashflowCategory>,
) -> Result<Json<cashflow_categories::CashflowCategoryRow>, AppError> {
    Ok(Json(
        cashflow_categories::create(&s.db, &b)
            .await
            .map_err(|e| AppError::BadRequest(e.to_string()))?,
    ))
}

pub async fn delete_category(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    cashflow_categories::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

// ── Monthly Summary ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SummaryQuery {
    pub month: Option<String>,
}

pub async fn summary(
    State(s): State<AppState>,
    Query(q): Query<SummaryQuery>,
) -> Result<Json<MonthSummary>, AppError> {
    let month = q
        .month
        .unwrap_or_else(|| chrono::Utc::now().format("%Y-%m").to_string());

    let cf_rows = cashflow::list_for_month(&s.db, &month)
        .await
        .map_err(AppError::Other)?;

    let cat_rows = cashflow_categories::list(&s.db)
        .await
        .map_err(AppError::Other)?;

    // Convert repo rows → service types
    let cf: Vec<CfRow> = cf_rows
        .into_iter()
        .map(|r| -> anyhow::Result<CfRow> {
            Ok(CfRow {
                direction: r.direction,
                amount: dec(&r.amount)?,
                category_id: r.category_id,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(AppError::Other)?;

    let cats: Vec<CatRow> = cat_rows
        .into_iter()
        .map(|r| -> anyhow::Result<CatRow> {
            let budget = match r.monthly_budget.as_deref() {
                Some(b) => Some(dec(b)?),
                None => None,
            };
            Ok(CatRow {
                id: r.id,
                name: r.name,
                kind: r.kind,
                budget,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()
        .map_err(AppError::Other)?;

    Ok(Json(month_summary(&month, &cf, &cats)))
}
