//! Public, embeddable customer-service endpoints. Protected by: scoped CORS
//! (Origin allowlist, applied in api/mod.rs), a widget site-key, opaque session
//! tokens, and an in-memory rate limiter. No JWT — these are anonymous visitors.

use axum::{extract::State, http::HeaderMap, Json};
use serde::Deserialize;

use crate::cs::{gate, limiter, public};
use crate::error::AppError;
use crate::llm::claude::ClaudeClient;
use crate::cs::kb::CsEmbedder;
use crate::AppState;

const MAX_MESSAGE_CHARS: usize = 2000;
const MAX_MESSAGES_PER_CONVERSATION: i64 = 60;

// --- rate-limit knobs (per fixed window) ---
const SESSION_WINDOW_SECS: u64 = 60;
const SESSION_MAX: u32 = 5;        // new sessions per IP per minute
const MESSAGE_WINDOW_SECS: u64 = 60;
const MESSAGE_MAX: u32 = 20;       // messages per session per minute

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn origin(headers: &HeaderMap) -> Option<String> {
    headers.get("origin").and_then(|v| v.to_str().ok()).map(|s| s.to_string())
}

/// Shared front-door checks for every public CS request: site-key + Origin allowlist.
fn gate_request(headers: &HeaderMap, presented_key: Option<&str>) -> Result<(), AppError> {
    let configured = std::env::var("CS_WIDGET_KEY").ok();
    if !gate::site_key_ok(configured.as_deref(), presented_key) {
        return Err(AppError::Unauthorized("invalid widget key".into()));
    }
    let allow = gate::allowed_origins();
    if !gate::origin_allowed(&allow, origin(headers).as_deref()) {
        return Err(AppError::Unauthorized("origin not allowed".into()));
    }
    Ok(())
}

#[derive(Deserialize)]
pub struct SessionIn {
    pub site_key: String,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
}

pub async fn session(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<SessionIn>,
) -> Result<Json<public::StartedSession>, AppError> {
    gate_request(&headers, Some(&b.site_key))?;
    if !limiter::allow(&format!("sess:{}", client_ip(&headers)), SESSION_WINDOW_SECS, SESSION_MAX) {
        return Err(AppError::RateLimited("too many sessions, slow down".into()));
    }
    let started = public::start_session(&s.db, &b.name, b.email.as_deref(), b.phone.as_deref())
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(started))
}

#[derive(Deserialize)]
pub struct MessageIn {
    pub site_key: String,
    pub session_token: String,
    pub message: String,
}

#[derive(serde::Serialize)]
pub struct MessageOut {
    pub reply: String,
}

pub async fn message(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<MessageIn>,
) -> Result<Json<MessageOut>, AppError> {
    gate_request(&headers, Some(&b.site_key))?;
    let msg = b.message.trim();
    if msg.is_empty() {
        return Err(AppError::BadRequest("empty message".into()));
    }
    if msg.chars().count() > MAX_MESSAGE_CHARS {
        return Err(AppError::BadRequest("message too long".into()));
    }
    if !limiter::allow(&format!("msg:{}", b.session_token), MESSAGE_WINDOW_SECS, MESSAGE_MAX) {
        return Err(AppError::RateLimited("too many messages, slow down".into()));
    }

    let conv = crate::repo::cs::conversation_by_token(&s.db, &b.session_token)
        .await
        .map_err(|e| AppError::PublicInternal(format!("{e:#}")))?
        .ok_or_else(|| AppError::Unauthorized("unknown session".into()))?;

    let count = crate::repo::cs::message_count(&s.db, conv.id)
        .await
        .map_err(|e| AppError::PublicInternal(format!("{e:#}")))?;
    if count >= MAX_MESSAGES_PER_CONVERSATION {
        return Err(AppError::BadRequest("conversation limit reached; please start a new chat or contact us directly".into()));
    }

    let model = ClaudeClient::from_env()
        .map_err(|e| AppError::PublicInternal(format!("chat unavailable: {e:#}")))?;
    let embedder = CsEmbedder::from_env()
        .map_err(|e| AppError::PublicInternal(format!("cs unavailable: {e:#}")))?;
    let reply = crate::cs::agent::handle_message(&s.db, &embedder, &model, conv.id, msg)
        .await
        .map_err(|e| AppError::PublicInternal(format!("{e:#}")))?;
    Ok(Json(MessageOut { reply }))
}

#[derive(Deserialize)]
pub struct HistoryIn {
    pub site_key: String,
    pub session_token: String,
}

pub async fn history(
    State(s): State<AppState>,
    headers: HeaderMap,
    Json(b): Json<HistoryIn>,
) -> Result<Json<Vec<public::HistoryMessage>>, AppError> {
    gate_request(&headers, Some(&b.site_key))?;
    if !limiter::allow(&format!("hist:{}", b.session_token), MESSAGE_WINDOW_SECS, MESSAGE_MAX) {
        return Err(AppError::RateLimited("slow down".into()));
    }
    let hist = public::load_history(&s.db, &b.session_token)
        .await
        .map_err(|_| AppError::Unauthorized("unknown session".into()))?;
    Ok(Json(hist))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderMap;

    #[test]
    fn gate_rejects_missing_site_key_and_origin() {
        // No CS_WIDGET_KEY configured in test env => site_key_ok is false => reject.
        let headers = HeaderMap::new();
        let r = gate_request(&headers, Some("whatever"));
        assert!(r.is_err());
    }

    #[test]
    fn client_ip_prefers_first_forwarded_for() {
        let mut h = HeaderMap::new();
        h.insert("x-forwarded-for", "1.2.3.4, 5.6.7.8".parse().unwrap());
        assert_eq!(client_ip(&h), "1.2.3.4");
        assert_eq!(client_ip(&HeaderMap::new()), "unknown");
    }
}
