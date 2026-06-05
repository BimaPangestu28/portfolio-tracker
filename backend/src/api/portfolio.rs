use crate::error::AppError;
use crate::repo::snapshots;
use crate::service::insights::{build_insights, Insights};
use crate::service::performance::{build_performance, PerformanceView};
use crate::service::portfolio::{build_summary, PortfolioSummary};
use crate::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

pub async fn summary(State(s): State<AppState>) -> Result<Json<PortfolioSummary>, AppError> {
    Ok(Json(build_summary(&s.db).await.map_err(AppError::Other)?))
}
pub async fn history(State(s): State<AppState>) -> Result<Json<Vec<snapshots::SnapshotRow>>, AppError> {
    Ok(Json(snapshots::history(&s.db).await.map_err(AppError::Other)?))
}
pub async fn refresh(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    crate::pricing::service::refresh_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
pub async fn insights(State(s): State<AppState>) -> Result<Json<Insights>, AppError> {
    Ok(Json(build_insights(&s.db).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct PerfQuery {
    pub base: Option<String>,
    pub period: Option<String>,
}

pub async fn performance(
    State(s): State<AppState>,
    Query(q): Query<PerfQuery>,
) -> Result<Json<PerformanceView>, AppError> {
    let base = q.base.as_deref().unwrap_or("idr");
    if base != "idr" && base != "usd" {
        return Err(AppError::BadRequest("base must be idr or usd".into()));
    }
    let period = q.period.as_deref().unwrap_or("1y");
    if !["1m", "3m", "6m", "ytd", "1y", "all"].contains(&period) {
        return Err(AppError::BadRequest("invalid period".into()));
    }
    Ok(Json(
        build_performance(&s.db, base, period)
            .await
            .map_err(AppError::Other)?,
    ))
}
