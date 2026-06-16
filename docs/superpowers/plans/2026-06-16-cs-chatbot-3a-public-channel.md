# CS Chatbot — Plan 3a: Public Channel (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose the CS brain to the public internet safely: a new `/public/cs/{session,message,history}` route group protected by an Origin allowlist (scoped CORS), a widget site-key, opaque per-conversation session tokens, a hand-rolled in-memory rate limiter, and input caps — with zero new dependencies.

**Architecture:** A new `cs` sub-router is built with its OWN `CorsLayer` (origin allowlist from `CS_ALLOWED_ORIGINS`) and merged into the app AFTER the existing groups, so the global `CorsLayer::permissive()` no longer wraps it. Pure, testable logic (origin/site-key checks, token generation, rate-limiter, config validation, session/history services) lives in `cs/` modules; the axum handlers in `api/cs_public.rs` are thin glue that construct `ClaudeClient::from_env()` + `CsEmbedder::from_env()` per request and call `cs::agent::handle_message`. Abuse controls are layered: Origin allowlist (browser) + site-key (routing) + session token (per-conversation) + IP/session rate limit + input length + per-conversation message cap.

**Tech Stack:** Rust, axum, tower-http (cors), sqlx, rand, std (OnceLock/Mutex). No new crates.

**Depends on:** Plans 1 + 2 (`repo::cs`, `cs::agent::handle_message`, `cs::kb::CsEmbedder`) — merged on `feat/cs-chatbot`.

