use crate::error::AppError;
use crate::llm::claude::ClaudeClient;
use crate::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct WaIn { pub from: String, pub message: String }
#[derive(Serialize)]
pub struct WaOut { pub reply: String }

/// Called by the Baileys gateway for each inbound WhatsApp text. Optional shared-secret check
/// via header `x-gateway-token` == env GATEWAY_TOKEN (when set). Returns the assistant reply;
/// the gateway is responsible for sending it back over WhatsApp.
pub async fn inbound(State(s): State<AppState>, headers: HeaderMap, Json(b): Json<WaIn>) -> Result<Json<WaOut>, AppError> {
    if let Ok(expected) = std::env::var("GATEWAY_TOKEN") {
        let got = headers.get("x-gateway-token").and_then(|v| v.to_str().ok()).unwrap_or("");
        if got != expected { return Err(AppError::BadRequest("bad gateway token".into())); }
    }
    if b.message.trim().is_empty() { return Err(AppError::BadRequest("empty message".into())); }
    let client = ClaudeClient::from_env().map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let reply = crate::service::chat::answer(&s.db, &client, "whatsapp", &b.message).await.map_err(AppError::Other)?;
    let _ = &b.from; // reserved for future per-sender routing
    Ok(Json(WaOut { reply }))
}
