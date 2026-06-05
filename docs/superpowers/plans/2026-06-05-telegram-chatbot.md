# Telegram Chatbot Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Telegram as a second chat channel with parity to WhatsApp — text Q&A about the portfolio, restricted to the owner via a one-time link code generated in the web UI.

**Architecture:** The Rust backend long-polls the Telegram Bot API (`getUpdates`) in a Tokio background task spawned at startup when `TELEGRAM_BOT_TOKEN` is set. No new service. Linking: a 6-digit code (in-memory, 10-min TTL) is generated via a JWT-protected endpoint; the owner sends it to the bot; the matched `chat_id` is persisted in a new single-row `telegram_link` table. Replies reuse `service::chat::answer()` with `channel = "telegram"`.

**Tech Stack:** Rust (Axum 0.7, Tokio, sqlx/SQLite, reqwest), React + TanStack Query + zod + MSW/vitest frontend.

**Spec:** `docs/superpowers/specs/2026-06-05-telegram-chatbot-design.md`

**Working directory:** repo root is `/Users/bimapangestu/Desktop/Works/personal/portfolio-tracker`. Backend commands run in `backend/`, frontend commands in `frontend/`. Branch: `feat/telegram-chatbot`.

---

## File Structure

```
backend/
  migrations/0008_telegram_link.sql      (create)  single-row link table
  src/repo/telegram_link.rs              (create)  get/set/clear link row
  src/repo/mod.rs                        (modify)  register module
  src/repo/chat.rs                       (modify)  allow channel "telegram"
  src/telegram/mod.rs                    (create)  spawn() + poll loop + plan_action
  src/telegram/state.rs                  (create)  TgState: link code TTL + auth-failed flag
  src/telegram/client.rs                 (create)  TelegramClient: getUpdates/sendMessage + DTOs
  src/api/telegram.rs                    (create)  status / link-code / unlink handlers
  src/api/mod.rs                         (modify)  routes + test AppState
  src/error.rs                           (modify)  add Conflict variant (409)
  src/main.rs                            (modify)  mod decls, AppState.tg, spawn poller
  Cargo.toml                             (modify)  add rand
frontend/
  src/api/schemas.ts                     (modify)  TelegramStatusSchema, TelegramLinkCodeSchema
  src/api/hooks.ts                       (modify)  useTelegramStatus/LinkCode/Unlink
  src/pages/TelegramPage.tsx             (create)  status page + linking flow
  src/pages/TelegramPage.test.tsx        (create)  page tests
  src/test/server.ts                     (modify)  default MSW handler
  src/App.tsx                            (modify)  /telegram route
  src/components/AppShell.tsx            (modify)  nav item
docker-compose.yml                       (modify)  TELEGRAM_BOT_TOKEN env
docker-compose.prod.yml                  (modify)  TELEGRAM_BOT_TOKEN env
.env.production.example                  (modify)  document the new var
k8s/10-backend.yaml                      (modify)  secret-backed env (optional)
k8s/secret.example.yaml                  (modify)  document the new key
```

---

### Task 1: `telegram_link` migration + repo

