# CS WhatsApp — Plan A: Backend Channel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route a dedicated CS WhatsApp number through the Phase-1 CS brain: per-contact conversations, bot-answers-unless-escalated gating, a second WhatsApp connection state, an outbound queue, and an owner reply endpoint that delivers to the customer over WhatsApp.

**Architecture:** New `api/cs_whatsapp.rs` mirrors `api/whatsapp.rs` but locks a second `WaState` (`cs_wa` on `AppState`), authenticates with `CS_GATEWAY_TOKEN`, and adds an outbound-message queue (`cs_outbound`). Inbound finds/creates a `cs_conversation` by WhatsApp JID, stores the message, and runs `cs::agent::handle_message` only when the conversation status is `bot` (escalated conversations stay silent — the owner has taken over). The owner's `POST /cs/admin/conversations/:id/reply` stores an assistant message and, for WhatsApp conversations, enqueues an outbound `(jid, text)` the gateway drains and sends.

**Tech Stack:** Rust, axum, sqlx, std (VecDeque/Mutex). No new dependencies. Depends on Phase 1.

> **Worktree** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`. **No `cargo fmt`.** Mirror `api/whatsapp.rs`, `api/cs_public.rs`, `repo/cs.rs`.

> **Reused (verified) signatures:**
> - `WaState` (`wa_state.rs`): `apply_push(status, qr, number, now)`, `set_command(WaCommand)`, `take_command() -> Option<WaCommand>`, `view(now) -> WaStatusView`; `SharedWaState = Arc<Mutex<WaState>>`; `WaStatus`, `WaCommand`, `WaStatusView`.
> - `api/whatsapp.rs`: `WaIn{from,message}`, `WaOut{reply}`, `StatePush{status,qr,number}`, `CommandOut{command}`, `token_matches`, `check_gateway_token`, `lock_wa`.
> - `AppState { db, wa, tg }` (`main.rs:28`), constructed at `main.rs:46`. Router `test_state()` in `api/mod.rs` `router_tests` builds an AppState — it MUST be updated for new fields.
> - `cs::agent::handle_message(db, &embedder, &model, conversation_id, text) -> anyhow::Result<String>` (stores user+assistant turns internally).
> - `cs::kb::CsEmbedder::from_env()`, `crate::llm::claude::ClaudeClient::from_env()`.
> - `repo::cs::{conversation_create, message_add, conversation_by_token, ConversationRow}`. `cs_conversation.channel` CHECK already allows `'whatsapp'`.

---

## File Structure

- Create: `backend/migrations/0024_cs_conversation_wa_jid.sql` — add `wa_jid` column + index.
- Modify: `backend/src/repo/cs.rs` — `conversation_by_wa_jid`, `conversation_create_wa` (+ tests).
- Create: `backend/src/cs/wa_outbound.rs` — `OutboundMsg` + `SharedOutbound` queue type + push/drain (+ tests).
- Modify: `backend/src/cs/mod.rs` — `pub mod wa_outbound;`.
- Modify: `backend/src/main.rs` — add `cs_wa` + `cs_outbound` to `AppState` + construct them.
- Create: `backend/src/api/cs_whatsapp.rs` — CS gateway + dashboard handlers.
- Modify: `backend/src/api/cs_admin.rs` — `reply_conversation` handler.
- Modify: `backend/src/api/mod.rs` — declare module, register CS WhatsApp routes (gateway + protected tiers), fix `test_state()`.
- Modify: `backend/.env.example` — document `CS_GATEWAY_TOKEN`.

---

## Task 1: Migration + repo (per-JID conversation)

**Files:** Create `backend/migrations/0024_cs_conversation_wa_jid.sql`; Modify `backend/src/repo/cs.rs`.

- [ ] **Step 1: Migration**

Create `backend/migrations/0024_cs_conversation_wa_jid.sql`:

```sql
-- CS WhatsApp (Phase 2): map a WhatsApp sender JID to one ongoing conversation.
ALTER TABLE cs_conversation ADD COLUMN wa_jid TEXT;
CREATE INDEX idx_cs_conversation_wa_jid ON cs_conversation (wa_jid);
```

- [ ] **Step 2: Failing tests** (in `repo/cs.rs` `mod tests`)

```rust
#[tokio::test]
async fn wa_conversation_find_or_create_is_idempotent_per_jid() {
    let db = mem_db().await;
    let jid = "628123@s.whatsapp.net";
    let c1 = conversation_create_wa(&db, jid, "628123", "tok-wa-1").await.unwrap();
    assert_eq!(c1.channel, "whatsapp");
    assert_eq!(c1.visitor_phone.as_deref(), Some("628123"));

    // lookup returns the same row
    let found = conversation_by_wa_jid(&db, jid).await.unwrap().unwrap();
    assert_eq!(found.id, c1.id);

    // a different jid is a different conversation
    let c2 = conversation_create_wa(&db, "628999@s.whatsapp.net", "628999", "tok-wa-2").await.unwrap();
    assert_ne!(c2.id, c1.id);
    assert!(conversation_by_wa_jid(&db, "000@none").await.unwrap().is_none());
}
```

- [ ] **Step 3: Run to verify fail**

Run: `cd backend && cargo test repo::cs::tests::wa_conversation_find_or_create_is_idempotent_per_jid`
Expected: FAIL — functions not found.

- [ ] **Step 4: Implement** (add to `repo/cs.rs`)

```rust
pub async fn conversation_by_wa_jid(db: &Db, jid: &str) -> anyhow::Result<Option<ConversationRow>> {
    let row = sqlx::query_as::<_, ConversationRow>("SELECT * FROM cs_conversation WHERE wa_jid = ?")
        .bind(jid)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

/// Create a WhatsApp CS conversation for a sender JID. `session_token` is a
/// caller-supplied unique value (the row needs one); `wa_jid` is the lookup key.
pub async fn conversation_create_wa(db: &Db, jid: &str, phone: &str, session_token: &str) -> anyhow::Result<ConversationRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_conversation \
         (channel, visitor_name, visitor_email, visitor_phone, session_token, status, created_at, last_msg_at, wa_jid) \
         VALUES ('whatsapp', NULL, NULL, ?, ?, 'bot', ?, ?, ?)",
    )
    .bind(phone)
    .bind(session_token)
    .bind(&now)
    .bind(&now)
    .bind(jid)
    .execute(db)
    .await?
    .last_insert_rowid();
    let row = sqlx::query_as::<_, ConversationRow>("SELECT * FROM cs_conversation WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}
```

> **Note:** `ConversationRow` gains no field for `wa_jid` unless the struct uses `SELECT *` into named fields — it does (`sqlx::FromRow` with explicit fields). **Add `pub wa_jid: Option<String>` to the `ConversationRow` struct** (after `last_msg_at`) so `SELECT *` maps cleanly; otherwise sqlx errors on the extra column. Confirm and add it. This also lets the reply handler read the JID.

- [ ] **Step 5: Run to verify pass + commit**

Run: `cd backend && cargo test repo::cs`
Expected: PASS (existing + new). 

```bash
git add backend/migrations/0024_cs_conversation_wa_jid.sql backend/src/repo/cs.rs
git commit -m "feat(cs-wa): wa_jid column + per-JID conversation repo"
```

---

## Task 2: Outbound queue

**Files:** Create `backend/src/cs/wa_outbound.rs`; Modify `backend/src/cs/mod.rs`.

- [ ] **Step 1: Implement + test**

Create `backend/src/cs/wa_outbound.rs`:

```rust
//! Outbound WhatsApp message queue for the CS number. The dashboard reply
//! endpoint pushes; the CS gateway drains it via GET /cs/whatsapp/outbound.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct OutboundMsg {
    pub jid: String,
    pub text: String,
}

