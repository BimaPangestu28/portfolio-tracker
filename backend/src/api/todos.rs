use crate::error::AppError;
use crate::repo::todos::{self, TodoRow};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

/// Open todos (status = open).
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<TodoRow>>, AppError> {
    let rows = todos::list_open(&s.db).await.map_err(AppError::Other)?;
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
