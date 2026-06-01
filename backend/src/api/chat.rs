use crate::error::AppError;
use crate::llm::claude::ClaudeClient;
use crate::repo::chat;
use crate::AppState;
use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ChatIn { pub message: String }
#[derive(Serialize)]
pub struct ChatOut { pub reply: String }

pub async fn post_chat(State(s): State<AppState>, Json(b): Json<ChatIn>) -> Result<Json<ChatOut>, AppError> {
    if b.message.trim().is_empty() { return Err(AppError::BadRequest("empty message".into())); }
    let client = ClaudeClient::from_env().map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let reply = crate::service::chat::answer(&s.db, &client, "inapp", &b.message).await.map_err(AppError::Other)?;
    Ok(Json(ChatOut { reply }))
}

pub async fn history(State(s): State<AppState>) -> Result<Json<Vec<chat::ChatMessageRow>>, AppError> {
    Ok(Json(chat::history(&s.db, 100).await.map_err(AppError::Other)?))
}
