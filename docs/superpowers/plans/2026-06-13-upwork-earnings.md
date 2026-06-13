# Upwork Earnings → Cashflow Income — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ingest the user's Upwork earnings into the existing `cashflow` ledger as income (USD), idempotently, without touching the portfolio domain.

**Architecture:** A new `backend/src/upwork/` module mirrors the established `backend/src/google/` integration pattern — OAuth2 + at-rest token encryption + a **mockable client trait** + a **pure reconciler** + an executor engine + a single-row encrypted token table. Earnings land in `cashflow` (`direction='in'`) tagged with `source='upwork'` + `external_ref`, made idempotent by a unique index. The mockable client lets the whole module be built and tested now, before the Upwork API key is approved.

**Tech Stack:** Rust, axum, sqlx (SQLite), reqwest, async-trait, aes-gcm, jsonwebtoken, chrono. Tests use `sqlite::memory:` in-memory DBs, mirroring existing repo/engine tests.

---

## File Structure

| Path | Create/Modify | Responsibility |
|---|---|---|
| `backend/migrations/0015_upwork.sql` | Create | `upwork_integration` table; `cashflow.source` + `cashflow.external_ref` columns; unique index. |
| `backend/src/repo/cashflow.rs` | Modify | Add `source`/`external_ref` to `CashflowRow`; add `insert_sourced()` (idempotent). |
| `backend/src/repo/cashflow_categories.rs` | Modify | Add `ensure_by_name()`. |
| `backend/src/repo/upwork_integration.rs` | Create | Single-row token/cursor/status persistence. |
| `backend/src/repo/mod.rs` | Modify | Declare `upwork_integration`. |
| `backend/src/upwork/mod.rs` | Create | Module decls + `UpworkTransaction` type. |
| `backend/src/upwork/crypto.rs` | Create | `UPWORK_TOKEN_ENC_KEY` loader; reuses `google::crypto` encrypt/decrypt. |
| `backend/src/upwork/oauth.rs` | Create | Upwork OAuth2 config, consent URL, code exchange, refresh. |
| `backend/src/upwork/client.rs` | Create | `UpworkClient` trait + `HttpUpwork` GraphQL impl + `TransactionBatch`. |
| `backend/src/upwork/sync.rs` | Create | Pure planner: `UpworkTransaction[]` → `PlannedEarning[]`. |
| `backend/src/upwork/engine.rs` | Create | Executor: token refresh + fetch + plan + idempotent insert + cursor advance. |
| `backend/src/api/upwork.rs` | Create | Routes: start/callback/status/sync/disconnect. |
| `backend/src/api/mod.rs` | Modify | Declare + register routes. |
| `backend/src/main.rs` | Modify | `mod upwork;`. |
| `docker-compose.yml`, `docker-compose.prod.yml`, `k8s/*`, `.env.production.example` | Modify | `UPWORK_*` env wiring. |
| `frontend` connectors card | Modify | "Connect Upwork" card (additive). |

**Conventions to follow (verified in repo):**
- Money/amounts are **strings**; validate with `crate::repo::dec(&s)?`.
- In-memory test DB: `crate::db::connect("sqlite::memory:").await.unwrap()`.
- Migrations auto-run via `sqlx::migrate!("./migrations")` in `db::connect`.
- Single-row integration tables use `id = 1`.

---

## Task 1: Migration — schema for Upwork integration + sourced cashflow

**Files:**
- Create: `backend/migrations/0015_upwork.sql`

> Pre-check: `0015` is free vs `origin/main` (highest is `0014`). Re-confirm before merging (project memory: migration-number collisions).

- [ ] **Step 1: Write the migration**

```sql
-- backend/migrations/0015_upwork.sql

-- Single-row Upwork connection (mirrors google_integration). Tokens are stored
-- already-encrypted by the caller (see upwork::crypto). id is always 1.
CREATE TABLE upwork_integration (
  id INTEGER PRIMARY KEY,
  access_token TEXT NOT NULL,
  refresh_token TEXT NOT NULL,
  expiry TEXT NOT NULL,
  scope TEXT NOT NULL,
  earnings_cursor TEXT,
  status TEXT NOT NULL DEFAULT 'connected',
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- Provenance + idempotency for cashflow rows. Existing/manual rows leave these
-- NULL. SQLite treats NULLs as distinct in a UNIQUE index, so manual entries
-- are unconstrained; only (source, external_ref) pairs from connectors are
-- deduplicated.
ALTER TABLE cashflow ADD COLUMN source TEXT;
ALTER TABLE cashflow ADD COLUMN external_ref TEXT;
CREATE UNIQUE INDEX idx_cashflow_source_ref ON cashflow(source, external_ref);
```

- [ ] **Step 2: Verify it applies (migrations run on connect)**

Run: `cd backend && cargo test repo::cashflow_categories::tests::create_and_list -- --nocapture`
Expected: PASS (proves migration `0015` applies cleanly on a fresh in-memory DB).

- [ ] **Step 3: Commit**

```bash
git add backend/migrations/0015_upwork.sql
git commit -m "feat(upwork): migration for integration table + sourced cashflow"
```

---

## Task 2: Cashflow repo — idempotent sourced insert

