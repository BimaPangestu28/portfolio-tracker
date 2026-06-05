use crate::error::AppError;
use crate::ingestion::csv::{parse_csv_rows, ColumnMapping};
use crate::ingestion::ingest::{ingest_batch, needs_attention, UploadFile};
use crate::ingestion::matching::{suggest_account, suggest_instrument_for_entry};
use crate::ingestion::review::{confirm, reject, ConfirmPayload};
use crate::llm::claude::ClaudeClient;
use crate::repo::review_items::{self, NewReviewItem};
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

#[derive(serde::Deserialize)]
pub struct CsvIngestIn {
    pub filename: String,
    pub csv_text: String,
    pub mapping: std::collections::HashMap<String, String>,
    pub entry_type_const: Option<String>,
}

pub async fn ingest_csv(State(s): State<AppState>, Json(b): Json<CsvIngestIn>) -> Result<Json<IngestOut>, AppError> {
    let mapping: ColumnMapping = b.mapping.into_iter().collect();
    let entries = parse_csv_rows(&b.csv_text, &mapping, b.entry_type_const.as_deref())
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let batch_id = format!("csv-{}", chrono::Utc::now().timestamp_millis());
    let mut items = Vec::new();
    for entry in &entries {
        let payload = serde_json::to_string(entry).map_err(|e| AppError::Other(e.into()))?;
        let sug_ins = suggest_instrument_for_entry(&s.db, entry.symbol.as_deref(), entry.instrument_name.as_deref()).await.map_err(AppError::Other)?;
        let sug_acc = match &entry.account_hint { Some(x) => suggest_account(&s.db, x).await.map_err(AppError::Other)?, None => None };
        let row = review_items::create(&s.db, &NewReviewItem {
            batch_id: &batch_id, source_kind: "csv", source_filename: &b.filename, source_path: "(csv)",
            doc_type: "csv_import", needs_attention: needs_attention(entry),
            payload_json: &payload, raw_llm_json: "{}", suggested_instrument_id: sug_ins, suggested_account_id: sug_acc,
        }).await.map_err(AppError::Other)?;
        items.push(row);
    }
    Ok(Json(IngestOut { batch_id, items }))
}
