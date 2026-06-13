use crate::error::AppError;
use crate::repo::events::{self, EventRow};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RangeQuery {
    pub from: String,
    pub to: String,
}

/// Scheduled events with from <= start_at < to (both RFC3339 Z), ordered by start.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<EventRow>>, AppError> {
    let rows = events::list_between(&s.db, &q.from, &q.to)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct EventIn {
    pub title: String,
    pub start_at: String,
    pub location: Option<String>,
    pub notes: Option<String>,
}

fn validate(b: &EventIn) -> Result<(), AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    chrono::DateTime::parse_from_rfc3339(&b.start_at)
        .map_err(|_| AppError::BadRequest("start_at bukan RFC3339 valid".into()))?;
    Ok(())
}

pub async fn create(
    State(s): State<AppState>,
    Json(b): Json<EventIn>,
) -> Result<Json<EventRow>, AppError> {
    validate(&b)?;
    let row = events::create(&s.db, &b.title, b.location.as_deref(), b.notes.as_deref(), &b.start_at)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(row))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<EventIn>,
) -> Result<Json<EventRow>, AppError> {
    validate(&b)?;
    let ok = events::update(&s.db, id, &b.title, b.location.as_deref(), b.notes.as_deref(), &b.start_at)
        .await
        .map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    let row = events::get(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(row))
}

pub async fn cancel(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ok = events::cancel(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
