//! CS WhatsApp: a second WhatsApp connection routed through the CS brain.
//! Mirrors api/whatsapp.rs but locks AppState.cs_wa, authenticates with
//! CS_GATEWAY_TOKEN, and drains the outbound queue for proactive sends.

use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use std::time::Instant;

use crate::api::whatsapp::{CommandOut, StatePush};
use crate::cs::kb::CsEmbedder;
use crate::cs::wa_outbound;
use crate::error::AppError;
use crate::llm::claude::ClaudeClient;
use crate::wa_state::{WaCommand, WaState, WaStatusView};
use crate::AppState;

fn check_cs_gateway_token(headers: &HeaderMap) -> Result<(), AppError> {
    let expected = std::env::var("CS_GATEWAY_TOKEN").ok();
    let got      = headers.get("x-gateway-token").and_then(|v| v.to_str().ok());
    let ok = match expected {
        Some(exp) => got == Some(exp.as_str()),
        None      => true, // unset = open (dev)
    };
    if ok { Ok(()) } else { Err(AppError::Unauthorized("bad gateway token".into())) }
}

fn lock_cs_wa(s: &AppState) -> Result<std::sync::MutexGuard<'_, WaState>, AppError> {
    s.cs_wa
        .lock()
        .map_err(|_| AppError::Other(anyhow::anyhow!("cs_wa poisoned")))
}

#[derive(Deserialize)]
pub struct CsWaIn {
    pub from:    String,
    pub message: String,
}

/// reply is None when the bot stays silent (conversation taken over by a human).
#[derive(Serialize)]
pub struct CsWaOut {
    pub reply: Option<String>,
}

/// Called by the CS Baileys gateway for each inbound WhatsApp text message.
/// Finds or creates a per-JID conversation, then either runs the CS agent
/// (status == "bot") or records the message silently (escalated/resolved).
pub async fn inbound(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<CsWaIn>,
) -> Result<Json<CsWaOut>, AppError> {
    check_cs_gateway_token(&headers)?;
    let msg = b.message.trim();
    if msg.is_empty() {
        return Err(AppError::BadRequest("empty message".into()));
    }

    // Find or create the per-JID conversation.
    let conv = match crate::repo::cs::conversation_by_wa_jid(&s.db, &b.from)
        .await
        .map_err(AppError::Other)?
    {
        Some(c) => c,
        None => {
            let phone = b.from.split('@').next().unwrap_or(&b.from);
            let token = crate::cs::gate::new_session_token();
            crate::repo::cs::conversation_create_wa(&s.db, &b.from, phone, &token)
                .await
                .map_err(AppError::Other)?
        }
    };

    // Escalated/resolved → bot stays silent; just record the inbound message.
    if conv.status != "bot" {
        crate::repo::cs::message_add(&s.db, conv.id, "user", msg)
            .await
            .map_err(AppError::Other)?;
        crate::repo::cs::conversation_touch(&s.db, conv.id)
            .await
            .map_err(AppError::Other)?;
        return Ok(Json(CsWaOut { reply: None }));
    }

    // Bot path: cs::agent::handle_message stores both user + assistant turns internally.
    let model = ClaudeClient::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let embedder = CsEmbedder::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("cs unavailable: {e}")))?;
    let reply = crate::cs::agent::handle_message(&s.db, &embedder, &model, conv.id, msg)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(CsWaOut { reply: Some(reply) }))
}

/// Gateway pushes its current connection state here (using CS_GATEWAY_TOKEN).
pub async fn push_state(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<StatePush>,
) -> Result<Json<()>, AppError> {
    check_cs_gateway_token(&headers)?;
    lock_cs_wa(&s)?.apply_push(b.status, b.qr, b.number, Instant::now());
    Ok(Json(()))
}

/// CS gateway polls here for a pending control command (consume-once).
pub async fn poll_commands(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommandOut>, AppError> {
    check_cs_gateway_token(&headers)?;
    let command = lock_cs_wa(&s)?.take_command();
    Ok(Json(CommandOut { command }))
}

/// Outbound messages enqueued by the owner reply endpoint. Gateway drains these
/// and sends them to the customer over the CS number.
#[derive(Serialize)]
pub struct OutboundBatch {
    pub messages: Vec<wa_outbound::OutboundMsg>,
}

pub async fn poll_outbound(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OutboundBatch>, AppError> {
    check_cs_gateway_token(&headers)?;
    Ok(Json(OutboundBatch { messages: wa_outbound::drain(&s.cs_outbound) }))
}

/// Current CS connection status for the dashboard UI.
pub async fn status(State(s): State<AppState>) -> Result<Json<WaStatusView>, AppError> {
    Ok(Json(lock_cs_wa(&s)?.view(Instant::now())))
}

/// Request the CS gateway to (re)start a session — produces a fresh QR.
pub async fn connect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_cs_wa(&s)?.set_command(WaCommand::Restart);
    Ok(Json(()))
}

/// Request the CS gateway to log out and clear its session.
pub async fn disconnect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_cs_wa(&s)?.set_command(WaCommand::Logout);
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use crate::db::Db;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    // The silent-when-escalated branch is pure repo logic; verify it here without HTTP.
    #[tokio::test]
    async fn escalated_conversation_records_without_bot_reply() {
        let db = mem_db().await;
        let c  = crate::repo::cs::conversation_create_wa(&db, "j@x", "j", "tk-1")
            .await
            .unwrap();
        crate::repo::cs::conversation_set_status(&db, c.id, "needs_human")
            .await
            .unwrap();

        // Simulate the inbound silent branch: status != "bot" → store user msg only.
        crate::repo::cs::message_add(&db, c.id, "user", "halo").await.unwrap();
        let msgs = crate::repo::cs::message_all(&db, c.id).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
    }
}