**Files:**
- Create: `backend/migrations/0008_telegram_link.sql`
- Create: `backend/src/repo/telegram_link.rs`
- Modify: `backend/src/repo/mod.rs:5` (module list)

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0008_telegram_link.sql`:

```sql
-- Single-row table: which Telegram chat is linked as the owner.
-- id is CHECKed to 1 so the app can only ever have one link (single-user app).
CREATE TABLE telegram_link (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  chat_id INTEGER NOT NULL,
  username TEXT,
  linked_at TEXT NOT NULL
);
```

- [ ] **Step 2: Write the failing repo tests**

Create `backend/src/repo/telegram_link.rs`:

```rust
//! Persistence for the single Telegram owner link (see migration 0008).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TelegramLinkRow {
    pub chat_id: i64,
    pub username: Option<String>,
    pub linked_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn get_returns_none_before_linking() {
        let db = mem_db().await;
        assert!(get(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn set_then_get_returns_the_link() {
        let db = mem_db().await;
        set(&db, 12345, Some("bima")).await.unwrap();
        let row = get(&db).await.unwrap().expect("link row");
        assert_eq!(row.chat_id, 12345);
        assert_eq!(row.username.as_deref(), Some("bima"));
        assert!(!row.linked_at.is_empty());
    }

    #[tokio::test]
    async fn set_replaces_an_existing_link() {
        let db = mem_db().await;
        set(&db, 111, Some("old")).await.unwrap();
        set(&db, 222, None).await.unwrap();
        let row = get(&db).await.unwrap().expect("link row");
        assert_eq!(row.chat_id, 222);
        assert_eq!(row.username, None);
    }

    #[tokio::test]
    async fn clear_removes_the_link() {
        let db = mem_db().await;
        set(&db, 111, None).await.unwrap();
        clear(&db).await.unwrap();
        assert!(get(&db).await.unwrap().is_none());
    }
}
```

Register the module in `backend/src/repo/mod.rs` — add after `pub mod snapshots;`:

```rust
pub mod telegram_link;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend && cargo test repo::telegram_link`
Expected: COMPILE ERROR — `get`, `set`, `clear` not found.

- [ ] **Step 4: Implement the repo functions**

Add above the `#[cfg(test)]` block in `backend/src/repo/telegram_link.rs`:

```rust
/// The current owner link, or None when no Telegram chat is linked.
pub async fn get(db: &Db) -> anyhow::Result<Option<TelegramLinkRow>> {
    let row = sqlx::query_as::<_, TelegramLinkRow>(
        "SELECT chat_id, username, linked_at FROM telegram_link WHERE id = 1",
    )
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Link (or re-link) the owner chat. Replaces any existing link.
pub async fn set(db: &Db, chat_id: i64, username: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO telegram_link (id, chat_id, username, linked_at) VALUES (1, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET chat_id = excluded.chat_id,
                                       username = excluded.username,
                                       linked_at = excluded.linked_at",
    )
    .bind(chat_id)
    .bind(username)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

/// Remove the owner link (unlink).
pub async fn clear(db: &Db) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM telegram_link WHERE id = 1")
        .execute(db)
        .await?;
    Ok(())
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test repo::telegram_link`
Expected: 4 passed.

- [ ] **Step 6: Commit**

```bash
git add backend/migrations/0008_telegram_link.sql backend/src/repo/telegram_link.rs backend/src/repo/mod.rs
git commit -m "feat(backend): add telegram_link table and repo"
```

---

### Task 2: Allow the `telegram` channel in chat persistence

**Files:**
- Modify: `backend/src/repo/chat.rs:17-19` (channel validation) and `:83-87` (test)

- [ ] **Step 1: Update the tests first**

In `backend/src/repo/chat.rs`, the existing test `invalid_channel_returns_error` uses `"telegram"` as the invalid example — it must become valid. Replace the test (lines 82-87) with:

```rust
    #[tokio::test]
    async fn telegram_channel_is_accepted() {
        let db = mem_db().await;
        let msg = add(&db, "user", "berapa net worth saya?", "telegram").await.unwrap();
        assert_eq!(msg.channel, "telegram");
    }

    #[tokio::test]
    async fn invalid_channel_returns_error() {
        let db = mem_db().await;
        let result = add(&db, "user", "hello", "email").await;
        assert!(result.is_err());
    }
```

- [ ] **Step 2: Run tests to verify the new one fails**

Run: `cd backend && cargo test repo::chat`
Expected: `telegram_channel_is_accepted` FAILS (invalid channel error); `invalid_channel_returns_error` passes.

- [ ] **Step 3: Widen the validation**

In `backend/src/repo/chat.rs:17-19`, replace:

```rust
    if !matches!(channel, "inapp" | "whatsapp") {
        anyhow::bail!("invalid channel '{}': must be 'inapp' or 'whatsapp'", channel);
    }
```

with:

```rust
    if !matches!(channel, "inapp" | "whatsapp" | "telegram") {
        anyhow::bail!("invalid channel '{}': must be 'inapp', 'whatsapp', or 'telegram'", channel);
    }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test repo::chat`
Expected: all pass (4 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/chat.rs
git commit -m "feat(backend): accept telegram as a chat channel"
```

---

### Task 3: Link-code state (`telegram/state.rs`)

In-memory state mirroring the `wa_state.rs` pattern: a 6-digit one-time code with a 10-minute TTL, plus an `auth_failed` flag the poller sets on a 401 so the UI can report a bad token.

**Files:**
- Create: `backend/src/telegram/state.rs`
- Create: `backend/src/telegram/mod.rs` (module shell for now)
- Modify: `backend/src/main.rs:1-13` (add `mod telegram;`)
- Modify: `backend/Cargo.toml` (add `rand`)

- [ ] **Step 1: Add the rand dependency**

In `backend/Cargo.toml` `[dependencies]`, add after `constant_time_eq = "0.3"`:

```toml
rand = "0.8"
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/telegram/state.rs`:

```rust
//! In-memory Telegram linking state.
//!
//! Holds the active one-time link code (10-minute TTL, consumed on first
//! successful verification) and whether the bot token was rejected by
//! Telegram. Ephemeral by design — a backend restart simply requires
//! generating a fresh code.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A link code older than this is rejected.
const CODE_TTL: Duration = Duration::from_secs(600);

/// Surface CODE_TTL to the API layer (expires_in in seconds).
pub const CODE_TTL_SECS: u64 = CODE_TTL.as_secs();

#[derive(Debug, Default)]
pub struct TgState {
    /// Active link code and when it was generated.
    code: Option<(String, Instant)>,
    /// Set by the poller when Telegram rejects the bot token (401).
    auth_failed: bool,
}

pub type SharedTgState = Arc<Mutex<TgState>>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_is_six_digits() {
        let mut state = TgState::default();
        let code = state.generate_code(Instant::now());
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()), "non-digit in {code}");
    }

    #[test]
    fn fresh_code_verifies_once_then_is_consumed() {
        let mut state = TgState::default();
        let now = Instant::now();
        let code = state.generate_code(now);
        assert!(state.verify_code(&code, now));
        // Single-use: the same code must not verify twice.
        assert!(!state.verify_code(&code, now));
    }

    #[test]
    fn wrong_code_is_rejected_and_does_not_consume() {
        let mut state = TgState::default();
        let now = Instant::now();
        let code = state.generate_code(now);
        assert!(!state.verify_code("000000", now));
        // The real code still works after a failed attempt.
        assert!(state.verify_code(&code, now));
    }

    #[test]
    fn expired_code_is_rejected() {
        let mut state = TgState::default();
        let created = Instant::now();
        let code = state.generate_code(created);
        let later = created + CODE_TTL + Duration::from_secs(1);
        assert!(!state.verify_code(&code, later));
    }

    #[test]
    fn verify_trims_surrounding_whitespace() {
        let mut state = TgState::default();
        let now = Instant::now();
        let code = state.generate_code(now);
        assert!(state.verify_code(&format!("  {code} \n"), now));
    }

    #[test]
    fn regenerating_invalidates_the_previous_code() {
        let mut state = TgState::default();
        let now = Instant::now();
        let first = state.generate_code(now);
        let second = state.generate_code(now);
        assert!(!state.verify_code(&first, now) || first == second);
        assert!(state.verify_code(&second, now) || first == second);
    }

    #[test]
    fn auth_failed_flag_round_trips() {
        let mut state = TgState::default();
        assert!(!state.auth_failed());
        state.set_auth_failed();
        assert!(state.auth_failed());
    }
}
```

Create `backend/src/telegram/mod.rs`:

```rust
//! Telegram bot channel: linking state, Bot API client, and the polling loop.