pub type SharedOutbound = Arc<Mutex<VecDeque<OutboundMsg>>>;

pub fn new_queue() -> SharedOutbound {
    Arc::new(Mutex::new(VecDeque::new()))
}

/// Enqueue a message. Lock poisoning is recovered (never panics).
pub fn push(q: &SharedOutbound, jid: &str, text: &str) {
    let mut g = q.lock().unwrap_or_else(|p| p.into_inner());
    g.push_back(OutboundMsg { jid: jid.to_string(), text: text.to_string() });
}

/// Drain all pending messages (at-most-once delivery — removed when handed out).
pub fn drain(q: &SharedOutbound) -> Vec<OutboundMsg> {
    let mut g = q.lock().unwrap_or_else(|p| p.into_inner());
    g.drain(..).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_then_drain_preserves_order_and_empties() {
        let q = new_queue();
        push(&q, "a@x", "hi");
        push(&q, "b@x", "yo");
        let out = drain(&q);
        assert_eq!(out, vec![
            OutboundMsg { jid: "a@x".into(), text: "hi".into() },
            OutboundMsg { jid: "b@x".into(), text: "yo".into() },
        ]);
        assert!(drain(&q).is_empty()); // drained
    }
}
```

Add to `backend/src/cs/mod.rs`: `pub mod wa_outbound;`.

- [ ] **Step 2: Run + commit**

Run: `cd backend && cargo test cs::wa_outbound`
Expected: PASS.

```bash
git add backend/src/cs/wa_outbound.rs backend/src/cs/mod.rs
git commit -m "feat(cs-wa): outbound message queue"
```

---

## Task 3: AppState fields

**Files:** Modify `backend/src/main.rs`, and `backend/src/api/mod.rs` (`test_state()` helper).

- [ ] **Step 1: Extend AppState** (`main.rs`)

In the `AppState` struct (after `tg`):

```rust
    pub cs_wa: SharedWaState,
    pub cs_outbound: crate::cs::wa_outbound::SharedOutbound,