**Files:**
- Modify: `backend/src/repo/cashflow.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `backend/src/repo/cashflow.rs`:

```rust
    #[tokio::test]
    async fn insert_sourced_is_idempotent_on_external_ref() {
        let db = mem_db().await;
        let c = NewCashflow {
            account_id: None, occurred_on: "2026-06-10".into(), direction: "in".into(),
            amount: "500.00".into(), currency: "USD".into(), category_id: None,
            note: Some("Acme contract".into()),
        };
        let first = insert_sourced(&db, &c, "upwork", "txn-1").await.unwrap();
        let second = insert_sourced(&db, &c, "upwork", "txn-1").await.unwrap();
        assert!(first, "first insert should create a row");
        assert!(!second, "duplicate external_ref must be a no-op");
        assert_eq!(list_all(&db).await.unwrap().len(), 1);
        let row = &list_all(&db).await.unwrap()[0];
        assert_eq!(row.source.as_deref(), Some("upwork"));
        assert_eq!(row.external_ref.as_deref(), Some("txn-1"));
        assert_eq!(row.direction, "in");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test repo::cashflow::tests::insert_sourced_is_idempotent_on_external_ref`
Expected: FAIL — `insert_sourced` not found, and `CashflowRow` has no `source`/`external_ref`.

- [ ] **Step 3: Extend `CashflowRow` with the new columns**

In `backend/src/repo/cashflow.rs`, add two fields to the `CashflowRow` struct (after `note`):

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CashflowRow {
    pub id: i64,
    pub account_id: Option<i64>,
    pub occurred_on: String,
    pub direction: String,
    pub amount: String,
    pub currency: String,
    pub category_id: Option<i64>,
    pub note: Option<String>,
    pub source: Option<String>,
    pub external_ref: Option<String>,
    pub created_at: String,
}
```

- [ ] **Step 4: Add `insert_sourced`**

In `backend/src/repo/cashflow.rs`, after the existing `create` function:

```rust
/// Insert a cashflow row tagged with provenance, deduplicated by
/// (source, external_ref). Returns true if a new row was inserted, false if it
/// already existed (idempotent re-sync). Mirrors `create`'s validation.
pub async fn insert_sourced(
    db: &Db,
    c: &NewCashflow,
    source: &str,
    external_ref: &str,
) -> anyhow::Result<bool> {
    if c.direction != "in" && c.direction != "out" {
        anyhow::bail!("direction must be 'in' or 'out', got '{}'", c.direction);
    }
    crate::repo::dec(&c.amount)?;
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query(
        "INSERT INTO cashflow
            (account_id, occurred_on, direction, amount, currency, category_id, note, source, external_ref, created_at)
         VALUES (?,?,?,?,?,?,?,?,?,?)
         ON CONFLICT(source, external_ref) DO NOTHING",
    )
    .bind(c.account_id).bind(&c.occurred_on).bind(&c.direction)
    .bind(&c.amount).bind(&c.currency).bind(c.category_id)
    .bind(&c.note).bind(source).bind(external_ref).bind(&now)
    .execute(db).await?;
    Ok(res.rows_affected() > 0)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test repo::cashflow::`
Expected: PASS (new test + existing cashflow tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/repo/cashflow.rs
git commit -m "feat(upwork): idempotent sourced cashflow insert"
```

---

## Task 3: Cashflow categories — ensure-by-name

**Files:**
- Modify: `backend/src/repo/cashflow_categories.rs`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `backend/src/repo/cashflow_categories.rs`:

```rust
    #[tokio::test]
    async fn ensure_by_name_is_idempotent() {
        let db = mem_db().await;
        let a = ensure_by_name(&db, "Upwork", "income").await.unwrap();
        let b = ensure_by_name(&db, "Upwork", "income").await.unwrap();
        assert_eq!(a.id, b.id, "second call must reuse the same category");
        assert_eq!(a.kind, "income");
        assert_eq!(list(&db).await.unwrap().len(), 1);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test repo::cashflow_categories::tests::ensure_by_name_is_idempotent`
Expected: FAIL — `ensure_by_name` not found.

- [ ] **Step 3: Implement `ensure_by_name`**

In `backend/src/repo/cashflow_categories.rs`, after `list`:

```rust
/// Return the category with this name, creating it if absent. Used by
/// connectors that must attach income to a stable category.
pub async fn ensure_by_name(db: &Db, name: &str, kind: &str) -> anyhow::Result<CashflowCategoryRow> {
    if let Some(row) = sqlx::query_as::<_, CashflowCategoryRow>(
        "SELECT * FROM cashflow_category WHERE name = ?",
    )
    .bind(name)
    .fetch_optional(db)
    .await?
    {
        return Ok(row);
    }
    create(db, &NewCashflowCategory {
        name: name.to_string(),
        kind: kind.to_string(),
        monthly_budget: None,
        color: None,
    })
    .await
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test repo::cashflow_categories::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cashflow_categories.rs
git commit -m "feat(upwork): ensure cashflow category by name"
```

---

## Task 4: `upwork_integration` repo

**Files:**
- Create: `backend/src/repo/upwork_integration.rs`
- Modify: `backend/src/repo/mod.rs`

- [ ] **Step 1: Declare the module**

In `backend/src/repo/mod.rs`, add to the `pub mod` list (near `google_integration`):

```rust
pub mod upwork_integration;
```

- [ ] **Step 2: Write the repo with its tests**

Create `backend/src/repo/upwork_integration.rs` (mirrors `google_integration.rs`):

```rust
//! Single-row persistence for the Upwork connection (see migration 0015).
//! Tokens are stored already-encrypted by the caller (see upwork::crypto).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct IntegrationRow {
    pub id: i64,
    #[serde(skip)]
    pub access_token: String,
    #[serde(skip)]
    pub refresh_token: String,
    pub expiry: String,
    pub scope: String,
    pub earnings_cursor: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

pub async fn get(db: &Db) -> anyhow::Result<Option<IntegrationRow>> {
    Ok(sqlx::query_as::<_, IntegrationRow>("SELECT * FROM upwork_integration WHERE id = 1")
        .fetch_optional(db)
        .await?)
}

/// Insert or replace the single connection row, resetting status to 'connected'.
pub async fn upsert(
    db: &Db,
    enc_access_token: &str,
    enc_refresh_token: &str,
    expiry: &str,
    scope: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO upwork_integration
            (id, access_token, refresh_token, expiry, scope, status, created_at, updated_at)
         VALUES (1, ?, ?, ?, ?, 'connected', ?, ?)
         ON CONFLICT(id) DO UPDATE SET
            access_token = excluded.access_token,
            refresh_token = excluded.refresh_token,
            expiry = excluded.expiry,
            scope = excluded.scope,
            status = 'connected',
            last_error = NULL,
            updated_at = excluded.updated_at",
    )
    .bind(enc_access_token).bind(enc_refresh_token).bind(expiry).bind(scope)
    .bind(&now).bind(&now)
    .execute(db).await?;
    Ok(())
}

/// Persist a refreshed access token + expiry without touching the refresh token.
pub async fn update_access(db: &Db, enc_access_token: &str, expiry: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE upwork_integration SET access_token = ?, expiry = ?, updated_at = ? WHERE id = 1")
        .bind(enc_access_token).bind(expiry).bind(&now)
        .execute(db).await?;
    Ok(())
}

pub async fn set_cursor(db: &Db, cursor: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE upwork_integration SET earnings_cursor = ? WHERE id = 1")
        .bind(cursor)
        .execute(db).await?;
    Ok(())
}

pub async fn set_status(db: &Db, status: &str, last_error: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE upwork_integration SET status = ?, last_error = ?, updated_at = ? WHERE id = 1")
        .bind(status).bind(last_error).bind(&now)
        .execute(db).await?;
    Ok(())
}

pub async fn delete(db: &Db) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM upwork_integration WHERE id = 1").execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn upsert_get_cursor_status_delete() {
        let db = mem_db().await;
        assert!(get(&db).await.unwrap().is_none());
        upsert(&db, "enc-a", "enc-r", "2026-06-12T10:00:00+00:00", "scope").await.unwrap();
        upsert(&db, "enc-a2", "enc-r2", "2026-06-12T11:00:00+00:00", "scope").await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.access_token, "enc-a2");
        assert_eq!(row.status, "connected");

        set_cursor(&db, "cur-9").await.unwrap();
        set_status(&db, "error", Some("boom")).await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.earnings_cursor.as_deref(), Some("cur-9"));
        assert_eq!(row.status, "error");
        assert_eq!(row.last_error.as_deref(), Some("boom"));

        delete(&db).await.unwrap();
        assert!(get(&db).await.unwrap().is_none());
    }
}
```

- [ ] **Step 3: Run tests to verify they pass**

Run: `cd backend && cargo test repo::upwork_integration::`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/src/repo/upwork_integration.rs backend/src/repo/mod.rs
git commit -m "feat(upwork): single-row integration repo"
```

