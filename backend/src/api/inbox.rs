use crate::error::AppError;
use crate::repo::inbox::{self, InboxRow};
use crate::AppState;
use axum::{extract::State, Json};

/// Pending inbox items (status = pending).
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<InboxRow>>, AppError> {
    let rows = inbox::list_pending(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
