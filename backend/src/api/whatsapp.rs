use crate::error::AppError;
use crate::llm::claude::ClaudeClient;
use crate::wa_state::{WaCommand, WaStatus, WaStatusView};
use crate::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use serde::{Deserialize, Serialize};
use std::time::Instant;

#[derive(Deserialize)]
pub struct WaIn {
    pub from: String,
    pub message: String,
}
#[derive(Serialize)]
pub struct WaOut {
    pub reply: String,
}

/// Pure token comparison. `None` expected means no token is configured (local
/// dev) and any request is allowed.
fn token_matches(expected: Option<String>, got: Option<&str>) -> bool {
    match expected {
        Some(exp) => got == Some(exp.as_str()),
        None => true,
    }
}

/// Enforce the shared-secret header `x-gateway-token` == env `GATEWAY_TOKEN`.
pub fn check_gateway_token(headers: &HeaderMap) -> Result<(), AppError> {
    let expected = std::env::var("GATEWAY_TOKEN").ok();
    let got = headers
        .get("x-gateway-token")
        .and_then(|v| v.to_str().ok());
    if token_matches(expected, got) {
        Ok(())
    } else {
        Err(AppError::BadRequest("bad gateway token".into()))
    }
}

fn lock_wa(s: &AppState) -> Result<std::sync::MutexGuard<'_, crate::wa_state::WaState>, AppError> {
    s.wa
        .lock()
        .map_err(|_| AppError::Other(anyhow::anyhow!("wa state poisoned")))
}

/// Called by the Baileys gateway for each inbound WhatsApp text. Returns the
/// assistant reply; the gateway sends it back over WhatsApp.
pub async fn inbound(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<WaIn>,
) -> Result<Json<WaOut>, AppError> {
    check_gateway_token(&headers)?;
    if b.message.trim().is_empty() {
        return Err(AppError::BadRequest("empty message".into()));
    }
    let client = ClaudeClient::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let reply = crate::service::chat::answer(&s.db, &client, "whatsapp", &b.message)
        .await
        .map_err(AppError::Other)?;
    let _ = &b.from; // reserved for future per-sender routing
    Ok(Json(WaOut { reply }))
}

// ── Gateway-facing endpoints ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StatePush {
    pub status: WaStatus,
    pub qr: Option<String>,
    pub number: Option<String>,
}

#[derive(Serialize)]
pub struct CommandOut {
    pub command: Option<WaCommand>,
}

/// Gateway pushes its current connection state here.
pub async fn push_state(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<StatePush>,
) -> Result<Json<()>, AppError> {
    check_gateway_token(&headers)?;
    lock_wa(&s)?.apply_push(b.status, b.qr, b.number, Instant::now());
    Ok(Json(()))
}

/// Gateway polls here for a pending control command (consume-once).
pub async fn poll_commands(
    State(s): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<CommandOut>, AppError> {
    check_gateway_token(&headers)?;
    let command = lock_wa(&s)?.take_command();
    Ok(Json(CommandOut { command }))
}

// ── Frontend-facing endpoints ───────────────────────────────────────────────
//
// These deliberately do NOT check the gateway token: like every other web API in
// this single-user, self-hosted app they are gated only by the network boundary
// (and the client-side unlock), not by the shared gateway secret.

/// Current connection status for the web UI.
pub async fn status(State(s): State<AppState>) -> Result<Json<WaStatusView>, AppError> {
    let view = lock_wa(&s)?.view(Instant::now());
    Ok(Json(view))
}

/// Request the gateway to (re)start a session — produces a fresh QR.
pub async fn connect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_wa(&s)?.set_command(WaCommand::Restart);
    Ok(Json(()))
}

/// Request the gateway to log out and clear its session.
pub async fn disconnect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_wa(&s)?.set_command(WaCommand::Logout);
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_matches_allows_when_unset() {
        assert!(token_matches(None, None));
        assert!(token_matches(None, Some("anything")));
    }

    #[test]
    fn token_matches_requires_equality_when_set() {
        assert!(token_matches(Some("secret".into()), Some("secret")));
        assert!(!token_matches(Some("secret".into()), Some("wrong")));
        assert!(!token_matches(Some("secret".into()), None));
    }
}
