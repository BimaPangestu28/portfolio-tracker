use crate::error::AppError;
use crate::ingestion::ingest::{ingest_batch, UploadFile};
use crate::ingestion::review::{confirm, reject, ConfirmPayload};
use crate::llm::claude::ClaudeClient;
use crate::repo::review_items;
use crate::AppState;
use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UploadFileIn { pub filename: String, pub media_type: String, pub data_base64: String }

#[derive(Deserialize)]
pub struct IngestIn { pub files: Vec<UploadFileIn> }

#[derive(serde::Serialize)]
pub struct IngestOut { pub batch_id: String, pub items: Vec<review_items::ReviewItemRow> }

/// Deterministic-ish batch id without Date::now() in non-test code: use a uuid-like from process time.
fn new_batch_id() -> String {
    // chrono::Utc::now is available in production paths (only Date::now()/rand are restricted in *workflow scripts*, not app code).
    format!("batch-{}", chrono::Utc::now().timestamp_millis())
}

pub async fn ingest(State(s): State<AppState>, Json(b): Json<IngestIn>) -> Result<Json<IngestOut>, AppError> {
    if b.files.is_empty() { return Err(AppError::BadRequest("no files".into())); }
    let client = ClaudeClient::from_env().map_err(|e| AppError::Other(anyhow::anyhow!("llm config: {e}")))?;
    let files: Vec<UploadFile> = b.files.into_iter()
        .map(|f| UploadFile { filename: f.filename, media_type: f.media_type, data_base64: f.data_base64 })
        .collect();
    let batch_id = new_batch_id();
    let res = ingest_batch(&s.db, &client, &batch_id, &files).await
        .map_err(|e| AppError::Other(anyhow::anyhow!("ingest failed: {e}")))?;
    Ok(Json(IngestOut { batch_id: res.batch_id, items: res.items }))
}

#[derive(Deserialize)]
pub struct ReviewQuery { #[serde(default = "default_status")] pub status: String }
fn default_status() -> String { "pending".into() }

pub async fn list_review(State(s): State<AppState>, Query(q): Query<ReviewQuery>) -> Result<Json<Vec<review_items::ReviewItemRow>>, AppError> {
    Ok(Json(review_items::list_by_status(&s.db, &q.status).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct PatchIn { pub payload_json: serde_json::Value }
pub async fn patch_review(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<PatchIn>) -> Result<Json<review_items::ReviewItemRow>, AppError> {
    let payload = serde_json::to_string(&b.payload_json).map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(review_items::update_payload(&s.db, id, &payload).await.map_err(AppError::Other)?))
}

#[derive(serde::Serialize)]
pub struct ConfirmOut { pub created_txn_id: i64 }
pub async fn confirm_review(State(s): State<AppState>, Path(id): Path<i64>, Json(p): Json<ConfirmPayload>) -> Result<Json<ConfirmOut>, AppError> {
    let txn_id = confirm(&s.db, id, &p).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(ConfirmOut { created_txn_id: txn_id }))
}

pub async fn reject_review(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    reject(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