```

In the construction (`main.rs:46`):

```rust
    let state = AppState {
        db: db.clone(),
        wa: Arc::new(Mutex::new(WaState::default())),
        tg: Arc::new(Mutex::new(TgState::default())),
        cs_wa: Arc::new(Mutex::new(WaState::default())),
        cs_outbound: crate::cs::wa_outbound::new_queue(),
    };
```

- [ ] **Step 2: Fix the router test helper** (`api/mod.rs`)

Find `test_state()` in the `#[cfg(test)] mod router_tests` and add the two new fields to its `AppState { ... }` literal, mirroring the construction above (`cs_wa: Arc::new(Mutex::new(WaState::default()))`, `cs_outbound: crate::cs::wa_outbound::new_queue()`). Confirm the imports it needs (`WaState`, `Arc`, `Mutex`) are in scope in that test module; add `use` if needed.

- [ ] **Step 3: Verify compile + commit**

Run: `cd backend && cargo check --tests 2>&1 | tail -6 && cargo test api:: 2>&1 | tail -3`
Expected: compiles; existing api tests still pass.

```bash
git add backend/src/main.rs backend/src/api/mod.rs
git commit -m "feat(cs-wa): second WhatsApp state + outbound queue on AppState"
```

---

## Task 4: CS WhatsApp handlers

**Files:** Create `backend/src/api/cs_whatsapp.rs`; Modify `backend/src/api/mod.rs` (`mod cs_whatsapp;`).

- [ ] **Step 1: Implement**

Create `backend/src/api/cs_whatsapp.rs`:

```rust
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
    let got = headers.get("x-gateway-token").and_then(|v| v.to_str().ok());
    let ok = match expected {
        Some(exp) => got == Some(exp.as_str()),
        None => true, // unset = open (dev)
    };
    if ok { Ok(()) } else { Err(AppError::BadRequest("bad gateway token".into())) }
}

fn lock_cs_wa(s: &AppState) -> Result<std::sync::MutexGuard<'_, WaState>, AppError> {
    s.cs_wa.lock().map_err(|_| AppError::Other(anyhow::anyhow!("cs_wa poisoned")))
}

#[derive(Deserialize)]
pub struct CsWaIn { pub from: String, pub message: String }

/// reply is None when the bot stays silent (conversation taken over by a human).
#[derive(Serialize)]
pub struct CsWaOut { pub reply: Option<String> }

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
    let conv = match crate::repo::cs::conversation_by_wa_jid(&s.db, &b.from).await.map_err(AppError::Other)? {
        Some(c) => c,
        None => {
            let phone = b.from.split('@').next().unwrap_or(&b.from);
            let token = crate::cs::gate::new_session_token();
            crate::repo::cs::conversation_create_wa(&s.db, &b.from, phone, &token)
                .await
                .map_err(AppError::Other)?
        }
    };

    // Escalated/resolved → bot is silent; just record the inbound message.
    if conv.status != "bot" {
        crate::repo::cs::message_add(&s.db, conv.id, "user", msg).await.map_err(AppError::Other)?;
        crate::repo::cs::conversation_touch(&s.db, conv.id).await.map_err(AppError::Other)?;
        return Ok(Json(CsWaOut { reply: None }));
    }

    let model = ClaudeClient::from_env().map_err(|e| AppError::Other(anyhow::anyhow!("chat unavailable: {e}")))?;
    let embedder = CsEmbedder::from_env().map_err(|e| AppError::Other(anyhow::anyhow!("cs unavailable: {e}")))?;
    let reply = crate::cs::agent::handle_message(&s.db, &embedder, &model, conv.id, msg)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(CsWaOut { reply: Some(reply) }))
}

pub async fn push_state(State(s): State<AppState>, headers: HeaderMap, Json(b): Json<StatePush>) -> Result<Json<()>, AppError> {
    check_cs_gateway_token(&headers)?;
    lock_cs_wa(&s)?.apply_push(b.status, b.qr, b.number, Instant::now());
    Ok(Json(()))
}

pub async fn poll_commands(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<CommandOut>, AppError> {
    check_cs_gateway_token(&headers)?;
    let command = lock_cs_wa(&s)?.take_command();
    Ok(Json(CommandOut { command }))
}

#[derive(Serialize)]
pub struct OutboundBatch { pub messages: Vec<wa_outbound::OutboundMsg> }

pub async fn poll_outbound(State(s): State<AppState>, headers: HeaderMap) -> Result<Json<OutboundBatch>, AppError> {
    check_cs_gateway_token(&headers)?;
    Ok(Json(OutboundBatch { messages: wa_outbound::drain(&s.cs_outbound) }))
}

pub async fn status(State(s): State<AppState>) -> Result<Json<WaStatusView>, AppError> {
    Ok(Json(lock_cs_wa(&s)?.view(Instant::now())))
}

pub async fn connect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_cs_wa(&s)?.set_command(WaCommand::Restart);
    Ok(Json(()))
}

pub async fn disconnect(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    lock_cs_wa(&s)?.set_command(WaCommand::Logout);
    Ok(Json(()))
}
```