> **Work in the worktree:** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`. Run all git/cargo there. **Do NOT `cargo fmt`.** Verify with `cargo test` + `cargo clippy`.

---

## File Structure

- Modify: `backend/src/error.rs` — add `AppError::RateLimited(String)` → 429.
- Create: `backend/src/cs/limiter.rs` — pure fixed-window rate limiter + process-global wrapper.
- Create: `backend/src/cs/gate.rs` — config validation, origin allowlist parse/check, site-key check, session-token generation.
- Create: `backend/src/cs/public.rs` — LLM-free services: `start_session`, `load_history` (testable without a model).
- Create: `backend/src/api/cs_public.rs` — the three axum handlers.
- Modify: `backend/src/cs/mod.rs` — declare `pub mod limiter; pub mod gate; pub mod public;`.
- Modify: `backend/src/api/mod.rs` — build the `cs` sub-router with scoped CORS; merge it.
- Modify: `backend/src/api/mod.rs` module list / `backend/src/main.rs` — call `cs::gate::validate_config()` at startup.
- Modify: `backend/.env.example` (and `.env.production.example` if present) — document new env vars.

---

## Task 1: `AppError::RateLimited` → 429

**Files:**
- Modify: `backend/src/error.rs`

- [ ] **Step 1: Write the failing test**

Add to `backend/src/error.rs` (create a `#[cfg(test)] mod tests` if none exists; check first and append to the existing one if present):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use axum::response::IntoResponse;

    #[test]
    fn rate_limited_maps_to_429() {
        let resp = AppError::RateLimited("slow down".into()).into_response();
        assert_eq!(resp.status(), axum::http::StatusCode::TOO_MANY_REQUESTS);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test error::tests::rate_limited_maps_to_429`
Expected: FAIL — no `RateLimited` variant.

- [ ] **Step 3: Implement**

In the `AppError` enum add:

```rust
    #[error("rate limit exceeded: {0}")]
    RateLimited(String),
```

In the `IntoResponse` match add (before the `Other` arm):

```rust
            AppError::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test error::tests::rate_limited_maps_to_429`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/error.rs
git commit -m "feat(cs): AppError::RateLimited -> 429"
```

---

## Task 2: Rate limiter (`cs/limiter.rs`)

**Files:**
- Create: `backend/src/cs/limiter.rs`
- Modify: `backend/src/cs/mod.rs` (add `pub mod limiter;`)

- [ ] **Step 1: Write the failing tests**

Create `backend/src/cs/limiter.rs`:

```rust
//! Tiny in-memory fixed-window rate limiter. No external deps — a process-global
//! map keyed by a caller-chosen string (IP and/or session). Suitable for a
//! single-tenant deployment; not distributed.

use std::collections::HashMap;

/// One caller's hit count within the current window.
#[derive(Clone, Copy)]
pub struct Window {
    pub window_start: u64, // unix seconds
    pub count: u32,
}

/// Pure core: returns true if the hit is ALLOWED, mutating `state` in place.
/// `now` is unix seconds, `window_secs` the bucket size, `max` the per-window cap.
pub fn check(
    state: &mut HashMap<String, Window>,
    key: &str,
    now: u64,
    window_secs: u64,
    max: u32,
) -> bool {
    let w = state.entry(key.to_string()).or_insert(Window { window_start: now, count: 0 });
    if now.saturating_sub(w.window_start) >= window_secs {
        w.window_start = now;
        w.count = 0;
    }
    if w.count >= max {
        return false;
    }
    w.count += 1;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max_then_blocks_within_window() {
        let mut s = HashMap::new();
        for _ in 0..3 {
            assert!(check(&mut s, "ip-1", 100, 60, 3));
        }
        // 4th in the same window is blocked
        assert!(!check(&mut s, "ip-1", 100, 60, 3));
    }

    #[test]
    fn window_resets_after_elapsed_time() {
        let mut s = HashMap::new();
        assert!(check(&mut s, "ip-1", 100, 60, 1));
        assert!(!check(&mut s, "ip-1", 130, 60, 1)); // still in window
        assert!(check(&mut s, "ip-1", 161, 60, 1));  // window elapsed -> reset
    }

    #[test]
    fn keys_are_independent() {
        let mut s = HashMap::new();
        assert!(check(&mut s, "a", 100, 60, 1));
        assert!(check(&mut s, "b", 100, 60, 1)); // different key unaffected
        assert!(!check(&mut s, "a", 100, 60, 1));
    }
}
```

- [ ] **Step 2: Run to verify pass**

Run: `cd backend && cargo test cs::limiter::tests`
Expected: PASS (pure functions; passing on first write is fine — the missing-module is the "red").

- [ ] **Step 3: Add the process-global wrapper**

Append to `backend/src/cs/limiter.rs`:

```rust
use std::sync::{Mutex, OnceLock};

fn global() -> &'static Mutex<HashMap<String, Window>> {
    static MAP: OnceLock<Mutex<HashMap<String, Window>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Current unix seconds. Isolated so tests use the pure `check` directly.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Process-global allow check. Returns true if the hit is allowed.
pub fn allow(key: &str, window_secs: u64, max: u32) -> bool {
    let mut map = match global().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    check(&mut map, key, now_secs(), window_secs, max)
}
```

- [ ] **Step 4: Run to verify pass + wire module**

Add `pub mod limiter;` to `backend/src/cs/mod.rs`.
Run: `cd backend && cargo test cs::limiter`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/cs/limiter.rs backend/src/cs/mod.rs
git commit -m "feat(cs): in-memory fixed-window rate limiter"
```

---

## Task 3: Gate — config, origin allowlist, site-key, session token (`cs/gate.rs`)

**Files:**
- Create: `backend/src/cs/gate.rs`
- Modify: `backend/src/cs/mod.rs` (add `pub mod gate;`)

- [ ] **Step 1: Write the failing tests**

Create `backend/src/cs/gate.rs`:

```rust
//! Public-channel gatekeeping: env config validation, Origin allowlist, widget
//! site-key check, and opaque session-token generation.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_config_requires_both_or_neither() {
        assert!(check_config(false, false).is_ok()); // disabled
        assert!(check_config(true, true).is_ok());    // enabled
        assert!(check_config(true, false).is_err());  // partial
        assert!(check_config(false, true).is_err());
    }

    #[test]
    fn parse_origins_splits_and_trims() {
        let o = parse_origins("https://a.com, https://b.com ,, https://c.com");
        assert_eq!(o, vec!["https://a.com", "https://b.com", "https://c.com"]);
    }

    #[test]
    fn origin_allowed_exact_match_only() {
        let allow = vec!["https://shop.com".to_string()];
        assert!(origin_allowed(&allow, Some("https://shop.com")));
        assert!(!origin_allowed(&allow, Some("https://evil.com")));
        assert!(!origin_allowed(&allow, None));
        // empty allowlist denies everything (fail closed)
        assert!(!origin_allowed(&[], Some("https://shop.com")));
    }

    #[test]
    fn site_key_constant_check() {
        assert!(site_key_ok(Some("secret"), Some("secret")));
        assert!(!site_key_ok(Some("secret"), Some("wrong")));
        assert!(!site_key_ok(Some("secret"), None));
        // when no key configured, reject (public endpoint must be explicitly enabled)
        assert!(!site_key_ok(None, Some("anything")));
    }

    #[test]
    fn session_tokens_are_unique_and_long() {
        let a = new_session_token();
        let b = new_session_token();
        assert_ne!(a, b);
        assert!(a.len() >= 32);
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test cs::gate::tests::parse_origins_splits_and_trims`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement**

Add to `backend/src/cs/gate.rs` (above `mod tests`):

```rust
use rand::RngCore;

/// `Ok` when CS public config is coherent: both `CS_ALLOWED_ORIGINS` and
/// `CS_WIDGET_KEY` set (enabled), or both unset (disabled). Exactly one => error.
pub fn validate_config() -> Result<(), String> {
    check_config(
        std::env::var("CS_ALLOWED_ORIGINS").is_ok(),
        std::env::var("CS_WIDGET_KEY").is_ok(),
    )
}

pub fn check_config(origins_set: bool, key_set: bool) -> Result<(), String> {
    if origins_set != key_set {
        return Err("CS_ALLOWED_ORIGINS and CS_WIDGET_KEY must be set together (or both unset)".into());
    }
    Ok(())
}

/// True when the CS public channel is enabled (both env vars present).
pub fn is_enabled() -> bool {
    std::env::var("CS_ALLOWED_ORIGINS").is_ok() && std::env::var("CS_WIDGET_KEY").is_ok()
}

/// Split a comma-separated origins string, trimming and dropping empties.
pub fn parse_origins(raw: &str) -> Vec<String> {
    raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
}

/// The configured allowlist from env (empty if unset).
pub fn allowed_origins() -> Vec<String> {
    std::env::var("CS_ALLOWED_ORIGINS").map(|v| parse_origins(&v)).unwrap_or_default()
}

/// Exact-match origin check. Fails closed: empty allowlist or missing Origin => false.
pub fn origin_allowed(allow: &[String], origin: Option<&str>) -> bool {
    match origin {
        Some(o) => allow.iter().any(|a| a == o),
        None => false,
    }
}

/// Compare the presented site-key against the configured one. Rejects when no
/// key is configured (the endpoint must be explicitly enabled).
pub fn site_key_ok(configured: Option<&str>, presented: Option<&str>) -> bool {
    match (configured, presented) {
        (Some(c), Some(p)) => c == p,
        _ => false,
    }
}

/// 32 random bytes, hex-encoded — an opaque, unguessable session token.
pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 4: Run to verify pass + wire module**

Add `pub mod gate;` to `backend/src/cs/mod.rs`.
Run: `cd backend && cargo test cs::gate`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/cs/gate.rs backend/src/cs/mod.rs
git commit -m "feat(cs): public-channel gate (config/origin/site-key/token)"
```

---

## Task 4: LLM-free services (`cs/public.rs`)

**Files:**
- Create: `backend/src/cs/public.rs`
- Modify: `backend/src/cs/mod.rs` (add `pub mod public;`)

- [ ] **Step 1: Write the failing tests**

Create `backend/src/cs/public.rs`:

```rust
//! Public-channel services that do not need an LLM: starting a session (lead
//! capture) and loading a conversation transcript for the widget to restore.

use crate::db::Db;
use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn start_session_requires_name_and_a_contact() {
        let db = mem_db().await;
        // missing name
        assert!(start_session(&db, "", Some("a@x.com"), None).await.is_err());
        // missing both contacts
        assert!(start_session(&db, "Budi", None, None).await.is_err());
        // ok with email
        let s = start_session(&db, "Budi", Some("a@x.com"), None).await.unwrap();
        assert!(!s.session_token.is_empty());
        // ok with phone
        assert!(start_session(&db, "Ani", None, Some("0812")).await.is_ok());
    }

    #[tokio::test]
    async fn start_session_persists_conversation_resolvable_by_token() {
        let db = mem_db().await;
        let s = start_session(&db, "Budi", Some("a@x.com"), None).await.unwrap();
        let conv = crate::repo::cs::conversation_by_token(&db, &s.session_token).await.unwrap();
        assert!(conv.is_some());
        assert_eq!(conv.unwrap().visitor_name.as_deref(), Some("Budi"));
    }

    #[tokio::test]
    async fn load_history_returns_messages_for_token() {
        let db = mem_db().await;
        let s = start_session(&db, "Budi", Some("a@x.com"), None).await.unwrap();
        let conv = crate::repo::cs::conversation_by_token(&db, &s.session_token).await.unwrap().unwrap();
        crate::repo::cs::message_add(&db, conv.id, "user", "halo").await.unwrap();
        crate::repo::cs::message_add(&db, conv.id, "assistant", "halo juga").await.unwrap();

        let hist = load_history(&db, &s.session_token).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].role, "user");

        // unknown token -> error (not an empty list, so the widget knows the session is invalid)
        assert!(load_history(&db, "nope").await.is_err());
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test cs::public::tests::start_session_requires_name_and_a_contact`
Expected: FAIL — `start_session` not found.

- [ ] **Step 3: Implement**

Add to `backend/src/cs/public.rs` (above `mod tests`):

```rust
#[derive(Serialize)]
pub struct StartedSession {
    pub session_token: String,
}

