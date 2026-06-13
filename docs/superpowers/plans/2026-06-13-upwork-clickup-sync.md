# Upwork Contracts → ClickUp Sync Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On an interval, create a ClickUp project (List) for each new active Upwork contract, idempotently, with a Telegram heads-up.

**Architecture:** Extend the mockable `UpworkClient` with `fetch_contracts`; add a `upwork_project_link` mapping table + repo; add a `upwork/contracts.rs` module (pure helpers + a `run_pass` orchestration over three seams — `UpworkClient`, the existing `ClickUpApi`, and `upwork::jobs::Notifier` — plus `sync_cycle` and a polling loop). Idempotency is claim-after-success: the mapping row is written only after `create_project` succeeds.

**Tech Stack:** Rust, sqlx (SQLite), async-trait, reqwest, the existing `upwork`/`clickup`/`telegram`/`repo` modules. One migration; no frontend.

---

## File Structure

| Path | Create/Modify | Responsibility |
|---|---|---|
| `backend/migrations/0018_upwork_project_link.sql` | Create | Mapping table contract_id → clickup_list_id. |
| `backend/src/repo/upwork_project_link.rs` | Create | `get`/`link`/`list_all` for the mapping. |
| `backend/src/repo/mod.rs` | Modify | `pub mod upwork_project_link;`. |
| `backend/src/upwork/client.rs` | Modify | `Contract` type, `fetch_contracts` trait method + HttpUpwork impl + FakeUpwork extension. |
| `backend/src/upwork/contracts.rs` | Create | Pure `list_name`/`format_created_alert`, `run_pass`, `sync_cycle`, `spawn`. |
| `backend/src/upwork/mod.rs` | Modify | `pub mod contracts;`. |
| `backend/src/main.rs` | Modify | `upwork::contracts::spawn(db.clone());`. |

**Verified facts (do not re-derive):**
- `clickup::client::ClickUpApi` (trait, `Send+Sync`): `create_project(&self, name: &str) -> Result<Project, ClickUpError>` returns `Project { id: String, name: String }`; also `list_projects`, `create_task(&self, list_id, &NewTask)`, `list_tasks(&self, list_id)`, `complete_task(&self, task_id)`. `ClickUpClient::from_env() -> Result<ClickUpClient, ClickUpError>`. Types `NewTask`, `Task` live in `clickup::client`.
- `upwork::client`: `UpworkClient` trait + `ClientError { Http(String), Parse(String) }` + `HttpUpwork::new(String)` (posts to `GRAPHQL_ENDPOINT` with `.bearer_auth`) + `testkit::FakeUpwork` (fields `batch`, `jobs`, `invitations`, `seen_cursor`, `seen_query`; ctors `with`, `with_notifications`).
- `upwork::engine::ensure_access_token(db, cfg, key) -> anyhow::Result<String>` (pub(crate)). `upwork::oauth::OAuthConfig::from_env()`. `upwork::crypto::key_from_env()`.
- `upwork::jobs::{Notifier (trait, async fn send(&self, chat_id: i64, text: &str) -> Result<(), String>), TelegramNotifier { pub client: TelegramClient }}`.
- `repo::upwork_integration::set_status(db, status: &str, last_error: Option<&str>) -> anyhow::Result<()>`. `repo::telegram_link::get(db) -> anyhow::Result<Option<TelegramLinkRow{chat_id: i64,..}>>`. `telegram::client::TelegramClient::new(String)`.
- `main.rs` spawns block has `upwork::jobs::spawn(db.clone());` at the end of the spawn group.
- Migration `0018` is free (highest is `0017_invoices`).

---

## Task 1: Mapping migration + repo

**Files:**
- Create: `backend/migrations/0018_upwork_project_link.sql`
- Create: `backend/src/repo/upwork_project_link.rs`
- Modify: `backend/src/repo/mod.rs`

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0018_upwork_project_link.sql`:

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

- [ ] **Step 2: Declare the repo module**

In `backend/src/repo/mod.rs`, add after `pub mod upwork_integration;`:

```rust
pub mod upwork_project_link;
```

- [ ] **Step 3: Write the repo with tests**

Create `backend/src/repo/upwork_project_link.rs`:

```rust
//! Maps a synced Upwork contract to the ClickUp List created for it (migration
//! 0018). Row presence = "already synced"; written only after a successful
//! ClickUp create (claim-after-success).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct LinkRow {
    pub contract_id: String,
    pub clickup_list_id: String,
    pub name: String,
    pub created_at: String,
}

