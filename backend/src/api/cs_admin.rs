//! JWT-protected admin endpoints for the customer-service chatbot: knowledge
//! base, pricing, orders, and the CS inbox. Thin glue over repo::cs + cs::kb.

use axum::{extract::{Path, State}, Json};
use serde::Deserialize;

use crate::cs::kb::{self, CsEmbedder};
use crate::error::AppError;
use crate::repo::cs as repo;
use crate::AppState;

// ----------------------------- Knowledge base -----------------------------

pub async fn list_docs(State(s): State<AppState>) -> Result<Json<Vec<repo::KbDocRow>>, AppError> {
    Ok(Json(repo::kb_doc_list(&s.db).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct DocIn {
    pub title: String,
    pub source: Option<String>,
    pub body: String,
}

/// Persist chunks, then best-effort embed. Embedding failure does not fail the
/// save (the doc is durable; chunks embed later via reindex).
async fn save_chunks_and_embed(db: &crate::db::Db, doc_id: i64, body: &str) {
    let chunks = kb::chunk_text(body);
    if let Err(e) = repo::kb_replace_chunks(db, doc_id, &chunks).await {
        tracing::error!("cs admin: replace_chunks failed for doc {doc_id}: {e}");
        return;
    }
    match CsEmbedder::from_env() {
        Ok(embedder) => {
            if let Err(e) = kb::embed_pending(db, &embedder).await {
                tracing::warn!("cs admin: embed_pending failed for doc {doc_id}: {e}");
            }
        }
        Err(e) => tracing::warn!("cs admin: embedder unavailable, doc {doc_id} saved unembedded: {e}"),
    }
}

pub async fn create_doc(
    State(s): State<AppState>,
    Json(b): Json<DocIn>,
) -> Result<Json<repo::KbDocRow>, AppError> {
    if b.title.trim().is_empty() || b.body.trim().is_empty() {
        return Err(AppError::BadRequest("title and body are required".into()));
    }
    let id = repo::kb_doc_insert(&s.db, b.title.trim(), b.source.as_deref(), &b.body)
        .await
        .map_err(AppError::Other)?;
    save_chunks_and_embed(&s.db, id, &b.body).await;
    let doc = repo::kb_doc_list(&s.db)
        .await
        .map_err(AppError::Other)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or(AppError::NotFound)?;
    Ok(Json(doc))
}

pub async fn update_doc(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<DocIn>,
) -> Result<Json<()>, AppError> {
    if b.title.trim().is_empty() || b.body.trim().is_empty() {
        return Err(AppError::BadRequest("title and body are required".into()));
    }
    repo::kb_doc_update(&s.db, id, b.title.trim(), b.source.as_deref(), &b.body)
        .await
        .map_err(AppError::Other)?;
    save_chunks_and_embed(&s.db, id, &b.body).await;
    Ok(Json(()))
}

pub async fn delete_doc(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    repo::kb_doc_delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

/// Re-embed any chunks lacking an embedding (e.g. saved while the embedder was down).
pub async fn reindex_kb(State(s): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let embedder = CsEmbedder::from_env()
        .map_err(|e| AppError::BadRequest(format!("embedder unavailable: {e}")))?;
    let n = kb::embed_pending(&s.db, &embedder).await.map_err(AppError::Other)?;
    Ok(Json(serde_json::json!({ "embedded": n })))
}

// ----------------------------- Pricing -----------------------------

pub async fn list_products(
    State(s): State<AppState>,
) -> Result<Json<Vec<repo::ProductRow>>, AppError> {
    Ok(Json(repo::product_list_all(&s.db).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct ProductIn {
    pub name: String,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub availability: Option<String>,
}

pub async fn create_product(
    State(s): State<AppState>,
    Json(b): Json<ProductIn>,
) -> Result<Json<serde_json::Value>, AppError> {
    if b.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let id = repo::product_insert(
        &s.db,
        b.name.trim(),
        b.description.as_deref(),
        b.price,
        b.currency.as_deref(),
        b.availability.as_deref(),
    )
    .await
    .map_err(AppError::Other)?;
    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn update_product(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ProductIn>,
) -> Result<Json<()>, AppError> {
    repo::product_update(
        &s.db,
        id,
        b.name.trim(),
        b.description.as_deref(),
        b.price,
        b.currency.as_deref(),
        b.availability.as_deref(),
    )
    .await
    .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

#[derive(Deserialize)]
pub struct ActiveIn { pub active: bool }

pub async fn set_product_active(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ActiveIn>,
) -> Result<Json<()>, AppError> {
    repo::product_set_active(&s.db, id, b.active)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

pub async fn delete_product(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    repo::product_delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

// ----------------------------- Orders -----------------------------

pub async fn list_orders(
    State(s): State<AppState>,
) -> Result<Json<Vec<repo::OrderRow>>, AppError> {
    Ok(Json(repo::order_list(&s.db, 500).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct OrderIn {
    pub external_ref: String,
    pub customer_name: Option<String>,
    pub customer_contact: Option<String>,
    pub status: String,
    pub details_json: Option<String>,
}

/// Upsert by external_ref (owner-populated).
pub async fn upsert_order(
    State(s): State<AppState>,
    Json(b): Json<OrderIn>,
) -> Result<Json<()>, AppError> {
    if b.external_ref.trim().is_empty() || b.status.trim().is_empty() {
        return Err(AppError::BadRequest("external_ref and status are required".into()));
    }
    repo::order_upsert(
        &s.db,
        b.external_ref.trim(),
        b.customer_name.as_deref(),
        b.customer_contact.as_deref(),
        b.status.trim(),
        b.details_json.as_deref(),
    )
    .await
    .map_err(AppError::Other)?;
    Ok(Json(()))
}

pub async fn delete_order(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    repo::order_delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

// ----------------------------- Inbox / escalations -----------------------------

pub async fn list_conversations(
    State(s): State<AppState>,
) -> Result<Json<Vec<repo::ConversationRow>>, AppError> {
    Ok(Json(repo::conversation_list_recent(&s.db, 200).await.map_err(AppError::Other)?))
}

pub async fn conversation_messages(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Vec<repo::MessageRow>>, AppError> {
    Ok(Json(repo::message_all(&s.db, id).await.map_err(AppError::Other)?))
}

pub async fn resolve_conversation(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    repo::conversation_set_status(&s.db, id, "resolved")
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

pub async fn list_escalations(
    State(s): State<AppState>,
) -> Result<Json<Vec<repo::EscalationRow>>, AppError> {
    Ok(Json(repo::escalation_list_open(&s.db).await.map_err(AppError::Other)?))
}

pub async fn handle_escalation(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    repo::escalation_mark_handled(&s.db, id)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

// ----------------------------- Owner reply -----------------------------

#[derive(Deserialize)]
pub struct ReplyIn {
    pub text: String,
}

/// Owner reply to a CS conversation from the inbox. Stored as an assistant
/// message; for WhatsApp conversations with a `wa_jid`, the message is also
/// enqueued for delivery to the customer over the CS number.
pub async fn reply_conversation(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ReplyIn>,
) -> Result<Json<()>, AppError> {
    let text = b.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("empty reply".into()));
    }

    // Use conversation_get for O(1) lookup rather than scanning the list.
    let conv = repo::conversation_get(&s.db, id)
        .await
        .map_err(AppError::Other)?
        .ok_or(AppError::NotFound)?;

    repo::message_add(&s.db, id, "assistant", text).await.map_err(AppError::Other)?;
    repo::conversation_touch(&s.db, id).await.map_err(AppError::Other)?;

    if conv.channel == "whatsapp" {
        if let Some(jid) = conv.wa_jid.as_deref() {
            crate::cs::wa_outbound::push(&s.cs_outbound, jid, text);
        }
    }
    Ok(Json(()))
}