#[derive(Serialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// Start a web CS conversation with lead capture. Requires a name AND at least
/// one contact (email or phone) — the pre-chat form guarantees this; we enforce
/// it server-side too.
pub async fn start_session(
    db: &Db,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
) -> anyhow::Result<StartedSession> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("name is required");
    }
    let has_contact = email.map(|e| !e.trim().is_empty()).unwrap_or(false)
        || phone.map(|p| !p.trim().is_empty()).unwrap_or(false);
    if !has_contact {
        anyhow::bail!("an email or phone is required");
    }
    let token = crate::cs::gate::new_session_token();
    crate::repo::cs::conversation_create(db, "web", Some(name), email, phone, &token).await?;
    Ok(StartedSession { session_token: token })
}

/// Load the transcript for a session token. Errors if the token is unknown.
pub async fn load_history(db: &Db, token: &str) -> anyhow::Result<Vec<HistoryMessage>> {
    let conv = crate::repo::cs::conversation_by_token(db, token)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown session"))?;
    let rows = crate::repo::cs::message_all(db, conv.id).await?;
    Ok(rows
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| HistoryMessage { role: m.role, content: m.content, created_at: m.created_at })
        .collect())
}
```

- [ ] **Step 4: Run to verify pass + wire module**

Add `pub mod public;` to `backend/src/cs/mod.rs`.
Run: `cd backend && cargo test cs::public`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/cs/public.rs backend/src/cs/mod.rs
git commit -m "feat(cs): public services (start_session lead capture + load_history)"
```