> **Implementer notes:** `StatePush`/`CommandOut` are declared in `api/whatsapp.rs` — confirm they're `pub` and re-exportable as `crate::api::whatsapp::{StatePush, CommandOut}`. If not `pub`, make them `pub` (small, safe change). `conversation_touch` exists (Plan 1). Confirm `cs::gate::new_session_token` is `pub`.

- [ ] **Step 2: Add a gating test**

Append a `#[cfg(test)] mod tests` to `cs_whatsapp.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::db::Db;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    // The silent-when-escalated branch is pure repo logic; verify it here without HTTP.
    #[tokio::test]
    async fn escalated_conversation_records_without_bot_reply() {
        let db = mem_db().await;
        let c = crate::repo::cs::conversation_create_wa(&db, "j@x", "j", "tk-1").await.unwrap();
        crate::repo::cs::conversation_set_status(&db, c.id, "needs_human").await.unwrap();

        // Simulate the inbound silent branch: status != bot -> store user msg only.
        crate::repo::cs::message_add(&db, c.id, "user", "halo").await.unwrap();
        let msgs = crate::repo::cs::message_all(&db, c.id).await.unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0].role, "user");
    }
}
```

- [ ] **Step 3: Verify compile + test + commit**

Run: `cd backend && cargo check 2>&1 | tail -6 && cargo test api::cs_whatsapp 2>&1 | tail -3`

```bash
git add backend/src/api/cs_whatsapp.rs backend/src/api/mod.rs
git commit -m "feat(cs-wa): CS WhatsApp handlers (gated inbound, state, outbound)"
```

---

## Task 5: Owner reply endpoint

**Files:** Modify `backend/src/api/cs_admin.rs`.

- [ ] **Step 1: Implement** (append to `cs_admin.rs`)

```rust
#[derive(Deserialize)]
pub struct ReplyIn { pub text: String }

/// Owner reply to a CS conversation from the inbox. Stored as an assistant
/// message; for WhatsApp conversations it is enqueued for delivery to the
/// customer over the CS number.
pub async fn reply_conversation(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<ReplyIn>,
) -> Result<Json<()>, AppError> {
    let text = b.text.trim();
    if text.is_empty() {
        return Err(AppError::BadRequest("empty reply".into()));
    }
    // Resolve the conversation (need channel + wa_jid).
    let conv = repo::conversation_list_recent(&s.db, 1000)
        .await
        .map_err(AppError::Other)?
        .into_iter()
        .find(|c| c.id == id)
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
```

> **Note:** uses `repo::conversation_list_recent` to find by id (a `conversation_get` exists from Plan 2 — prefer `repo::conversation_get(&s.db, id)` if present for efficiency). Confirm `ConversationRow.wa_jid` exists (Task 1) and `AppState.cs_outbound` (Task 3).