pub mod state;
```

In `backend/src/main.rs`, add to the module list (alphabetical, after `mod service;`):

```rust
mod telegram;
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend && cargo test telegram::state`
Expected: COMPILE ERROR — `generate_code`, `verify_code`, `auth_failed`, `set_auth_failed` not found.

- [ ] **Step 4: Implement TgState**

Add to `backend/src/telegram/state.rs` above the tests:

```rust
impl TgState {
    /// Generate a fresh 6-digit link code, replacing any previous one.
    pub fn generate_code(&mut self, now: Instant) -> String {
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        self.code = Some((code.clone(), now));
        code
    }

    /// Check `input` against the active code. A match consumes the code
    /// (single-use); a mismatch leaves it in place for another attempt.
    pub fn verify_code(&mut self, input: &str, now: Instant) -> bool {
        let matches = match &self.code {
            Some((code, created)) => {
                now.duration_since(*created) <= CODE_TTL && input.trim() == code
            }
            None => false,
        };
        if matches {
            self.code = None;
        }
        matches
    }

    /// Record that Telegram rejected the bot token (401).
    pub fn set_auth_failed(&mut self) {
        self.auth_failed = true;
    }

    pub fn auth_failed(&self) -> bool {
        self.auth_failed
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test telegram::state`
Expected: 7 passed.

- [ ] **Step 6: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/src/telegram/ backend/src/main.rs
git commit -m "feat(backend): add telegram link-code state"
```

---

### Task 4: Bot API client (`telegram/client.rs`)

A thin `reqwest` wrapper mirroring the style of `llm/claude.rs`: typed errors, serde DTOs, unit tests on the pure parsing parts.

**Files:**
- Create: `backend/src/telegram/client.rs`
- Modify: `backend/src/telegram/mod.rs` (register module)

- [ ] **Step 1: Write the failing parsing tests**

Create `backend/src/telegram/client.rs`:

```rust
//! Minimal Telegram Bot API client: long-poll getUpdates + sendMessage.
//! https://core.telegram.org/bots/api

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum TgError {
    #[error("telegram rejected the bot token (401)")]
    Unauthorized,
    #[error("http error: {0}")]
    Http(String),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

#[derive(Debug, Deserialize)]
pub struct TgUpdate {
    pub update_id: i64,
    /// Absent for non-message updates (edits, joins, ...), which we ignore.
    pub message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TgMessage {
    pub chat: TgChat,
    pub from: Option<TgUser>,
    /// Absent for media-only messages, which we ignore.
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TgChat {
    pub id: i64,
}

#[derive(Debug, Deserialize)]
pub struct TgUser {
    pub username: Option<String>,
}

/// Parse a getUpdates response body into updates.
pub fn parse_updates(body: &serde_json::Value) -> Result<Vec<TgUpdate>, TgError> {
    if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(TgError::Shape(format!("ok != true: {body}")));
    }
    let result = body
        .get("result")
        .cloned()
        .ok_or_else(|| TgError::Shape("no result array".into()))?;
    serde_json::from_value(result).map_err(|e| TgError::Shape(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_updates_extracts_text_messages() {
        let body = serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 42,
                "message": {
                    "message_id": 7,
                    "chat": { "id": 12345, "type": "private" },
                    "from": { "id": 12345, "is_bot": false, "username": "bima" },
                    "text": "halo"
                }
            }]
        });
        let updates = parse_updates(&body).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].update_id, 42);
        let msg = updates[0].message.as_ref().unwrap();
        assert_eq!(msg.chat.id, 12345);
        assert_eq!(msg.from.as_ref().unwrap().username.as_deref(), Some("bima"));
        assert_eq!(msg.text.as_deref(), Some("halo"));
    }

    #[test]
    fn parse_updates_tolerates_non_message_updates() {
        let body = serde_json::json!({
            "ok": true,
            "result": [{ "update_id": 43, "my_chat_member": {} }]
        });
        let updates = parse_updates(&body).unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].message.is_none());
    }

    #[test]
    fn parse_updates_rejects_not_ok() {
        let body = serde_json::json!({ "ok": false, "description": "bad" });
        assert!(matches!(parse_updates(&body), Err(TgError::Shape(_))));
    }
}
```

Register in `backend/src/telegram/mod.rs`:

```rust
pub mod client;
```

- [ ] **Step 2: Run tests to verify they pass (pure parsing only so far)**

Run: `cd backend && cargo test telegram::client`
Expected: 3 passed. (The DTOs + `parse_updates` are written together because serde derives are not meaningfully implementable "minimally" — the tests still pin the contract.)

- [ ] **Step 3: Add the HTTP client**

Add to `backend/src/telegram/client.rs` above the tests:

```rust
pub struct TelegramClient {
    token: String,
    client: reqwest::Client,
}

/// Long-poll wait passed to getUpdates (seconds). The HTTP client timeout
/// must comfortably exceed this so the long poll is not cut short.
const POLL_TIMEOUT_SECS: u64 = 30;

impl TelegramClient {
    pub fn new(token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(POLL_TIMEOUT_SECS + 20))
            .build()
            .expect("reqwest client");
        Self { token, client }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }

    async fn check(resp: reqwest::Response) -> Result<serde_json::Value, TgError> {
        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(TgError::Unauthorized);
        }
        let body: serde_json::Value =
            resp.json().await.map_err(|e| TgError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(TgError::Api { status: status.as_u16(), body: body.to_string() });
        }
        Ok(body)
    }

    /// Long-poll for updates after `offset` (pass last update_id + 1).
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<TgUpdate>, TgError> {
        let resp = self
            .client
            .get(self.url("getUpdates"))
            .query(&[("offset", offset.to_string()), ("timeout", POLL_TIMEOUT_SECS.to_string())])
            .send()
            .await
            .map_err(|e| TgError::Http(e.to_string()))?;
        let body = Self::check(resp).await?;
        parse_updates(&body)
    }

    /// Send a plain-text reply to a chat.
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<(), TgError> {
        let resp = self
            .client
            .post(self.url("sendMessage"))
            .json(&serde_json::json!({ "chat_id": chat_id, "text": text }))
            .send()
            .await
            .map_err(|e| TgError::Http(e.to_string()))?;
        Self::check(resp).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Verify it compiles and tests still pass**

Run: `cd backend && cargo test telegram::`
Expected: state + client tests all pass, no warnings about unused items (the poller lands next task; if `cargo test` warns about dead code here, that is acceptable until Task 5 wires it up).

- [ ] **Step 5: Commit**

```bash
git add backend/src/telegram/client.rs backend/src/telegram/mod.rs
git commit -m "feat(backend): add telegram bot api client"
```

---

### Task 5: Polling loop + dispatch (`telegram/mod.rs`)

**Files:**
- Modify: `backend/src/telegram/mod.rs` (add `plan_action`, `spawn`, poll loop)

- [ ] **Step 1: Write the failing dispatch tests**

Replace `backend/src/telegram/mod.rs` with:

```rust
//! Telegram bot channel: linking state, Bot API client, and the polling loop.
//!
//! The poller is spawned from main() only when TELEGRAM_BOT_TOKEN is set. It
//! long-polls getUpdates and answers messages from the linked owner chat via
//! the shared chat service. Messages from unlinked chats are only ever used
//! for the one-time link-code handshake.

pub mod client;
pub mod state;

/// What to do with an inbound text message, decided from the link state.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Linked owner chat: answer via the chat service.
    Answer,
    /// No link exists yet: try the message as a link code.
    TryLink,
    /// A link exists and this is some other chat: ignore silently.
    Ignore,
}

/// Pure dispatch decision: who may talk to the bot.
pub fn plan_action(linked_chat_id: Option<i64>, from_chat_id: i64) -> Action {
    match linked_chat_id {
        Some(id) if id == from_chat_id => Action::Answer,
        Some(_) => Action::Ignore,
        None => Action::TryLink,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_chat_gets_answered() {
        assert_eq!(plan_action(Some(42), 42), Action::Answer);
    }

    #[test]
    fn other_chats_are_ignored_once_linked() {
        assert_eq!(plan_action(Some(42), 99), Action::Ignore);
    }

    #[test]
    fn unlinked_messages_attempt_the_link_code() {
        assert_eq!(plan_action(None, 99), Action::TryLink);
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd backend && cargo test telegram::tests`
Expected: 3 passed.

- [ ] **Step 3: Add the poll loop and spawn()**

Add to `backend/src/telegram/mod.rs` below `plan_action`:

```rust
use crate::db::Db;
use client::{TelegramClient, TgError, TgUpdate};
use state::SharedTgState;
use std::time::Instant;

const LINK_OK_REPLY: &str =
    "✅ Telegram tertaut. Silakan tanya apa saja tentang portofoliomu.";
const LINK_HINT_REPLY: &str =
    "Kode tidak valid atau kedaluwarsa. Buka halaman Telegram di web UI untuk membuat kode tautan.";
const ANSWER_FAILED_REPLY: &str =
    "Maaf, lagi ada gangguan saat menjawab. Coba lagi sebentar lagi ya.";

/// Spawn the background poller when TELEGRAM_BOT_TOKEN is configured.
/// Without the token the Telegram channel is simply off.
pub fn spawn(db: Db, tg: SharedTgState) {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else {
        tracing::info!("TELEGRAM_BOT_TOKEN not set; telegram channel disabled");
        return;
    };
    tokio::spawn(async move {
        poll_loop(TelegramClient::new(token), db, tg).await;
    });
}

/// Long-poll getUpdates forever. Network errors back off and retry; a 401
/// (bad token) flags the state for the UI and waits longer between retries.
async fn poll_loop(client: TelegramClient, db: Db, tg: SharedTgState) {
    tracing::info!("telegram poller started");
    let mut offset = 0i64;
    loop {
        match client.get_updates(offset).await {
            Ok(updates) => {
                for update in updates {
                    offset = offset.max(update.update_id + 1);
                    handle_update(&client, &db, &tg, update).await;
                }
            }
            Err(TgError::Unauthorized) => {
                tracing::error!("telegram rejected the bot token; check TELEGRAM_BOT_TOKEN");
                if let Ok(mut guard) = tg.lock() {
                    guard.set_auth_failed();
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
            Err(e) => {
                tracing::warn!("telegram getUpdates failed: {e}; retrying");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Process one update end-to-end. All failures are logged, never propagated —
/// one bad message must not kill the poller.
async fn handle_update(client: &TelegramClient, db: &Db, tg: &SharedTgState, update: TgUpdate) {
    // Ignore non-message updates and non-text messages.
    let Some(message) = update.message else { return };
    let Some(text) = message.text.as_deref().filter(|t| !t.trim().is_empty()) else { return };
    let chat_id = message.chat.id;

    let linked = match crate::repo::telegram_link::get(db).await {
        Ok(row) => row.map(|r| r.chat_id),
        Err(e) => {
            tracing::error!("telegram: failed to read link row: {e:#}");
            return;
        }
    };

    match plan_action(linked, chat_id) {
        Action::Answer => {
            let reply = answer(db, text).await.unwrap_or_else(|e| {
                tracing::error!("telegram: answer failed: {e:#}");
                ANSWER_FAILED_REPLY.to_string()
            });
            send_or_log(client, chat_id, &reply).await;
        }
        Action::TryLink => {
            let code_ok = match tg.lock() {
                Ok(mut guard) => guard.verify_code(text, Instant::now()),
                Err(_) => false,
            };
            if code_ok {
                let username = message.from.as_ref().and_then(|u| u.username.as_deref());
                match crate::repo::telegram_link::set(db, chat_id, username).await {
                    Ok(()) => {
                        tracing::info!("telegram: linked chat {chat_id}");
                        send_or_log(client, chat_id, LINK_OK_REPLY).await;
                    }
                    Err(e) => tracing::error!("telegram: failed to persist link: {e:#}"),
                }
            } else {
                send_or_log(client, chat_id, LINK_HINT_REPLY).await;
            }
        }
        Action::Ignore => {}
    }
}

/// Answer a linked owner message via the shared chat service.
async fn answer(db: &Db, text: &str) -> anyhow::Result<String> {
    let llm = crate::llm::claude::ClaudeClient::from_env()
        .map_err(|e| anyhow::anyhow!("chat unavailable: {e}"))?;
    crate::service::chat::answer(db, &llm, "telegram", text).await
}

async fn send_or_log(client: &TelegramClient, chat_id: i64, text: &str) {
    if let Err(e) = client.send_message(chat_id, text).await {
        tracing::error!("telegram: sendMessage to {chat_id} failed: {e}");
    }
}
```

- [ ] **Step 4: Verify the whole crate compiles and tests pass**

Run: `cd backend && cargo test telegram::`
Expected: all telegram tests pass; `cargo test` compiles cleanly. (`spawn` is not yet called from main — a dead-code warning is acceptable until Task 6.)

- [ ] **Step 5: Commit**

```bash
git add backend/src/telegram/mod.rs
git commit -m "feat(backend): add telegram polling loop with link handshake"
```

---

### Task 6: API endpoints + wiring (`api/telegram.rs`, AppState, main)

**Files:**
- Modify: `backend/src/error.rs:6-32` (add `Conflict`)
- Create: `backend/src/api/telegram.rs`
- Modify: `backend/src/api/mod.rs:1-9` (module), `:32-38` (routes), `:115-121` (test state)
- Modify: `backend/src/main.rs:19-36` (AppState field + spawn)

- [ ] **Step 1: Add the Conflict error variant**

In `backend/src/error.rs`, add a variant after `BadRequest` (line 11):

```rust
    #[error("conflict: {0}")]
    Conflict(String),
```

and a match arm in `into_response` after the `BadRequest` arm (line 22):

```rust
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
```

- [ ] **Step 2: Wire AppState and spawn first (handlers need `s.tg`)**

In `backend/src/main.rs`, replace lines 15-37 (imports + AppState + state construction) so they read:

```rust
use db::Db;
use std::sync::{Arc, Mutex};
use telegram::state::{SharedTgState, TgState};
use wa_state::{SharedWaState, WaState};

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
    pub wa: SharedWaState,
    pub tg: SharedTgState,
}
```

and inside `main()`:

```rust
    let state = AppState {
        db: db.clone(),
        wa: Arc::new(Mutex::new(WaState::default())),
        tg: Arc::new(Mutex::new(TgState::default())),
    };
    telegram::spawn(db.clone(), state.tg.clone());
    scheduler::spawn(db, std::time::Duration::from_secs(3600));
```

(Note: `telegram::spawn` takes `db` by value-clone before `scheduler::spawn` consumes `db`.)

In `backend/src/api/mod.rs` test helper (line 115-121), add the new field:

```rust
    async fn test_state() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        AppState {
            db,
            wa: Default::default(),
            tg: Default::default(),
        }
    }
```

- [ ] **Step 3: Write the failing handler tests**

Create `backend/src/api/telegram.rs`:

```rust
//! Frontend-facing Telegram linking endpoints (JWT-protected via the router).
//!
//! The bot itself does not call these — inbound traffic arrives through the
//! long-poller in `crate::telegram`, not through HTTP.

use crate::error::AppError;
use crate::telegram::state::CODE_TTL_SECS;
use crate::AppState;
use axum::{extract::State, Json};
use serde::Serialize;
use std::time::Instant;

#[derive(Serialize)]
pub struct TelegramStatusView {
    /// Token present and not rejected by Telegram.
    pub configured: bool,
    pub linked: bool,
    pub username: Option<String>,
}

#[derive(Serialize)]
pub struct LinkCodeOut {
    pub code: String,
    pub expires_in: u64,
}

fn lock_tg(
    s: &AppState,
) -> Result<std::sync::MutexGuard<'_, crate::telegram::state::TgState>, AppError> {
    s.tg
        .lock()
        .map_err(|_| AppError::Other(anyhow::anyhow!("tg state poisoned")))
}

fn token_configured() -> bool {
    std::env::var("TELEGRAM_BOT_TOKEN").is_ok_and(|t| !t.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    async fn test_state() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        AppState {
            db,
            wa: Default::default(),
            tg: Default::default(),
        }
    }

    // These tests mutate TELEGRAM_BOT_TOKEN, so they run serially.
    #[serial]
    #[tokio::test]
    async fn status_reports_unconfigured_without_token() {
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
        let s = test_state().await;
        let Json(view) = status(State(s)).await.unwrap();
        assert!(!view.configured);
        assert!(!view.linked);
        assert_eq!(view.username, None);
    }

    #[serial]
    #[tokio::test]
    async fn status_reports_linked_username() {
        std::env::set_var("TELEGRAM_BOT_TOKEN", "123:abc");
        let s = test_state().await;
        crate::repo::telegram_link::set(&s.db, 42, Some("bima")).await.unwrap();
        let Json(view) = status(State(s)).await.unwrap();
        assert!(view.configured);
        assert!(view.linked);
        assert_eq!(view.username.as_deref(), Some("bima"));
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
    }

    #[serial]
    #[tokio::test]
    async fn link_code_conflicts_when_unconfigured() {
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
        let s = test_state().await;
        let err = link_code(State(s)).await.err().expect("must fail");
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[serial]
    #[tokio::test]
    async fn link_code_returns_a_six_digit_code() {
        std::env::set_var("TELEGRAM_BOT_TOKEN", "123:abc");
        let s = test_state().await;
        let Json(out) = link_code(State(s.clone())).await.unwrap();
        assert_eq!(out.code.len(), 6);
        assert_eq!(out.expires_in, CODE_TTL_SECS);
        // The generated code is actually verifiable in the shared state.
        assert!(s.tg.lock().unwrap().verify_code(&out.code, Instant::now()));
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
    }

    #[serial]
    #[tokio::test]
    async fn unlink_clears_the_link() {
        let s = test_state().await;
        crate::repo::telegram_link::set(&s.db, 42, None).await.unwrap();
        unlink(State(s.clone())).await.unwrap();
        assert!(crate::repo::telegram_link::get(&s.db).await.unwrap().is_none());
    }
}
```

Register the module in `backend/src/api/mod.rs` (alphabetical, after `pub mod portfolio;`):

```rust
pub mod telegram;
```

and add routes in the `protected` router after the whatsapp routes (line 38):

```rust
        .route("/telegram/status", get(telegram::status))
        .route("/telegram/link-code", post(telegram::link_code))
        .route("/telegram/unlink", post(telegram::unlink))
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cd backend && cargo test api::telegram`
Expected: COMPILE ERROR — `status`, `link_code`, `unlink` not found.

- [ ] **Step 5: Implement the handlers**

Add to `backend/src/api/telegram.rs` above the tests:

```rust
/// Linking status for the web UI. `configured` is false when the token is
/// missing OR Telegram rejected it (auth_failed) — either way the channel
/// is not usable and the UI should say so.
pub async fn status(State(s): State<AppState>) -> Result<Json<TelegramStatusView>, AppError> {
    let auth_failed = lock_tg(&s)?.auth_failed();
    let link = crate::repo::telegram_link::get(&s.db)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(TelegramStatusView {
        configured: token_configured() && !auth_failed,
        linked: link.is_some(),
        username: link.and_then(|l| l.username),
    }))
}

/// Generate a fresh one-time link code (invalidates any previous code).
pub async fn link_code(State(s): State<AppState>) -> Result<Json<LinkCodeOut>, AppError> {
    if !token_configured() {
        return Err(AppError::Conflict(
            "telegram bot is not configured (set TELEGRAM_BOT_TOKEN)".into(),
        ));
    }
    let code = lock_tg(&s)?.generate_code(Instant::now());
    Ok(Json(LinkCodeOut { code, expires_in: CODE_TTL_SECS }))
}

/// Remove the owner link; the bot stops answering until re-linked.
pub async fn unlink(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    crate::repo::telegram_link::clear(&s.db)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(()))
}
```

- [ ] **Step 6: Run the full backend suite**

Run: `cd backend && cargo test`
Expected: ALL tests pass (including the existing router tests with the new `tg` field).

- [ ] **Step 7: Commit**

```bash
git add backend/src/error.rs backend/src/api/telegram.rs backend/src/api/mod.rs backend/src/main.rs
git commit -m "feat(backend): add telegram linking endpoints and poller wiring"
```

---

### Task 7: Frontend schema, hooks, MSW handler

**Files:**
- Modify: `frontend/src/api/schemas.ts` (after the WhatsApp section, line 280)
- Modify: `frontend/src/api/hooks.ts` (after the WhatsApp hooks, line 212)
- Modify: `frontend/src/test/server.ts:142-146` (default handler)

- [ ] **Step 1: Add schemas**

In `frontend/src/api/schemas.ts`, after `WhatsappStatus` (line 279), add:

```typescript
// ── Telegram connection ─────────────────────────────────────────────────────

export const TelegramStatusSchema = z.object({
  configured: z.boolean(),
  linked: z.boolean(),
  username: z.string().nullable(),
});
export type TelegramStatus = z.infer<typeof TelegramStatusSchema>;

export const TelegramLinkCodeSchema = z.object({
  code: z.string(),
  expires_in: z.number(),
});
export type TelegramLinkCode = z.infer<typeof TelegramLinkCodeSchema>;
```

- [ ] **Step 2: Add hooks**

In `frontend/src/api/hooks.ts`:

Add `TelegramStatusSchema, TelegramLinkCodeSchema,` to the schema import list (after `WhatsappStatusSchema,` on line 14).

Append at the end of the file:

```typescript
// ── Telegram connection hooks ────────────────────────────────────────────────

export const useTelegramStatus = () =>
  useQuery({
    queryKey: ["telegram-status"],
    queryFn: () => api.get("/telegram/status", TelegramStatusSchema),
    refetchInterval: 2000,
  });

export const useTelegramLinkCode = () =>
  useMutation({
    mutationFn: () => api.post("/telegram/link-code", TelegramLinkCodeSchema, {}),
  });

export const useUnlinkTelegram = () => {
  const qc = useQueryClient();
  return useMutation({
    mutationFn: () => api.post("/telegram/unlink", z.unknown(), {}),
    onSuccess: () => { qc.invalidateQueries({ queryKey: ["telegram-status"] }); },
  });
};
```

- [ ] **Step 3: Add the default MSW handler**

In `frontend/src/test/server.ts`, after the WhatsApp handler (line 145), add:

```typescript
  // ── Telegram ───────────────────────────────────────────────────────────────
  http.get("/api/telegram/status", () =>
    HttpResponse.json({ configured: true, linked: false, username: null }),
  ),
```

- [ ] **Step 4: Verify it compiles and existing tests pass**

Run: `cd frontend && npm test`
Expected: all existing tests pass.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts frontend/src/test/server.ts
git commit -m "feat(frontend): add telegram status/link api hooks"
```

---

### Task 8: TelegramPage + tests

**Files:**
- Create: `frontend/src/pages/TelegramPage.test.tsx`
- Create: `frontend/src/pages/TelegramPage.tsx`

- [ ] **Step 1: Write the failing page tests**

Create `frontend/src/pages/TelegramPage.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../test/server";
import TelegramPage from "./TelegramPage";

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <TelegramPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("shows setup instructions when the bot token is not configured", async () => {
  server.use(
    http.get("/api/telegram/status", () =>
      HttpResponse.json({ configured: false, linked: false, username: null }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/TELEGRAM_BOT_TOKEN/)).toBeInTheDocument());
});

test("shows the generate-code button when configured but unlinked", async () => {
  // Default handler in server.ts returns { configured: true, linked: false }
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /buat kode tautan/i })).toBeInTheDocument(),
  );
});

test("generating a code displays it with instructions", async () => {
  server.use(
    http.post("/api/telegram/link-code", () =>
      HttpResponse.json({ code: "123456", expires_in: 600 }),
    ),
  );
  renderPage();
  const button = await screen.findByRole("button", { name: /buat kode tautan/i });
  await userEvent.click(button);
  await waitFor(() => expect(screen.getByText("123456")).toBeInTheDocument());
  expect(screen.getByText(/kirim kode ini/i)).toBeInTheDocument();
});

test("shows the linked username and unlink button when linked", async () => {
  server.use(
    http.get("/api/telegram/status", () =>
      HttpResponse.json({ configured: true, linked: true, username: "bima" }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/@bima/)).toBeInTheDocument());
  expect(screen.getByRole("button", { name: /putus tautan/i })).toBeInTheDocument();
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd frontend && npm test -- TelegramPage`
Expected: FAIL — `./TelegramPage` module not found.

- [ ] **Step 3: Implement the page**

Create `frontend/src/pages/TelegramPage.tsx`:

```tsx
import { useState } from "react";
import { toast } from "sonner";
import { useTelegramStatus, useTelegramLinkCode, useUnlinkTelegram } from "../api/hooks";

/**
 * Telegram linking control. The bot token lives in the backend env; this page
 * only drives the one-time link-code handshake and shows the current status.
 */
export default function TelegramPage() {
  const statusQuery = useTelegramStatus();
  const linkCode = useTelegramLinkCode();
  const unlink = useUnlinkTelegram();
  const [code, setCode] = useState<string | null>(null);

  const configured = statusQuery.data?.configured ?? true;
  const linked = statusQuery.data?.linked ?? false;
  const username = statusQuery.data?.username;

  const handleGenerate = () =>
    linkCode.mutate(undefined, {
      onSuccess: (out) => setCode(out.code),
      onError: (err) => toast.error((err as Error).message),
    });

  const handleUnlink = () =>
    unlink.mutate(undefined, {
      onSuccess: () => {
        setCode(null);
        toast.success("Tautan Telegram diputus");
      },
      onError: (err) => toast.error((err as Error).message),
    });

  return (
    <div>
      <h1 className="t-h1">Telegram</h1>
      <div className="t-sm t-muted" style={{ marginBottom: 12 }}>Hubungkan bot Telegram</div>

      <div className="card" style={{ padding: 22, maxWidth: 420 }}>
        {!configured && (
          <p className="t-sm t-muted">
            Bot Telegram belum dikonfigurasi. Buat bot lewat @BotFather, lalu set
            env <code>TELEGRAM_BOT_TOKEN</code> di backend dan restart.
          </p>
        )}

        {configured && linked && (
          <div className="col gap-3">
            <p className="t-sm">
              Tertaut sebagai <strong>@{username ?? "(tanpa username)"}</strong>
            </p>
            <button
              type="button"
              className="btn btn-danger"
              disabled={unlink.isPending}
              onClick={handleUnlink}
            >
              Putus Tautan
            </button>
          </div>
        )}

        {configured && !linked && code && (
          <div style={{ textAlign: "center" }}>
            <div style={{ fontSize: 36, fontWeight: 700, letterSpacing: 6 }}>{code}</div>
            <p className="t-sm t-muted" style={{ marginTop: 12 }}>
              Kirim kode ini sebagai pesan ke bot Telegram kamu. Kode berlaku 10 menit.
              Halaman ini akan terbarui otomatis setelah tertaut.
            </p>
          </div>
        )}

        {configured && !linked && !code && (
          <button
            type="button"
            className="btn btn-primary"
            disabled={linkCode.isPending}
            onClick={handleGenerate}
          >
            Buat Kode Tautan
          </button>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd frontend && npm test -- TelegramPage`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/TelegramPage.tsx frontend/src/pages/TelegramPage.test.tsx
git commit -m "feat(frontend): add telegram linking page"
```

---

### Task 9: Route + navigation

**Files:**
- Modify: `frontend/src/App.tsx:14,38` (import + route)
- Modify: `frontend/src/components/AppShell.tsx:12-30,46-55` (icon import + nav item)

- [ ] **Step 1: Add the route**

In `frontend/src/App.tsx`, add the import after `WhatsAppPage` (line 14):

```tsx
import TelegramPage from "./pages/TelegramPage";
```

and the route after the whatsapp route (line 38):

```tsx
        <Route path="telegram" element={<TelegramPage />} />
```

- [ ] **Step 2: Add the nav item**

In `frontend/src/components/AppShell.tsx`, add `Send,` to the lucide-react import list (after `MessageCircle,` on line 20), and add to `NAV_ITEMS` after the WhatsApp entry (line 54):

```tsx
  { to: "/telegram",  label: "Telegram",   icon: Send },
```

- [ ] **Step 3: Verify build + tests**

Run: `cd frontend && npm test && npm run build`
Expected: all tests pass; `tsc -b && vite build` succeeds.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(frontend): add /telegram route and nav item"
```

---

### Task 10: Deployment configuration

**Files:**
- Modify: `docker-compose.yml` (backend environment block)
- Modify: `docker-compose.prod.yml` (backend environment block)
- Modify: `.env.production.example`
- Modify: `k8s/10-backend.yaml:39-67` (env list)
- Modify: `k8s/secret.example.yaml`

- [ ] **Step 1: docker-compose files**

In BOTH `docker-compose.yml` and `docker-compose.prod.yml`, in the `backend:` service `environment:` block, add after `GATEWAY_TOKEN`:

```yaml
      # Optional: enables the Telegram chatbot channel when set.
      TELEGRAM_BOT_TOKEN: ${TELEGRAM_BOT_TOKEN:-}
```

- [ ] **Step 2: Env example**

In `.env.production.example`, add after the `GATEWAY_TOKEN` block:

```bash
# Telegram bot token from @BotFather — optional. Leave empty to disable the
# Telegram chatbot channel.
TELEGRAM_BOT_TOKEN=
```

- [ ] **Step 3: k8s manifests**

In `k8s/10-backend.yaml`, add to the backend container `env:` list after the `JWT_SECRET` entry (line 67):

```yaml
            # Optional: enables the Telegram chatbot channel when present in the secret.
            - name: TELEGRAM_BOT_TOKEN
              valueFrom:
                secretKeyRef:
                  name: portfolio-secrets
                  key: TELEGRAM_BOT_TOKEN
                  optional: true
```

In `k8s/secret.example.yaml`, add to `stringData:`:

```yaml
  TELEGRAM_BOT_TOKEN: "REPLACE_ME_OR_OMIT"
```

and extend the comment block's `kubectl create secret` example with the new literal:

```
#     --from-literal=TELEGRAM_BOT_TOKEN=123456:ABC-...
```

- [ ] **Step 4: Validate YAML**

Run: `docker compose -f docker-compose.yml config -q && docker compose -f docker-compose.prod.yml config -q`
Expected: exits 0 for both (warnings about missing env values are fine; with `:-` defaults there should be none for the new var).

- [ ] **Step 5: Commit**

```bash
git add docker-compose.yml docker-compose.prod.yml .env.production.example k8s/10-backend.yaml k8s/secret.example.yaml
git commit -m "feat(deploy): pass optional TELEGRAM_BOT_TOKEN to backend"
```

---

### Task 11: Full verification

- [ ] **Step 1: Backend suite**

Run: `cd backend && cargo test`
Expected: ALL pass, including migration of `0008_telegram_link.sql` in every `sqlite::memory:` test db.

- [ ] **Step 2: Backend lint**

Run: `cd backend && cargo clippy -- -D warnings`
Expected: clean. Fix any warnings introduced by the new modules.

- [ ] **Step 3: Frontend suite + build**

Run: `cd frontend && npm test && npm run build`
Expected: all tests pass; build succeeds.

- [ ] **Step 4: Manual smoke (optional, needs a real bot token)**

```bash
cd backend && TELEGRAM_BOT_TOKEN=<real token> ANTHROPIC_API_KEY=<key> cargo run
```

- Open the web UI → Telegram → Buat Kode Tautan → send the code to the bot from your Telegram account → page flips to "Tertaut sebagai @…" → ask "berapa net worth saya?" → bot replies.
- Send a message from a second Telegram account → silently ignored.

- [ ] **Step 5: Final commit (if any fixups)**

```bash
git add -A && git commit -m "test: fixups from full verification"
```