---

## Task 5: Public HTTP handlers (`api/cs_public.rs`)

**Files:**
- Create: `backend/src/api/cs_public.rs`
- Modify: `backend/src/api/mod.rs` (declare `mod cs_public;` near the other `mod` lines — check how sibling handler modules like `whatsapp`/`chat` are declared and match it)

> **Context:** Handlers are thin. They (1) enforce site-key + Origin + rate limit, (2) call the LLM-free services or `cs::agent::handle_message`. The message handler constructs `ClaudeClient::from_env()` + `CsEmbedder::from_env()` per request (mirroring `api/whatsapp.rs::inbound`). The client IP for rate-limit keying comes from the `x-forwarded-for` header (Caddy sets it in prod); fall back to `"unknown"`.

- [ ] **Step 1: Write the handler module**

Create `backend/src/api/cs_public.rs`:

```rust
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
        .map_err(AppError::Other)?
        .ok_or_else(|| AppError::Unauthorized("unknown session".into()))?;

    let count = crate::repo::cs::message_all(&s.db, conv.id).await.map_err(AppError::Other)?.len() as i64;
    if count >= MAX_MESSAGES_PER_CONVERSATION {
        return Err(AppError::BadRequest("conversation limit reached; please start a new chat or contact us directly".into()));
    }

    let model = ClaudeClient::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let embedder = CsEmbedder::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("cs unavailable: {e}")))?;
    let reply = crate::cs::agent::handle_message(&s.db, &embedder, &model, conv.id, msg)
        .await
        .map_err(AppError::Other)?;
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
```