---

## Task 5: `upwork` module scaffold + crypto

**Files:**
- Create: `backend/src/upwork/mod.rs`
- Create: `backend/src/upwork/crypto.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Declare the module in main.rs**

In `backend/src/main.rs`, add to the `mod` list (after `mod telegram;` or alphabetically near it):

```rust
mod upwork;
```

- [ ] **Step 2: Create the module root with the shared type**

Create `backend/src/upwork/mod.rs`:

```rust
//! Upwork earnings integration: OAuth2, a mockable transaction client, a pure
//! reconciler, and the sync engine. Earnings land in the `cashflow` ledger as
//! income; the portfolio domain is never touched. Mirrors the `google` module.

pub mod client;
pub mod crypto;
pub mod engine;
pub mod oauth;
pub mod sync;

/// One Upwork financial transaction, decoded from the GraphQL API or a fixture.
#[derive(Debug, Clone, PartialEq)]
pub struct UpworkTransaction {
    /// Upwork transaction reference — the idempotency key.
    pub external_id: String,
    /// Date the transaction occurred (YYYY-MM-DD or rfc3339).
    pub date: String,
    /// Raw Upwork type/description, used to classify earning vs fee/withdrawal.
    pub kind: String,
    /// Money as a string (never parsed for storage).
    pub amount: String,
    /// Currency code, e.g. "USD".
    pub currency: String,
    /// Contract / project name, if any.
    pub contract: Option<String>,
}
```

- [ ] **Step 3: Write the crypto loader with its test**

Create `backend/src/upwork/crypto.rs`:

```rust
//! AES-256-GCM token encryption for Upwork, reusing the google::crypto
//! primitives. Only the key source differs (UPWORK_TOKEN_ENC_KEY). Fail closed.

pub use crate::google::crypto::{decrypt, encrypt};

