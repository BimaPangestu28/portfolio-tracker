use crate::error::AppError;
use crate::repo::connectors::{self, NewConnector};
use crate::AppState;
use axum::{extract::{Path, State}, Json};

pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<connectors::ConnectorRow>>, AppError> {
    Ok(Json(connectors::list(&s.db).await.map_err(AppError::Other)?))
}

pub async fn create(
    State(s): State<AppState>,
    Json(b): Json<NewConnector>,
) -> Result<Json<connectors::ConnectorRow>, AppError> {
    Ok(Json(connectors::create(&s.db, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    connectors::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

pub async fn sync(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<crate::service::sync::SyncReport>, AppError> {
    let row = crate::repo::connectors::get(&s.db, id).await?;
    let conn = crate::connectors::factory::build(&row)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let report = crate::service::sync::run_sync(&s.db, &row, conn.as_ref())
        .await
        .map_err(AppError::Other)?;
    Ok(Json(report))
}