> **Implementer notes:**
> - History is a POST (takes a JSON body with `site_key` + `session_token`) to keep the site-key out of URLs/logs and simplify CORS. Wire it as `post(cs_public::history)` in Task 6.
> - Confirm `crate::AppState` is the correct path (the research shows `AppState` defined in `main.rs`; import as the other handlers do — check `api/chat.rs`'s `use`). Confirm `ClaudeClient` path is `crate::llm::claude::ClaudeClient` (matches `api/whatsapp.rs`).

- [ ] **Step 2: Verify it compiles**

Run: `cd backend && cargo check 2>&1 | tail -8`
Expected: compiles (handlers not yet routed — `dead_code` warnings expected). Fix any path/import errors per the notes.

- [ ] **Step 3: Commit**

```bash
git add backend/src/api/cs_public.rs backend/src/api/mod.rs
git commit -m "feat(cs): public HTTP handlers (session/message/history)"
```

---

## Task 6: Wire the scoped-CORS sub-router + startup validation

**Files:**
- Modify: `backend/src/api/mod.rs` (router function + imports)
- Modify: `backend/src/main.rs` (startup validation)

- [ ] **Step 1: Build the `cs` sub-router with its own CORS and merge it**

In `backend/src/api/mod.rs`, add imports near the existing `tower_http` import:

```rust
use tower_http::cors::{AllowOrigin, CorsLayer};
use axum::http::{HeaderName, Method};
```
(Keep the existing `CorsLayer` import; merge the `use` lines — don't duplicate.)

Refactor the tail of `router(...)`. Replace:

```rust
    public
        .merge(gateway)
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(state)
```

with:

```rust
    // Public CS widget endpoints: their OWN strict CORS (origin allowlist), so the
    // global permissive layer below does NOT relax them.
    let cs = Router::new()
        .route("/public/cs/session", post(cs_public::session))
        .route("/public/cs/message", post(cs_public::message))
        .route("/public/cs/history", post(cs_public::history))
        .layer(cs_cors_layer());

    let core = public
        .merge(gateway)
        .merge(protected)
        .layer(CorsLayer::permissive());

    core.merge(cs).with_state(state)
```

Add this helper function in `backend/src/api/mod.rs` (outside `router`):

```rust
/// CORS for the public CS endpoints: only the configured origins, GET/POST,
/// content-type header. Empty/unset allowlist => no origins allowed (fail closed).
fn cs_cors_layer() -> CorsLayer {
    let origins: Vec<_> = crate::cs::gate::allowed_origins()
        .into_iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    CorsLayer::new()
        .allow_origin(AllowOrigin::list(origins))
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([HeaderName::from_static("content-type")])
}
```

> **Implementer note:** `AllowOrigin::list` takes `HeaderValue`s; `o.parse()` yields `Result<HeaderValue, _>`. Confirm the exact type and that `post` + `Router` are already imported in this file (they are — used by the other groups). The `cs_public` module must be declared (`mod cs_public;`).

- [ ] **Step 2: Validate config at startup**

In `backend/src/main.rs`, right after the existing `auth::validate_env_config()` block, add:

```rust
    if let Err(e) = cs::gate::validate_config() {
        anyhow::bail!("{e}");
    }
```
(Confirm `cs` is reachable from `main.rs` — `mod cs;` was added in Plan 2. Use the correct path, e.g. `crate::cs::gate::validate_config()` or `cs::gate::validate_config()` matching how `auth::` is referenced there.)

- [ ] **Step 3: Verify compile + full test run**

Run: `cd backend && cargo check 2>&1 | tail -5 && cargo test cs:: error:: 2>&1 | tail -4`
Expected: compiles; all `cs::` + `error::` tests pass.

- [ ] **Step 4: Smoke-test the router builds**

Add a test to `backend/src/api/cs_public.rs`'s (new) `#[cfg(test)] mod tests` proving the gate logic rejects bad input without a DB:

```rust
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
```

Run: `cd backend && cargo test api::cs_public::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/api/mod.rs backend/src/main.rs backend/src/api/cs_public.rs
git commit -m "feat(cs): scoped-CORS public router + startup config validation"
```

---

## Task 7: Document env vars + final verification

**Files:**
- Modify: `backend/.env.example` (and `.env.production.example` / root `*.env.example` if the repo keeps one — check with `ls *.env.example backend/*.env.example` from the repo root)

- [ ] **Step 1: Add the new env vars (commented, with guidance)**

Append to `backend/.env.example` (match the file's existing comment style):

```bash
# --- Customer-service chatbot (public widget) ---
# Both must be set together to ENABLE the public /public/cs/* endpoints; leave
# both unset to keep the widget disabled. The backend refuses to start if only
# one is set.
# Comma-separated list of exact origins allowed to embed the widget:
CS_ALLOWED_ORIGINS=https://your-site.com,https://www.your-site.com
# Public site-key the embed script presents (NOT a secret — ships in page JS):
CS_WIDGET_KEY=
# Embedding model for the KB (OpenAI-shape; reuses OPENAI_API_KEY + INGEST_BASE_URL):
CS_EMBED_MODEL=text-embedding-3-small
```

- [ ] **Step 2: Final verification**

Run: `cd backend && cargo test cs:: error:: api::cs_public 2>&1 | tail -5 && cargo clippy --all-targets 2>&1 | tail -12`
Expected: all tests PASS; only `dead_code` warnings (some consumed now; widget consumes the rest in Plan 3b). No compile errors.

- [ ] **Step 3: Commit**

```bash
git add backend/.env.example
git commit -m "docs(cs): document public-widget env vars"
```

---

## Self-Review

**Spec coverage (spec §5 public API, §8 auth & abuse protection):**
- `/public/cs/session|message|history` ✓ Tasks 4–6.
- Scoped CORS origin allowlist (the real browser gate), not the global permissive ✓ Task 6 `cs_cors_layer` + merge order.
- Site-key check ✓ Task 3/5 (`site_key_ok`, `gate_request`).
- Opaque per-conversation session token ✓ Task 3 (`new_session_token`) stored on `cs_conversation`.
- Rate limit (per-IP sessions, per-session messages) ✓ Task 2 + 5.
- Input length cap + per-conversation message cap ✓ Task 5.
- Lead capture (name + contact required) before chat ✓ Task 4 `start_session`.
- 429 surfaced ✓ Task 1.
- Fail-closed config (both env vars or neither) ✓ Task 3 + startup validation Task 6.
- No new dependencies ✓ (rand/std only).

**Placeholder scan:** No TBD/TODO. Implementer-verification notes target real existing paths (`AppState`, `ClaudeClient`, axum imports) — not undefined new types.

**Type consistency:** `gate::{site_key_ok, origin_allowed, allowed_origins, new_session_token, validate_config, check_config}` used identically across `gate.rs`, `cs_public.rs`, `api/mod.rs`, `public.rs`. `limiter::allow(key, window, max)` matches all call sites. `public::{StartedSession, HistoryMessage, start_session, load_history}` match handler return types. `handle_message(db, embedder, model, conv_id, msg)` matches Plan 2's signature.

---

## Downstream

- **Plan 3b — Widget bundle:** `frontend/vite.config.widget.ts` + `src/cs-widget/` (Shadow-DOM bubble + pre-chat form), builds `cs-widget.js`, Dockerfile/Caddy serve it, embed snippet calls `/api/public/cs/*` with the site-key.
- **Plan 4 — Admin UI:** manage KB (calls `kb::chunk_text` + `repo::cs::kb_replace_chunks` + `kb::embed_pending`), pricing, orders, and a CS inbox (`escalation_list_open`, `conversation_list_recent`, `message_all`).
- **Plan 2.5 — Upwork `get_project_status` tool.**
