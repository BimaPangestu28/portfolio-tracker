use crate::error::AppError;
use crate::repo::reminders::{self, ReminderRow};
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

/// Reminders by status (?status=pending|sent|cancelled|all); defaults to pending.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ReminderRow>>, AppError> {
    let status = q.status.as_deref().unwrap_or("pending");
    let rows = reminders::list_by_status(&s.db, status).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct ReminderIn {
    pub message: String,
    pub remind_at: String,
    pub recurrence: Option<String>,
}

/// Create a standalone reminder.
pub async fn create(State(s): State<AppState>, Json(b): Json<ReminderIn>) -> Result<Json<ReminderRow>, AppError> {
    if b.message.trim().is_empty() {
        return Err(AppError::BadRequest("pesan tidak boleh kosong".into()));
    }
    chrono::DateTime::parse_from_rfc3339(&b.remind_at)
        .map_err(|_| AppError::BadRequest("remind_at bukan RFC3339 valid".into()))?;
    let recurrence = b.recurrence.as_deref().unwrap_or("none");
    let row = reminders::create(&s.db, None, b.message.trim(), &b.remind_at, recurrence)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(row))
}

/// Cancel a pending reminder.
pub async fn cancel(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = reminders::cancel(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
