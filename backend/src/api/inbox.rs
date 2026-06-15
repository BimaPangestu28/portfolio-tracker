use crate::error::AppError;
use crate::repo::inbox::{self, InboxRow};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};

/// Pending inbox items (status = pending).
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<InboxRow>>, AppError> {
    let rows = inbox::list_pending(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

/// Mark a pending inbox item as handled (status = sorted).
pub async fn resolve(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let affected = inbox::resolve(&s.db, &[id], "sorted").await.map_err(AppError::Other)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
