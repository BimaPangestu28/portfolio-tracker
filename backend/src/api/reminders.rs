use crate::error::AppError;
use crate::repo::reminders::{self, ReminderRow};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};

/// Pending reminders (not yet sent), ordered by remind_at.
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<ReminderRow>>, AppError> {
    let rows = reminders::list_pending(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

/// Cancel a pending reminder.
pub async fn cancel(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = reminders::cancel(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