/// Read + parse the base64 32-byte key from UPWORK_TOKEN_ENC_KEY. Err when unset
/// or malformed (fail closed: callers treat this as "cannot connect").
pub fn key_from_env() -> anyhow::Result<[u8; 32]> {
    let b64 = std::env::var("UPWORK_TOKEN_ENC_KEY")
        .map_err(|_| anyhow::anyhow!("UPWORK_TOKEN_ENC_KEY is not set"))?;
    let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64.trim())
        .map_err(|_| anyhow::anyhow!("UPWORK_TOKEN_ENC_KEY is not valid base64"))?;
    raw.try_into()
        .map_err(|_| anyhow::anyhow!("UPWORK_TOKEN_ENC_KEY must decode to exactly 32 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trips() {
        let k = [9u8; 32];
        let ct = encrypt("oauth-secret", &k).unwrap();
        assert_ne!(ct, "oauth-secret");
        assert_eq!(decrypt(&ct, &k).unwrap(), "oauth-secret");
    }

    #[test]
    fn key_from_env_requires_var() {
        std::env::remove_var("UPWORK_TOKEN_ENC_KEY");
        assert!(key_from_env().is_err());
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test upwork::crypto::`
Expected: PASS.

> Note: the build will fail until `client.rs`, `engine.rs`, `oauth.rs`, `sync.rs` exist (declared in `mod.rs`). Create empty stubs to compile incrementally **only if** your runner compiles the whole crate per task; otherwise proceed — the next tasks create them. To unblock compilation now, create one-line stubs:
> - `backend/src/upwork/oauth.rs`, `sync.rs`, `client.rs`, `engine.rs` each containing `// filled in a later task` — then replace in their tasks.

- [ ] **Step 5: Commit**

```bash
git add backend/src/main.rs backend/src/upwork/mod.rs backend/src/upwork/crypto.rs
git commit -m "feat(upwork): module scaffold + token crypto"
```

---

## Task 6: OAuth2 (`upwork/oauth.rs`)

**Files:**
- Create/replace: `backend/src/upwork/oauth.rs`

Reuses the generic signed-`state` helpers and `TokenResponse`/`expiry_from_now` from `google::oauth` (DRY); only Upwork's endpoints and config differ.

- [ ] **Step 1: Write `oauth.rs` with its test**

```rust
//! Upwork OAuth2: env config, consent URL, and code exchange / refresh. The
//! signed `state` CSRF helpers and the token-response shape are reused from
//! `google::oauth`. Upwork scope is governed by the API key's configured
//! permissions, so no scope param is sent in the consent URL.

pub use crate::google::oauth::{expiry_from_now, TokenResponse};

const AUTHORIZE_ENDPOINT: &str = "https://www.upwork.com/ab/account-security/oauth2/authorize";
const TOKEN_ENDPOINT: &str = "https://www.upwork.com/api/v3/oauth2/token";

pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OAuthConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client_id: std::env::var("UPWORK_CLIENT_ID")
                .map_err(|_| anyhow::anyhow!("UPWORK_CLIENT_ID is not set"))?,
            client_secret: std::env::var("UPWORK_CLIENT_SECRET")
                .map_err(|_| anyhow::anyhow!("UPWORK_CLIENT_SECRET is not set"))?,
            redirect_uri: std::env::var("UPWORK_REDIRECT_URI")
                .map_err(|_| anyhow::anyhow!("UPWORK_REDIRECT_URI is not set"))?,
        })
    }
}

fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the Upwork consent URL. `state` is the signed CSRF token.
pub fn consent_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{AUTHORIZE_ENDPOINT}?response_type=code&client_id={}&redirect_uri={}&state={}",
        enc(client_id), enc(redirect_uri), enc(state)
    )
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(cfg: &OAuthConfig, code: &str) -> anyhow::Result<TokenResponse> {
    let resp = reqwest::Client::new()
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("redirect_uri", cfg.redirect_uri.as_str()),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("upwork token exchange failed: {} {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

/// Refresh the access token using the stored refresh token.
pub async fn refresh_access(cfg: &OAuthConfig, refresh_token: &str) -> anyhow::Result<TokenResponse> {
    let resp = reqwest::Client::new()
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!("upwork token refresh failed: {} {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_url_has_required_params() {
        let url = consent_url("cid-1", "https://app/api/upwork/oauth/callback", "STATE");
        assert!(url.starts_with("https://www.upwork.com/ab/account-security/oauth2/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=cid-1"));
        assert!(url.contains("state=STATE"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp%2Fapi%2Fupwork%2Foauth%2Fcallback"));
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cd backend && cargo test upwork::oauth::tests::consent_url_has_required_params`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/src/upwork/oauth.rs
git commit -m "feat(upwork): oauth2 consent url + token exchange/refresh"
```

---

## Task 7: Pure reconciler (`upwork/sync.rs`)

**Files:**
- Create/replace: `backend/src/upwork/sync.rs`

- [ ] **Step 1: Write `sync.rs` with its tests**

```rust
//! Pure reconciliation: turn a batch of Upwork transactions into the cashflow
//! income rows to insert. No DB, no network — fully unit-testable. Only
//! earning-type transactions are kept (Approach A); fees/withdrawals/refunds
//! are dropped.

use crate::repo::cashflow::NewCashflow;
use crate::upwork::UpworkTransaction;

/// A cashflow row to insert, paired with the Upwork transaction id for
/// idempotent persistence.
#[derive(Debug, PartialEq)]
pub struct PlannedEarning {
    pub external_ref: String,
    pub cashflow: NewCashflow,
}

/// Classify a transaction as earned income. Earnings are fixed-price/milestone
/// releases, hourly charges, and bonuses; fees, withdrawals, and refunds are
/// excluded.
pub fn is_earning(kind: &str) -> bool {
    let k = kind.to_ascii_lowercase();
    if k.contains("refund") || k.contains("withdraw") || k.contains("fee") {
        return false;
    }
    ["fixed", "milestone", "hourly", "bonus"].iter().any(|p| k.contains(p))
}

/// Map earning transactions to cashflow income rows under `category_id`.
pub fn plan_earnings(txns: &[UpworkTransaction], category_id: i64) -> Vec<PlannedEarning> {
    txns.iter()
        .filter(|t| is_earning(&t.kind))
        .map(|t| PlannedEarning {
            external_ref: t.external_id.clone(),
            cashflow: NewCashflow {
                account_id: None,
                occurred_on: t.date.clone(),
                direction: "in".to_string(),
                amount: t.amount.clone(),
                currency: t.currency.clone(),
                category_id: Some(category_id),
                note: t.contract.clone(),
            },
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn txn(id: &str, kind: &str, amount: &str) -> UpworkTransaction {
        UpworkTransaction {
            external_id: id.into(), date: "2026-06-10".into(), kind: kind.into(),
            amount: amount.into(), currency: "USD".into(), contract: Some("Acme".into()),
        }
    }

    #[test]
    fn keeps_only_earnings() {
        let batch = vec![
            txn("t1", "Fixed Price milestone", "500.00"),
            txn("t2", "Hourly", "120.00"),
            txn("t3", "Bonus", "50.00"),
            txn("t4", "Service Fee", "-50.00"),
            txn("t5", "Withdrawal", "-400.00"),
            txn("t6", "Refund", "-25.00"),
        ];
        let planned = plan_earnings(&batch, 7);
        let refs: Vec<&str> = planned.iter().map(|p| p.external_ref.as_str()).collect();
        assert_eq!(refs, vec!["t1", "t2", "t3"]);
    }

    #[test]
    fn maps_fields_to_income_cashflow() {
        let planned = plan_earnings(&[txn("t1", "Hourly", "120.00")], 7);
        let p = &planned[0];
        assert_eq!(p.external_ref, "t1");
        assert_eq!(p.cashflow.direction, "in");
        assert_eq!(p.cashflow.amount, "120.00");
        assert_eq!(p.cashflow.currency, "USD");
        assert_eq!(p.cashflow.category_id, Some(7));
        assert_eq!(p.cashflow.note.as_deref(), Some("Acme"));
        assert_eq!(p.cashflow.occurred_on, "2026-06-10");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd backend && cargo test upwork::sync::`
Expected: PASS.

> Note: `NewCashflow` must derive `PartialEq` for `PlannedEarning`'s derive to compile **only if** you compare whole `PlannedEarning` values. The tests above compare fields, not whole structs, so no extra derive is needed. If a future test compares `PlannedEarning` directly, add `#[derive(PartialEq)]` to `NewCashflow` in `repo/cashflow.rs`.

- [ ] **Step 3: Commit**

```bash
git add backend/src/upwork/sync.rs
git commit -m "feat(upwork): pure earnings reconciler (approach A)"
```

---

## Task 8: Mockable client (`upwork/client.rs`)

**Files:**
- Create/replace: `backend/src/upwork/client.rs`

- [ ] **Step 1: Write `client.rs` with a fake-backed test**

```rust
//! The Upwork transaction source. `UpworkClient` is the seam: the real
//! `HttpUpwork` calls the GraphQL API; tests and pre-approval development use a
//! fake. Returns a cursor for incremental paging.

use crate::upwork::UpworkTransaction;
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("http error: {0}")] Http(String),
    #[error("parse error: {0}")] Parse(String),
}

#[derive(Debug, Default)]
pub struct TransactionBatch {
    pub txns: Vec<UpworkTransaction>,
    pub next_cursor: Option<String>,
}

#[async_trait]
pub trait UpworkClient: Send + Sync {
    /// Fetch transactions newer than `cursor` (None = from the beginning).
    async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError>;
}

const GRAPHQL_ENDPOINT: &str = "https://api.upwork.com/graphql";

/// Live client. The GraphQL query/field mapping is exercised by the gated live
/// smoke test in `engine.rs`; adjust field paths once the approved schema is
/// confirmed.
pub struct HttpUpwork {
    access_token: String,
    http: reqwest::Client,
}

impl HttpUpwork {
    pub fn new(access_token: String) -> Self {
        Self { access_token, http: reqwest::Client::new() }
    }
}

#[async_trait]
impl UpworkClient for HttpUpwork {
    async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError> {
        // Minimal financial-reporting query. `after` drives incremental paging.
        let query = r#"
            query($after: String) {
              transactionHistory(after: $after) {
                edges {
                  cursor
                  node { reference dateTime type amount { rawValue currency } contractTitle }
                }
                pageInfo { endCursor hasNextPage }
              }
            }"#;
        let body = serde_json::json!({ "query": query, "variables": { "after": cursor } });
        let resp = self.http
            .post(GRAPHQL_ENDPOINT)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("{}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| ClientError::Parse(e.to_string()))?;
        let edges = v["data"]["transactionHistory"]["edges"]
            .as_array()
            .ok_or_else(|| ClientError::Parse("missing transactionHistory.edges".into()))?;
        let mut txns = Vec::with_capacity(edges.len());
        for e in edges {
            let node = &e["node"];
            txns.push(UpworkTransaction {
                external_id: node["reference"].as_str().unwrap_or_default().to_string(),
                date: node["dateTime"].as_str().unwrap_or_default().to_string(),
                kind: node["type"].as_str().unwrap_or_default().to_string(),
                amount: node["amount"]["rawValue"].as_str().unwrap_or("0").to_string(),
                currency: node["amount"]["currency"].as_str().unwrap_or("USD").to_string(),
                contract: node["contractTitle"].as_str().map(|s| s.to_string()),
            });
        }
        let next_cursor = v["data"]["transactionHistory"]["pageInfo"]["endCursor"]
            .as_str()
            .map(|s| s.to_string());
        Ok(TransactionBatch { txns, next_cursor })
    }
}

#[cfg(test)]
pub mod testkit {
    use super::*;
    use std::sync::Mutex;

    /// In-memory client returning a preset batch; records the cursor it was called with.
    pub struct FakeUpwork {
        pub batch: Mutex<TransactionBatch>,
        pub seen_cursor: Mutex<Option<String>>,
    }
    impl FakeUpwork {
        pub fn with(txns: Vec<UpworkTransaction>, next_cursor: Option<String>) -> Self {
            Self { batch: Mutex::new(TransactionBatch { txns, next_cursor }), seen_cursor: Mutex::new(None) }
        }
    }
    #[async_trait]
    impl UpworkClient for FakeUpwork {
        async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError> {
            *self.seen_cursor.lock().unwrap() = cursor.map(|c| c.to_string());
            let b = self.batch.lock().unwrap();
            Ok(TransactionBatch { txns: b.txns.clone(), next_cursor: b.next_cursor.clone() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::FakeUpwork;
    use super::*;

    #[tokio::test]
    async fn fake_returns_preset_batch_and_records_cursor() {
        let fake = FakeUpwork::with(
            vec![UpworkTransaction {
                external_id: "t1".into(), date: "2026-06-10".into(), kind: "Hourly".into(),
                amount: "120.00".into(), currency: "USD".into(), contract: None,
            }],
            Some("cur-2".into()),
        );
        let batch = fake.fetch_transactions(Some("cur-1")).await.unwrap();
        assert_eq!(batch.txns.len(), 1);
        assert_eq!(batch.next_cursor.as_deref(), Some("cur-2"));
        assert_eq!(fake.seen_cursor.lock().unwrap().as_deref(), Some("cur-1"));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd backend && cargo test upwork::client::`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/src/upwork/client.rs
git commit -m "feat(upwork): mockable transaction client + http graphql impl"
```

---

## Task 9: Engine (`upwork/engine.rs`)

**Files:**
- Create/replace: `backend/src/upwork/engine.rs`

- [ ] **Step 1: Write `engine.rs` with its tests**

```rust
//! Orchestrates one earnings sync: ensure a fresh access token, fetch
//! transactions since the stored cursor, plan income rows, insert them
//! idempotently, and advance the cursor. Network access goes through the
//! `UpworkClient` trait so this is testable with a fake.

use crate::db::Db;
use crate::repo::{cashflow, cashflow_categories, upwork_integration};
use crate::upwork::client::UpworkClient;
use crate::upwork::oauth::{self, OAuthConfig};
use crate::upwork::sync::plan_earnings;

const CATEGORY_NAME: &str = "Upwork";

/// Execute one fetch→plan→insert pass with a given client. Pure DB + trait; no
/// env reads, so tests inject a fake. Returns the number of new rows inserted.
pub async fn run_pass<C: UpworkClient>(db: &Db, client: &C) -> anyhow::Result<usize> {
    let cursor = upwork_integration::get(db).await?.and_then(|r| r.earnings_cursor);
    let batch = client
        .fetch_transactions(cursor.as_deref())
        .await
        .map_err(|e| anyhow::anyhow!("upwork fetch failed: {e}"))?;

    let category = cashflow_categories::ensure_by_name(db, CATEGORY_NAME, "income").await?;
    let planned = plan_earnings(&batch.txns, category.id);

    let mut inserted = 0usize;
    for p in &planned {
        if cashflow::insert_sourced(db, &p.cashflow, "upwork", &p.external_ref).await? {
            inserted += 1;
        }
    }
    if let Some(cur) = batch.next_cursor {
        upwork_integration::set_cursor(db, &cur).await?;
    }
    Ok(inserted)
}

/// Ensure a non-expired access token, refreshing if needed. Returns plaintext.
async fn ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8; 32]) -> anyhow::Result<String> {
    let row = upwork_integration::get(db).await?
        .ok_or_else(|| anyhow::anyhow!("not connected"))?;
    let expired = chrono::DateTime::parse_from_rfc3339(&row.expiry)
        .map(|exp| chrono::Utc::now() >= exp.with_timezone(&chrono::Utc))
        .unwrap_or(true);
    if !expired {
        return crate::upwork::crypto::decrypt(&row.access_token, key);
    }
    let refresh = crate::upwork::crypto::decrypt(&row.refresh_token, key)?;
    let tokens = oauth::refresh_access(cfg, &refresh).await?;
    let enc = crate::upwork::crypto::encrypt(&tokens.access_token, key)?;
    let expiry = oauth::expiry_from_now(tokens.expires_in);
    upwork_integration::update_access(db, &enc, &expiry).await?;
    Ok(tokens.access_token)
}

/// One full cycle including auth. Records integration status on failure.
/// Invoked by the manual `POST /api/upwork/sync` route (no background loop in v1).
pub async fn run_cycle(db: &Db) -> anyhow::Result<usize> {
    let Some(row) = upwork_integration::get(db).await? else { return Ok(0) };
    if row.status == "disconnected" { return Ok(0); }
    let cfg = OAuthConfig::from_env()?;
    let key = crate::upwork::crypto::key_from_env()?;
    let token = match ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(0);
        }
    };
    let client = crate::upwork::client::HttpUpwork::new(token);
    match run_pass(db, &client).await {
        Ok(n) => {
            if row.status == "error" {
                upwork_integration::set_status(db, "connected", None).await?;
            }
            Ok(n)
        }
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upwork::client::testkit::FakeUpwork;
    use crate::upwork::UpworkTransaction;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    fn txn(id: &str, kind: &str, amount: &str) -> UpworkTransaction {
        UpworkTransaction {
            external_id: id.into(), date: "2026-06-10".into(), kind: kind.into(),
            amount: amount.into(), currency: "USD".into(), contract: Some("Acme".into()),
        }
    }

    #[tokio::test]
    async fn run_pass_inserts_earnings_and_is_idempotent() {
        let db = mem_db().await;
        upwork_integration::upsert(&db, "a", "r", "2030-01-01T00:00:00+00:00", "s").await.unwrap();
        let fake = FakeUpwork::with(
            vec![txn("t1", "Hourly", "120.00"), txn("t2", "Service Fee", "-10.00")],
            Some("cur-2".into()),
        );

        let first = run_pass(&db, &fake).await.unwrap();
        assert_eq!(first, 1, "only the earning is inserted");
        let rows = cashflow::list_all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].direction, "in");
        assert_eq!(rows[0].source.as_deref(), Some("upwork"));

        // cursor advanced
        let cur = upwork_integration::get(&db).await.unwrap().unwrap().earnings_cursor;
        assert_eq!(cur.as_deref(), Some("cur-2"));

        // re-run: no duplicates
        let second = run_pass(&db, &fake).await.unwrap();
        assert_eq!(second, 0);
        assert_eq!(cashflow::list_all(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn run_pass_passes_stored_cursor_to_client() {
        let db = mem_db().await;
        upwork_integration::upsert(&db, "a", "r", "2030-01-01T00:00:00+00:00", "s").await.unwrap();
        upwork_integration::set_cursor(&db, "cur-1").await.unwrap();
        let fake = FakeUpwork::with(vec![], None);
        run_pass(&db, &fake).await.unwrap();
        assert_eq!(fake.seen_cursor.lock().unwrap().as_deref(), Some("cur-1"));
    }

    /// Live round-trip against a real Upwork account. Requires UPWORK_CLIENT_ID,
    /// UPWORK_CLIENT_SECRET, UPWORK_REDIRECT_URI, UPWORK_TOKEN_ENC_KEY, and an
    /// already-connected upwork_integration row in a file DB at UPWORK_SMOKE_DB.
    /// Run: UPWORK_SMOKE_DB=sqlite:///tmp/upwork.db cargo test upwork::engine::tests::live_cycle -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_cycle() {
        let url = match std::env::var("UPWORK_SMOKE_DB") { Ok(u) => u, Err(_) => return };
        let db = crate::db::connect(&url).await.unwrap();
        let n = run_cycle(&db).await.unwrap();
        let row = upwork_integration::get(&db).await.unwrap().unwrap();
        assert_eq!(row.status, "connected", "last_error={:?}", row.last_error);
        eprintln!("inserted {n} new earnings");
    }
}
```

- [ ] **Step 2: Run tests to verify they pass**

Run: `cd backend && cargo test upwork::engine::tests::run_pass`
Expected: PASS (two tests; the ignored `live_cycle` is skipped).

- [ ] **Step 3: Commit**

```bash
git add backend/src/upwork/engine.rs
git commit -m "feat(upwork): sync engine with token refresh + idempotent insert"
```

---

## Task 10: API routes (`api/upwork.rs`)

**Files:**
- Create: `backend/src/api/upwork.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Write `api/upwork.rs`** (mirrors `api/google.rs`)

```rust
use crate::error::AppError;
use crate::AppState;
use axum::{extract::{Query, State}, response::Redirect, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct StartOut { pub consent_url: String }

/// Build the Upwork consent URL (frontend redirects the browser to it).
pub async fn start() -> Result<Json<StartOut>, AppError> {
    let cfg = crate::upwork::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("upwork not configured: {e}")))?;
    let secret = crate::auth::jwt_secret()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("JWT_SECRET not set")))?;
    let now = chrono::Utc::now().timestamp();
    let state = crate::google::oauth::sign_state_with(&secret, now).map_err(AppError::Other)?;
    let consent_url = crate::upwork::oauth::consent_url(&cfg.client_id, &cfg.redirect_uri, &state);
    Ok(Json(StartOut { consent_url }))
}

#[derive(Deserialize)]
pub struct CallbackQuery { pub code: Option<String>, pub state: Option<String> }

/// Public OAuth callback. Guarded by the signed `state` (CSRF). Redirects to settings.
pub async fn callback(State(s): State<AppState>, Query(q): Query<CallbackQuery>) -> Result<Redirect, AppError> {
    let (code, state) = match (q.code, q.state) {
        (Some(c), Some(st)) => (c, st),
        _ => return Err(AppError::BadRequest("missing code/state".into())),
    };
    let secret = crate::auth::jwt_secret()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("JWT_SECRET not set")))?;
    let now = chrono::Utc::now().timestamp();
    if !crate::google::oauth::verify_state_with(&secret, &state, now) {
        return Err(AppError::Unauthorized("invalid state".into()));
    }
    let cfg = crate::upwork::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("upwork not configured: {e}")))?;
    let key = crate::upwork::crypto::key_from_env().map_err(AppError::Other)?;
    let tokens = crate::upwork::oauth::exchange_code(&cfg, &code).await.map_err(AppError::Other)?;
    let refresh = tokens.refresh_token.clone()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("no refresh_token returned; re-consent required")))?;
    let enc_access = crate::upwork::crypto::encrypt(&tokens.access_token, &key).map_err(AppError::Other)?;
    let enc_refresh = crate::upwork::crypto::encrypt(&refresh, &key).map_err(AppError::Other)?;
    let expiry = crate::upwork::oauth::expiry_from_now(tokens.expires_in);
    let scope = tokens.scope.unwrap_or_default();
    crate::repo::upwork_integration::upsert(&s.db, &enc_access, &enc_refresh, &expiry, &scope)
        .await.map_err(AppError::Other)?;
    Ok(Redirect::to("/settings?upwork=connected"))
}

#[derive(Serialize)]
pub struct StatusOut { pub status: String, pub last_error: Option<String> }

pub async fn status(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    let row = crate::repo::upwork_integration::get(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(match row {
        Some(r) => StatusOut { status: r.status, last_error: r.last_error },
        None => StatusOut { status: "disconnected".into(), last_error: None },
    }))
}

#[derive(Serialize)]
pub struct SyncOut { pub inserted: usize }

/// Trigger an earnings sync now (manual; no background loop in v1).
pub async fn sync(State(s): State<AppState>) -> Result<Json<SyncOut>, AppError> {
    let inserted = crate::upwork::engine::run_cycle(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(SyncOut { inserted }))
}

pub async fn disconnect(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    crate::repo::upwork_integration::delete(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(StatusOut { status: "disconnected".into(), last_error: None }))
}
```

- [ ] **Step 2: Register the routes in `api/mod.rs`**

Add `pub mod upwork;` to the module list at the top of `backend/src/api/mod.rs` (after `pub mod telegram;`).

In the `public` router (next to the google callback), add:

```rust
        .route("/upwork/oauth/callback", get(upwork::callback));
```

In the `protected` router (next to the google routes), add:

```rust
        .route("/upwork/oauth/start", get(upwork::start))
        .route("/upwork/status", get(upwork::status))
        .route("/upwork/sync", post(upwork::sync))
        .route("/upwork/disconnect", post(upwork::disconnect))
```

- [ ] **Step 3: Build to verify wiring compiles**

Run: `cd backend && cargo build`
Expected: builds clean.

- [ ] **Step 4: Run the full backend test suite**

Run: `cd backend && cargo test`
Expected: all PASS (ignored live tests skipped).

- [ ] **Step 5: Commit**

```bash
git add backend/src/api/upwork.rs backend/src/api/mod.rs
git commit -m "feat(upwork): oauth + sync + status api routes"
```

---

## Task 11: Environment wiring (compose, k8s, example)

**Files:**
- Modify: `docker-compose.yml`, `docker-compose.prod.yml`, `.env.production.example`
- Modify: `k8s/` backend Deployment/Secret manifests

- [ ] **Step 1: Add the four env vars wherever `GOOGLE_*` already appears**

For each file that references `GOOGLE_CLIENT_ID` / `GOOGLE_TOKEN_ENC_KEY`, add the parallel Upwork entries next to them:

```
UPWORK_CLIENT_ID=
UPWORK_CLIENT_SECRET=
UPWORK_REDIRECT_URI=https://<your-domain>/api/upwork/oauth/callback
UPWORK_TOKEN_ENC_KEY=   # base64 of 32 random bytes: `openssl rand -base64 32`
```

Find the exact spots:

Run: `grep -rln "GOOGLE_TOKEN_ENC_KEY\|GOOGLE_CLIENT_ID" docker-compose.yml docker-compose.prod.yml .env.production.example k8s/`

Mirror each occurrence (compose `environment:` blocks, k8s `env:`/`secretKeyRef` entries, and the example file) with the `UPWORK_*` equivalents.

- [ ] **Step 2: Validate compose parses**

Run: `docker compose -f docker-compose.yml config >/dev/null && echo OK`
Expected: `OK`.

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml docker-compose.prod.yml .env.production.example k8s/
git commit -m "feat(upwork): env wiring for compose + k8s"
```

---

## Task 12: Frontend — "Connect Upwork" card (additive)

**Files:**
- Modify: the Connectors/Settings page that already renders the Google connector card (find it in Step 1).

This is purely additive UI mirroring the existing Google card; it does not change `src/api/{client,schemas,hooks}.ts` semantics (per the redesign brief's hard constraint).

- [ ] **Step 1: Locate the Google connector card**

Run: `cd frontend && grep -rln "google/status\|google/oauth/start\|google=connected" src/`
Open the component(s) that render the Google connect/status card.

- [ ] **Step 2: Add an Upwork card following the same pattern**

Duplicate the Google card block and swap the endpoints/labels:
- Status: `GET /api/upwork/status` → `{ status, last_error }`
- Connect: `GET /api/upwork/oauth/start` → `{ consent_url }`, then `window.location.href = consent_url`
- Disconnect: `POST /api/upwork/disconnect`
- Sync now: `POST /api/upwork/sync` → `{ inserted }` (show a toast: "Imported N earnings")
- Title: "Upwork", subtitle: "Earnings → income"

Keep money as strings; this card shows status only, no amounts.

- [ ] **Step 3: Run frontend tests + build**

Run: `cd frontend && npm test && npm run build`
Expected: both PASS (per the redesign brief's hard constraint: keep tests green, build clean).

- [ ] **Step 4: Commit**

```bash
git add frontend/src
git commit -m "feat(ui): add Upwork connector card"
```

---

## Final verification

- [ ] **Backend:** `cd backend && cargo test` → all green; `cargo build` clean.
- [ ] **Frontend:** `cd frontend && npm test && npm run build` → green.
- [ ] **Manual smoke (after API key approved):** set `UPWORK_*` env, connect via the card, then `POST /api/upwork/sync`; confirm Upwork income rows appear in the cashflow view with `source='upwork'`, USD amounts, and that portfolio/net-worth values are unchanged. Optionally run the gated live test (`UPWORK_SMOKE_DB=… cargo test upwork::engine::tests::live_cycle -- --ignored`).

---

## Self-review notes (author)

- **Spec coverage:** module layout (Tasks 5–9), migration + idempotency (Tasks 1–2), category ensure (Task 3), token store (Task 4), Approach A earning filter (Task 7), mockable client / build-before-key (Task 8), routes + manual sync (Task 10), error handling via `set_status` + skip-non-earnings (Tasks 7, 9), env wiring (Task 11), minimal frontend (Task 12), gated live test (Task 9). Periodic auto-sync intentionally omitted (out of scope v1).
- **Type consistency:** `UpworkTransaction` (mod.rs) → `plan_earnings`/`PlannedEarning` (sync.rs) → `insert_sourced` (cashflow.rs) → `run_pass`/`run_cycle` (engine.rs) → `sync` route (api). `TransactionBatch`/`UpworkClient` shared by `HttpUpwork` and `FakeUpwork`. Names checked across tasks.
- **No portfolio writes:** only `cashflow` + `cashflow_category` + `upwork_integration` tables are written anywhere in the plan.
