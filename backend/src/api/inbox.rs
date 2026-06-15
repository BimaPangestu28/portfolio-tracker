use crate::error::AppError;
use crate::repo::inbox::{self, InboxRow};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// Inbox by status (?status=pending|sorted|all); defaults to pending.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<InboxRow>>, AppError> {
    let status = q.status.as_deref().unwrap_or("pending");
    let rows = inbox::list_by_status(&s.db, status).await.map_err(AppError::Other)?;
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

/// Move a sorted inbox item back to pending.
pub async fn unresolve(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = inbox::unresolve(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
