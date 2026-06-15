use crate::error::AppError;
use crate::repo::todos::{self, TodoRow};
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

/// Todos filtered by status (?status=open|done|all); defaults to open.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<TodoRow>>, AppError> {
    let status = q.status.as_deref().unwrap_or("open");
    let rows = todos::list_by_status(&s.db, status).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct TodoIn {
    pub title: String,
}

/// Quick-add a todo (title only; other fields default).
pub async fn create(State(s): State<AppState>, Json(b): Json<TodoIn>) -> Result<Json<TodoRow>, AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    let row = todos::create(&s.db, b.title.trim(), None, None, None, None)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(row))
}

/// Mark an open todo done.
pub async fn complete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = todos::complete(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}

#[derive(Deserialize)]
pub struct TodoUpdateIn {
    pub title: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub priority: Option<String>,
    pub estimate_minutes: Option<i64>,
}

/// Edit a todo (full replace of editable fields).
pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<TodoUpdateIn>,
) -> Result<Json<TodoRow>, AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    let ok = todos::update(
        &s.db, id, b.title.trim(), b.notes.as_deref(), b.due_at.as_deref(),
        b.priority.as_deref(), b.estimate_minutes,
    )
    .await
    .map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    let row = todos::get(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(row))
}

/// Reopen a done todo.
pub async fn reopen(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = todos::reopen(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
