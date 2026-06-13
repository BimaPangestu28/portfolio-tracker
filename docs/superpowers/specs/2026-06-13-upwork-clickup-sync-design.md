# Upwork Contracts → ClickUp Sync — Design

**Date:** 2026-06-13
**Status:** Approved (design); pending implementation plan
**Scope:** Sub-project 4 (final) of the Upwork integration. Auto-create a ClickUp project (List)
for each new active Upwork contract, idempotently, with a Telegram heads-up.

---

## 1. Purpose

When a new Upwork contract becomes active, create a matching ClickUp List ("project") so the owner
can manage the engagement's tasks there, and send a Telegram notification that the List was created.
One-way (Upwork → ClickUp), create-only.

### Non-goals (v1)
- Syncing tasks inside a List, or any task-level sync.
- Handling contract completion/archival, or updating a List when a contract changes.
- Two-way sync (ClickUp → Upwork).
- Confirm-before-create flows or chat-managed sync (the loop is fully automatic).
- Any web UI.

---

## 2. Context

The pieces already exist:

- **ClickUp seam** — `clickup::client::ClickUpApi` (mockable): `list_projects`, `create_project(name) -> Project { id, name }`, `create_task`, `list_tasks`, `complete_task`. A "project" is a List in the configured Space. `ClickUpClient::from_env()` builds the real client (`CLICKUP_API_TOKEN` + `CLICKUP_SPACE_ID`).
- **Upwork connection** — `upwork/` (sub-project 1): OAuth2, `upwork_integration` token store, mockable `UpworkClient`, `engine::ensure_access_token` (`pub(crate)`), `upwork_integration::set_status`.
- **Telegram delivery** — `upwork::jobs::{Notifier, TelegramNotifier}` (sub-project 2) and `repo::telegram_link::get(db) -> Option<{ chat_id }>`. `TelegramClient::new(token)`.

This feature adds a contract source to `UpworkClient`, a `upwork_project_link` mapping table, a
`repo::upwork_project_link` module, and a `upwork/contracts.rs` module (pure helpers + an
orchestration `sync_cycle` + a polling loop). It mirrors the earnings/jobs engine pattern and reuses
the ClickUp + Telegram seams.

---

## 3. Components

### 3.1 Migration `backend/migrations/0018_upwork_project_link.sql`

```sql
-- Maps each synced Upwork contract to the ClickUp List created for it. A row's
-- existence means "already synced" — the idempotency guarantee for contract sync.
CREATE TABLE upwork_project_link (
  contract_id TEXT PRIMARY KEY,
  clickup_list_id TEXT NOT NULL,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL
);
```
(`0018` confirmed free vs origin/main — highest is `0017_invoices`.)

### 3.2 `UpworkClient` extension — `backend/src/upwork/client.rs`

```rust
async fn fetch_contracts(&self) -> Result<Vec<Contract>, ClientError>;
```
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub id: String,
    pub title: String,
    pub client_name: String,
    pub status: String,   // e.g. "active"
}
```
`HttpUpwork` issues the GraphQL contracts query (field paths validated only by a gated live smoke
test, as with earnings). `FakeUpwork` gains a `contracts` vec + a `with_contracts(...)` constructor.

### 3.3 `repo/upwork_project_link.rs`

```rust
pub struct LinkRow { pub contract_id: String, pub clickup_list_id: String, pub name: String, pub created_at: String }

