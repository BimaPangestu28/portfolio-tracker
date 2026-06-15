use crate::error::AppError;
use crate::repo::todos::{self, TodoRow};
use crate::AppState;
use axum::{extract::State, Json};

/// Open todos (status = open).
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<TodoRow>>, AppError> {
    let rows = todos::list_open(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