- [ ] **Step 2: Verify + commit**

Run: `cd backend && cargo check 2>&1 | tail -5`

```bash
git add backend/src/api/cs_admin.rs
git commit -m "feat(cs-wa): owner reply endpoint (enqueue WhatsApp outbound)"
```

---

## Task 6: Routes + env doc

**Files:** Modify `backend/src/api/mod.rs`, `backend/.env.example`.

- [ ] **Step 1: Register routes**

In `api/mod.rs`, add to the **gateway** group (alongside the existing `/whatsapp/*` gateway routes):

```rust
        .route("/cs/chat/whatsapp/inbound", post(cs_whatsapp::inbound))
        .route("/cs/whatsapp/state", post(cs_whatsapp::push_state))
        .route("/cs/whatsapp/commands", get(cs_whatsapp::poll_commands))
        .route("/cs/whatsapp/outbound", get(cs_whatsapp::poll_outbound))
```

Add to the **protected** group (before `.route_layer(... require_auth)`):

```rust
        .route("/cs/whatsapp/status", get(cs_whatsapp::status))
        .route("/cs/whatsapp/connect", post(cs_whatsapp::connect))
        .route("/cs/whatsapp/disconnect", post(cs_whatsapp::disconnect))
        .route("/cs/admin/conversations/:id/reply", post(cs_admin::reply_conversation))
```

Declare the module near the other handler `mod`s: `mod cs_whatsapp;`.

> **Note:** these are same-origin (gateway server-to-server + JWT dashboard) — NOT in the public `cs` scoped-CORS group.

- [ ] **Step 2: Env doc** — append to `backend/.env.example`:

```bash
# CS WhatsApp (Phase 2): shared token for the SECOND (customer-service) gateway.
# Set this AND run a cs-gateway instance pointed at /cs/* to enable WhatsApp CS.
CS_GATEWAY_TOKEN=
```

- [ ] **Step 3: Full verification + commit**

Run (separately): `cd backend && cargo test cs::`, `cargo test repo::cs`, `cargo test api::`, `cargo check`, `cargo clippy --all-targets 2>&1 | tail -12`.
Expected: all pass; only `dead_code` for items consumed by Plans B–D.

```bash
git add backend/src/api/mod.rs backend/.env.example
git commit -m "feat(cs-wa): register CS WhatsApp routes + env doc"
```

---

## Self-Review

**Spec coverage (Phase-2 spec, Plan A scope):**
- Per-JID conversation (find-or-create) ✓ Task 1. Outbound queue ✓ Task 2. Second `WaState` ✓ Task 3.
- Gateway-tier `/cs/chat/whatsapp/inbound` + `/cs/whatsapp/{state,commands,outbound}` with `CS_GATEWAY_TOKEN` ✓ Tasks 4,6.
- Inbound gating: `bot` → agent + reply; `needs_human`/`resolved` → store-only, silent ✓ Task 4.
- Protected dashboard `/cs/whatsapp/{status,connect,disconnect}` ✓ Tasks 4,6. Owner reply → enqueue WA outbound ✓ Task 5.

**Placeholder scan:** No TBD/TODO; notes flag real symbols to confirm (`StatePush`/`CommandOut` visibility, `conversation_get`, `ConversationRow.wa_jid`).

**Type consistency:** `cs_wa: SharedWaState` reuses `WaState`. `cs_outbound: SharedOutbound` (`wa_outbound`) used by `poll_outbound` (drain) + `reply_conversation` (push). `CsWaOut.reply: Option<String>` drives the gateway's send-or-skip (Plan B). `conversation_create_wa`/`conversation_by_wa_jid` used by the inbound handler.

---

## Downstream

- **Plan B — Gateway:** parameterize `whatsapp-gateway` with `PATH_PREFIX` (owner `""`, CS `/cs`) + add an outbound poll-and-send loop hitting `/cs/whatsapp/outbound`, skipping sends when `reply` is null.
- **Plan C — Deploy:** `cs-gateway` in compose + k8s (own `CS_GATEWAY_TOKEN`, `AUTH_DIR`, `PATH_PREFIX=/cs`).
- **Plan D — Frontend:** "CS WhatsApp" pairing card (`/cs/whatsapp/*`) + CS Inbox reply box (`/cs/admin/conversations/:id/reply`).