pub async fn get(db: &Db, contract_id: &str) -> anyhow::Result<Option<LinkRow>> {
    Ok(sqlx::query_as::<_, LinkRow>("SELECT * FROM upwork_project_link WHERE contract_id = ?")
        .bind(contract_id)
        .fetch_optional(db)
        .await?)
}

pub async fn link(db: &Db, contract_id: &str, clickup_list_id: &str, name: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO upwork_project_link (contract_id, clickup_list_id, name, created_at) VALUES (?,?,?,?)",
    )
    .bind(contract_id).bind(clickup_list_id).bind(name).bind(&now)
    .execute(db).await?;
    Ok(())
}

pub async fn list_all(db: &Db) -> anyhow::Result<Vec<LinkRow>> {
    Ok(sqlx::query_as::<_, LinkRow>("SELECT * FROM upwork_project_link ORDER BY created_at")
        .fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn link_then_get_round_trips_and_unknown_is_none() {
        let db = mem_db().await;
        assert!(get(&db, "c1").await.unwrap().is_none());
        link(&db, "c1", "list-9", "Acme — Build API").await.unwrap();
        let row = get(&db, "c1").await.unwrap().unwrap();
        assert_eq!(row.clickup_list_id, "list-9");
        assert_eq!(row.name, "Acme — Build API");
        assert_eq!(list_all(&db).await.unwrap().len(), 1);
    }
}
```

- [ ] **Step 4: Run + commit**

Run: `cd backend && cargo test repo::upwork_project_link::`
Expected: 1 test PASS (proves the migration applies on a fresh in-memory DB).

```bash
git add backend/migrations/0018_upwork_project_link.sql backend/src/repo/upwork_project_link.rs backend/src/repo/mod.rs
git commit -m "feat(cusync): upwork_project_link mapping table + repo"
```

---

## Task 2: `UpworkClient` contract fetch

**Files:**
- Modify: `backend/src/upwork/client.rs`

- [ ] **Step 1: Add the `Contract` type** (after the `Invitation`/`InvitationBatch` types):

```rust
/// An active Upwork contract (engagement).
#[derive(Debug, Clone, PartialEq)]
pub struct Contract {
    pub id: String,
    pub title: String,
    pub client_name: String,
    pub status: String,
}
```

- [ ] **Step 2: Add the trait method** (inside `pub trait UpworkClient`, after `fetch_invitations`):

```rust
    /// Fetch the freelancer's active contracts.
    async fn fetch_contracts(&self) -> Result<Vec<Contract>, ClientError>;
```

- [ ] **Step 3: Implement for `HttpUpwork`** (inside `impl UpworkClient for HttpUpwork`, after `fetch_invitations`):

```rust
    async fn fetch_contracts(&self) -> Result<Vec<Contract>, ClientError> {
        let gql = r#"
            query {
              contracts(status: ACTIVE) {
                edges { node { id title client { name } status } }
              }
            }"#;
        let body = serde_json::json!({ "query": gql });
        let resp = self.http.post(GRAPHQL_ENDPOINT).bearer_auth(&self.access_token).json(&body)
            .send().await.map_err(|e| ClientError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("{}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| ClientError::Parse(e.to_string()))?;
        let edges = v["data"]["contracts"]["edges"]
            .as_array().ok_or_else(|| ClientError::Parse("missing contracts edges".into()))?;
        let mut out = Vec::with_capacity(edges.len());
        for e in edges {
            let n = &e["node"];
            out.push(Contract {
                id: n["id"].as_str().unwrap_or_default().to_string(),
                title: n["title"].as_str().unwrap_or_default().to_string(),
                client_name: n["client"]["name"].as_str().unwrap_or_default().to_string(),
                status: n["status"].as_str().unwrap_or_default().to_string(),
            });
        }
        Ok(out)
    }
```

- [ ] **Step 4: Extend `FakeUpwork`** (in the `testkit` module). Add a `contracts` field, initialize it in `with`, add a `with_contracts` constructor, and implement the new method. Specifically:

Add to the `FakeUpwork` struct (after `seen_query`):
```rust
        pub contracts: Mutex<Vec<Contract>>,
```
In the `with` constructor, add to the `Self { ... }` initializer (after `seen_query: Mutex::new(None),`):
```rust
                contracts: Mutex::new(Vec::new()),
```
After the `with_notifications` constructor, add:
```rust
        pub fn with_contracts(contracts: Vec<Contract>) -> Self {
            let mut f = Self::with(Vec::new(), None);
            *f.contracts.get_mut().unwrap() = contracts;
            f
        }
```
In `impl UpworkClient for FakeUpwork`, add the method:
```rust
        async fn fetch_contracts(&self) -> Result<Vec<Contract>, ClientError> {
            Ok(self.contracts.lock().unwrap().clone())
        }
```

- [ ] **Step 5: Run + commit**

Run: `cd backend && cargo test upwork::client::`
Expected: existing client tests still PASS; compiles. (A `dead_code` warning on `with_contracts`/`contracts` is fine until later tasks use them.)

```bash
git add backend/src/upwork/client.rs
git commit -m "feat(cusync): UpworkClient fetch_contracts"
```

---

## Task 3: `contracts.rs` pure helpers

**Files:**
- Create: `backend/src/upwork/contracts.rs`
- Modify: `backend/src/upwork/mod.rs`

- [ ] **Step 1: Declare the module.** In `backend/src/upwork/mod.rs`, add after `pub mod client;` (keep the list alphabetical-ish; any position compiles):

```rust
pub mod contracts;
```

- [ ] **Step 2: Create `backend/src/upwork/contracts.rs` with the pure helpers + tests:**

```rust
//! Upwork contracts → ClickUp project (List) sync. Pure helpers here; the
//! `run_pass` orchestration and polling loop are added in later tasks. One-way,
//! create-only; idempotent via the `upwork_project_link` mapping.

use crate::upwork::client::Contract;

/// The ClickUp List name for a contract: "{client} — {title}", or just the
/// title when the client name is empty.
pub fn list_name(contract: &Contract) -> String {
    if contract.client_name.trim().is_empty() {
        contract.title.clone()
    } else {
        format!("{} — {}", contract.client_name, contract.title)
    }
}

/// Plain-text Telegram alert announcing a synced contract (no Markdown).
pub fn format_created_alert(name: &str) -> String {
    format!("🗂 New Upwork contract synced to ClickUp: {name}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(client: &str, title: &str) -> Contract {
        Contract {
            id: "c1".into(), title: title.into(), client_name: client.into(), status: "active".into(),
        }
    }

    #[test]
    fn list_name_joins_client_and_title() {
        assert_eq!(list_name(&contract("Acme", "Build API")), "Acme — Build API");
    }

    #[test]
    fn list_name_falls_back_to_title_when_client_empty() {
        assert_eq!(list_name(&contract("   ", "Build API")), "Build API");
    }

    #[test]
    fn created_alert_has_name_no_markdown() {
        let msg = format_created_alert("Acme — Build API");
        assert!(msg.contains("Acme — Build API"));
        assert!(!msg.contains("**"));
    }
}
```

- [ ] **Step 3: Run + commit**

Run: `cd backend && cargo test upwork::contracts::`
Expected: 3 tests PASS.

```bash
git add backend/src/upwork/contracts.rs backend/src/upwork/mod.rs
git commit -m "feat(cusync): contract list-name + alert helpers"
```

---

## Task 4: `contracts.rs` — `run_pass` orchestration

**Files:**
- Modify: `backend/src/upwork/contracts.rs`

- [ ] **Step 1: Add `run_pass`** (above the `#[cfg(test)]` block):

```rust
use crate::clickup::client::ClickUpApi;
use crate::db::Db;
use crate::repo::upwork_project_link;
use crate::upwork::client::UpworkClient;
use crate::upwork::jobs::Notifier;

/// One sync pass against injected seams. For each active contract not yet in the
/// mapping, create a ClickUp List, record the mapping AFTER success, and (when an
/// owner chat is known) send a Telegram heads-up. Returns the number created.
pub async fn run_pass<U: UpworkClient, C: ClickUpApi, N: Notifier>(
    db: &Db,
    upwork: &U,
    clickup: &C,
    notifier: &N,
    owner_chat: Option<i64>,
) -> anyhow::Result<usize> {
    let contracts = match upwork.fetch_contracts().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("fetch contracts failed: {e}");
            return Ok(0);
        }
    };
    let mut created = 0usize;
    for c in &contracts {
        if upwork_project_link::get(db, &c.id).await?.is_some() {
            continue;
        }
        let name = list_name(c);
        match clickup.create_project(&name).await {
            Ok(project) => {
                upwork_project_link::link(db, &c.id, &project.id, &name).await?;
                if let Some(chat) = owner_chat {
                    if let Err(e) = notifier.send(chat, &format_created_alert(&name)).await {
                        tracing::warn!("contract sync notify failed: {e}");
                    }
                }
                created += 1;
            }
            Err(e) => tracing::warn!("create_project for contract {} failed: {e}", c.id),
        }
    }
    Ok(created)
}
```

- [ ] **Step 2: Add the orchestration test** (inside the `tests` module). It defines a minimal `FakeClickUp` and a capturing notifier:

```rust
    use crate::clickup::client::{ClickUpApi, ClickUpError, NewTask, Project, Task};
    use crate::upwork::client::testkit::FakeUpwork;
    use crate::upwork::jobs::Notifier;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeClickUp { created: Mutex<Vec<String>>, next_id: Mutex<u32> }
    #[async_trait::async_trait]
    impl ClickUpApi for FakeClickUp {
        async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError> { Ok(vec![]) }
        async fn create_project(&self, name: &str) -> Result<Project, ClickUpError> {
            let mut id = self.next_id.lock().unwrap();
            *id += 1;
            self.created.lock().unwrap().push(name.to_string());
            Ok(Project { id: format!("list-{}", *id), name: name.to_string() })
        }
        async fn create_task(&self, _list_id: &str, _task: &NewTask) -> Result<String, ClickUpError> { Ok("t".into()) }
        async fn list_tasks(&self, _list_id: &str) -> Result<Vec<Task>, ClickUpError> { Ok(vec![]) }
        async fn complete_task(&self, _task_id: &str) -> Result<(), ClickUpError> { Ok(()) }
    }

    #[derive(Default)]
    struct CapturingNotifier { sent: Mutex<Vec<String>> }
    #[async_trait::async_trait]
    impl Notifier for CapturingNotifier {
        async fn send(&self, _chat: i64, text: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    fn active(id: &str, client: &str, title: &str) -> Contract {
        Contract { id: id.into(), title: title.into(), client_name: client.into(), status: "active".into() }
    }

    #[tokio::test]
    async fn creates_lists_for_new_contracts_then_dedupes() {
        let db = mem_db().await;
        let upwork = FakeUpwork::with_contracts(vec![active("c1", "Acme", "API"), active("c2", "Globex", "App")]);
        let clickup = FakeClickUp::default();
        let notifier = CapturingNotifier::default();

        let n = run_pass(&db, &upwork, &clickup, &notifier, Some(42)).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(clickup.created.lock().unwrap().len(), 2);
        assert_eq!(upwork_project_link::list_all(&db).await.unwrap().len(), 2);
        assert_eq!(notifier.sent.lock().unwrap().len(), 2);
        assert!(notifier.sent.lock().unwrap().iter().any(|m| m.contains("Acme — API")));

        // Second pass: both already linked → nothing created or sent.
        let n2 = run_pass(&db, &upwork, &clickup, &notifier, Some(42)).await.unwrap();
        assert_eq!(n2, 0);
        assert_eq!(clickup.created.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn creates_lists_without_owner_chat_and_skips_notify() {
        let db = mem_db().await;
        let upwork = FakeUpwork::with_contracts(vec![active("c1", "Acme", "API")]);
        let clickup = FakeClickUp::default();
        let notifier = CapturingNotifier::default();

        let n = run_pass(&db, &upwork, &clickup, &notifier, None).await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(upwork_project_link::list_all(&db).await.unwrap().len(), 1);
        assert!(notifier.sent.lock().unwrap().is_empty(), "no owner chat → no notify");
    }
```

- [ ] **Step 3: Run + commit**

Run: `cd backend && cargo test upwork::contracts::`
Expected: all PASS (5 tests). `cargo build` compiles (dead_code on `run_pass` until Task 5 wires it).

```bash
git add backend/src/upwork/contracts.rs
git commit -m "feat(cusync): contract sync run_pass with dedup"
```

---

## Task 5: `sync_cycle`, loop, wiring

**Files:**
- Modify: `backend/src/upwork/contracts.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Add `sync_cycle` + `spawn`** (above the `#[cfg(test)]` block, after `run_pass`):

```rust
use crate::repo::{telegram_link, upwork_integration};
use crate::upwork::client::HttpUpwork;
use crate::upwork::jobs::TelegramNotifier;
use crate::upwork::oauth::OAuthConfig;

const DEFAULT_POLL_SECS: u64 = 3600;

/// One full cycle: ensure token, build the ClickUp + Telegram clients, run a pass.
pub async fn sync_cycle(db: &Db) -> anyhow::Result<usize> {
    let cfg = OAuthConfig::from_env()?;
    let key = crate::upwork::crypto::key_from_env()?;
    let token = match crate::upwork::engine::ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(0);
        }
    };
    let clickup = match crate::clickup::client::ClickUpClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::info!("clickup not configured; contract sync skipped: {e}");
            return Ok(0);
        }
    };
    let tg_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
    let owner_chat = match (telegram_link::get(db).await?, &tg_token) {
        (Some(link), Some(_)) => Some(link.chat_id),
        _ => None,
    };
    let notifier = TelegramNotifier {
        client: crate::telegram::client::TelegramClient::new(tg_token.unwrap_or_default()),
    };
    let upwork = HttpUpwork::new(token);
    run_pass(db, &upwork, &clickup, &notifier, owner_chat).await
}

/// Independent polling loop. No-op when Upwork OAuth env is unset.
pub fn spawn(db: Db) {
    if OAuthConfig::from_env().is_err() {
        tracing::info!("UPWORK_CLIENT_* not set; contract sync disabled");
        return;
    }
    let secs = std::env::var("UPWORK_CONTRACTS_POLL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_POLL_SECS);
    let period = std::time::Duration::from_secs(secs);
    tokio::spawn(async move {
        loop {
            match sync_cycle(&db).await {
                Ok(n) if n > 0 => tracing::info!("upwork contract sync: created {n} ClickUp lists"),
                Ok(_) => {}
                Err(e) => tracing::warn!("upwork contract sync cycle failed: {e:#}"),
            }
            tokio::time::sleep(period).await;
        }
    });
}
```

- [ ] **Step 2: Wire the loop in `main.rs`.** In `backend/src/main.rs`, after the line `upwork::jobs::spawn(db.clone());`, add:

```rust
    upwork::contracts::spawn(db.clone());
```

- [ ] **Step 3: Build + full test + commit**

Run: `cd backend && cargo build`
Expected: clean compile (dead_code on `run_pass` now gone).
Run: `cd backend && cargo test`
Expected: full suite green (ignored tests skipped).

```bash
git add backend/src/upwork/contracts.rs backend/src/main.rs
git commit -m "feat(cusync): sync_cycle + hourly polling loop + wiring"
```

---

## Final verification

- [ ] `cd backend && cargo test` → all green.
- [ ] `cd backend && cargo build` → clean, no new warnings.
- [ ] **Manual smoke (after Upwork API key + `UPWORK_*` + `CLICKUP_API_TOKEN`/`CLICKUP_SPACE_ID` set):** wait one interval (or call `upwork::contracts::sync_cycle`); confirm a ClickUp List appears per active contract, a Telegram heads-up arrives (when linked), and a second cycle creates nothing (dedup). With ClickUp unconfigured, confirm `sync_cycle` returns `Ok(0)` without panicking.

---

## Self-review notes (author)

- **Spec coverage:** migration + repo (Task 1), `fetch_contracts` + fake (Task 2), pure helpers (Task 3), `run_pass` dedup + create-after-success + owner-optional notify (Task 4), `sync_cycle` + loop + token-share + ClickUp-skip + wiring (Task 5). Error handling: token error → set_status; ClickUp unconfigured → Ok(0); fetch error → Ok(0); per-contract create error → log+continue (retried); owner unlinked → create without notify (Task 4 second test). Idempotency: mapping presence (Task 1/4). Out-of-scope (task sync, archival, two-way, UI) absent.
- **Type consistency:** `Contract` (client.rs) → `list_name`/`format_created_alert` (contracts.rs) → `run_pass`/`sync_cycle`/`spawn`. `ClickUpApi::create_project -> Project{id,name}`, `upwork_project_link::{get,link,list_all}`, `Notifier::send`, `TelegramNotifier{client}`, `ensure_access_token`, `set_status`, `telegram_link::get` all match the verified-facts list.
- **No portfolio/cashflow/earnings logic touched.** Reuses `ClickUpApi` + `upwork::jobs::Notifier` seams.