pub async fn get(db: &Db, contract_id: &str) -> anyhow::Result<Option<LinkRow>>;
pub async fn link(db: &Db, contract_id: &str, clickup_list_id: &str, name: &str) -> anyhow::Result<()>;  // INSERT
pub async fn list_all(db: &Db) -> anyhow::Result<Vec<LinkRow>>;
```

### 3.4 `upwork/contracts.rs` — pure helpers + orchestration

**Pure (unit-testable):**
- `list_name(contract: &Contract) -> String` — the ClickUp List name, `"{client_name} — {title}"` (falls back to just `title` when `client_name` is empty).
- `format_created_alert(name: &str) -> String` — plain-text Telegram message (no Markdown), e.g. `"🗂 New Upwork contract synced to ClickUp: {name}"`.

**Orchestration `run_pass<U: UpworkClient, C: ClickUpApi, N: Notifier>(db, upwork, clickup, notifier, owner_chat: Option<i64>) -> anyhow::Result<usize>`:**
1. `upwork.fetch_contracts()` (on error → `tracing::warn!` + return `Ok(0)`).
2. For each contract where `upwork_project_link::get(db, &c.id)?.is_none()`:
   - `clickup.create_project(&list_name(&c))` → on error log + continue (not linked → retried next cycle).
   - On success: `upwork_project_link::link(db, &c.id, &project.id, &name)`.
   - If `owner_chat` is `Some(chat)`: `notifier.send(chat, &format_created_alert(&name))` (send error logged, continue).
   - Increment created count.
3. Return the count.

> Claim-after-success: the mapping row is written only after `create_project` succeeds, so a
> transient ClickUp failure leaves the contract unsynced and it is retried next cycle (the List is
> never lost or duplicated).

**`async fn sync_cycle(db: &Db) -> anyhow::Result<usize>`:**
1. `OAuthConfig::from_env()?`; `key_from_env()?`; `ensure_access_token` — on error `set_status("error", ...)` + return `Ok(0)`.
2. `ClickUpClient::from_env()` — on error (ClickUp not configured) log + return `Ok(0)`.
3. `owner_chat`: from `telegram_link::get(db)` + `TELEGRAM_BOT_TOKEN`; build `TelegramNotifier` when both present, else a no-op send is avoided by passing `owner_chat = None`.
4. Build `HttpUpwork`, call `run_pass(db, &upwork, &clickup, &notifier, owner_chat)`.

**`pub fn spawn(db: Db)`** — loop on `UPWORK_CONTRACTS_POLL_SECS` (default 3600); no-op when `OAuthConfig::from_env().is_err()`. Mirrors `upwork::jobs::spawn` / google.

### 3.5 Wiring + env — `backend/src/main.rs`

`upwork::contracts::spawn(db.clone());` next to the other spawns. Env: `UPWORK_CONTRACTS_POLL_SECS` (default `3600`). ClickUp env (`CLICKUP_API_TOKEN`/`CLICKUP_SPACE_ID`) already exist.

---

## 4. Data flow

```
loop (every UPWORK_CONTRACTS_POLL_SECS):
  token = ensure_access_token();  upwork = HttpUpwork(token)
  clickup = ClickUpClient::from_env()  // skip cycle if unconfigured
  owner = telegram_link::get() + TELEGRAM_BOT_TOKEN   // Option<chat_id>
  for c in upwork.fetch_contracts():
      if upwork_project_link::get(c.id).is_none():
          list = clickup.create_project(list_name(c))      // on fail: log, continue (retry next cycle)
          upwork_project_link::link(c.id, list.id, name)   // record AFTER success
          if owner: notifier.send(owner, created_alert(name))
```

No portfolio/cashflow writes. New migration `0018` only.

---

## 5. Error handling

| Condition | Behavior |
|---|---|
| Upwork token error | `set_status("error", ...)`; return `Ok(0)`. |
| ClickUp not configured / `from_env` error | Log + return `Ok(0)` (no Lists created). |
| `fetch_contracts` error | Log + return `Ok(0)`. |
| Per-contract `create_project` error | Log + continue; contract left unlinked → retried next cycle. |
| Owner not linked / no `TELEGRAM_BOT_TOKEN` | Lists still created; notification skipped. |
| Re-run / overlap | Mapping row presence dedupes; a contract is synced at most once. |

---

## 6. Testing (TDD)

- **`list_name`** — `"Acme — Build API"`; empty client → `"Build API"`.
- **`format_created_alert`** — contains the name; no Markdown.
- **`run_pass`** with `FakeUpwork` (contracts) + a local `FakeClickUp` (records created Lists, returns ids) + a capturing notifier + in-memory DB: a List is created per new contract, the mapping row is recorded, a **second run creates nothing** (dedup), the notification is sent; with `owner_chat = None`, Lists are still created and no send occurs.
- **`repo::upwork_project_link`** — `link` then `get` round-trips; `get` is `None` for an unknown contract.
- **Gated live smoke test** — behind `UPWORK_SMOKE_DB`, skipped by default.

---

## 7. Out of scope (restated)

Task-level sync, contract completion/archival handling, List updates on contract change, two-way
sync, confirm-before-create, web UI. The `upwork_project_link` table records `clickup_list_id`,
leaving room for a future task-sync sub-project to attach tasks to the right List.
