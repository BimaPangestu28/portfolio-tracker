use crate::error::AppError;
use crate::repo::reminders::{self, ReminderRow};
use crate::AppState;
use axum::{extract::State, Json};

/// Pending reminders (not yet sent), ordered by remind_at.
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<ReminderRow>>, AppError> {
    let rows = reminders::list_pending(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
