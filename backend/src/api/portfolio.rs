use crate::error::AppError;
use crate::service::portfolio::{build_summary, PortfolioSummary};
use crate::repo::snapshots;
use crate::AppState;
use axum::{extract::State, Json};

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
