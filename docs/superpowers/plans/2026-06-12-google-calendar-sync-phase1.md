# Google Calendar Sync — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Connect the assistant's `events` agenda to a user's primary Google Calendar with two-way sync (app-owned events push/patch/delete; foreign events imported read-only for briefing awareness), gated behind a one-time Google OAuth connection.

**Architecture:** A new `backend/src/google/` module (separate from the financial `connectors`) holds OAuth, a mockable Calendar HTTP client, and a pure reconciler. Sync runs from a dedicated 5-minute loop (mirroring `proactive::spawn`, but independent of Telegram). App-owned Google events are tagged via `extendedProperties.private.app=portfolio`; only tagged events are mutated. Tokens are encrypted at rest (AES-GCM). The OAuth `state` is a short-lived JWT signed with the existing `JWT_SECRET`.

**Tech Stack:** Rust (axum, sqlx/SQLite, reqwest, jsonwebtoken, aes-gcm), React/TypeScript frontend, Google Calendar API v3.

**Spec:** `docs/superpowers/specs/2026-06-12-google-calendar-sync-design.md`

**Conventions in this codebase (follow these):**
- `Db = SqlitePool`; migrations auto-run on `crate::db::connect`. Tests use `crate::db::connect("sqlite::memory:")`.
- Repo functions return `anyhow::Result<T>`, use `sqlx::query`/`query_as`, timestamps via `chrono::Utc::now().to_rfc3339()` (audit) or Z-format for schedule comparisons.
- HTTP clients follow `backend/src/llm/claude.rs` (reqwest, `thiserror` error enum, 120s timeout).
- Inline `#[cfg(test)] mod tests` per file. Live network tests are `#[ignore]`.
- Run tests with `cargo test` from `backend/`.

---

## Task 0: Confirm migration number is free

- [ ] **Step 1: Verify no `0014_*` migration exists upstream**

Run:
```bash
cd backend && git fetch origin main -q && git ls-tree -r origin/main --name-only migrations/ | grep -E '0014' || echo "0014 is free"
ls migrations/ | tail -3
```
Expected: prints `0014 is free` and the latest local file is `0013_events.sql`. If a `0014_*` already exists upstream, renumber this plan's migration to the next free number and update all references.

---

## Task 1: Schema migration + EventRow fields

**Files:**
- Create: `backend/migrations/0014_google_calendar.sql`
- Modify: `backend/src/repo/events.rs` (the `EventRow` struct, ~lines 6-15)

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0014_google_calendar.sql`:
```sql
-- Phase 1 Google Calendar sync. Single-row integration holding OAuth tokens
-- (encrypted at rest) and the Calendar incremental sync token.
CREATE TABLE google_integration (
  id INTEGER PRIMARY KEY CHECK (id = 1),
  access_token TEXT NOT NULL,          -- AES-GCM ciphertext (base64)
  refresh_token TEXT NOT NULL,         -- AES-GCM ciphertext (base64)
  expiry TEXT NOT NULL,                -- RFC3339 UTC access-token expiry
  scope TEXT NOT NULL,
  calendar_sync_token TEXT,            -- Google events.list nextSyncToken
  status TEXT NOT NULL DEFAULT 'connected'
    CHECK (status IN ('connected', 'disconnected', 'error')),
  last_error TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

-- events ownership + Google linkage. 'local' = app-owned (two-way);
-- 'google' = foreign import (read-only). updated_at drives last-write-wins.
ALTER TABLE events ADD COLUMN source TEXT NOT NULL DEFAULT 'local'
  CHECK (source IN ('local', 'google'));
ALTER TABLE events ADD COLUMN google_event_id TEXT;
ALTER TABLE events ADD COLUMN google_etag TEXT;
ALTER TABLE events ADD COLUMN synced_at TEXT;
ALTER TABLE events ADD COLUMN updated_at TEXT;
UPDATE events SET updated_at = created_at WHERE updated_at IS NULL;

CREATE INDEX idx_events_google ON events (google_event_id);
CREATE INDEX idx_events_sync ON events (source, synced_at);
```

- [ ] **Step 2: Extend `EventRow`**

In `backend/src/repo/events.rs`, replace the struct (lines 6-15) with:
```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EventRow {
    pub id: i64,
    pub title: String,
    pub location: Option<String>,
    pub notes: Option<String>,
    pub start_at: String,
    pub status: String,
    pub created_at: String,
    pub source: String,
    pub google_event_id: Option<String>,
    pub google_etag: Option<String>,
    pub synced_at: Option<String>,
    pub updated_at: Option<String>,
}
```

- [ ] **Step 3: Run existing events tests to verify schema + struct still load**

Run: `cargo test repo::events:: -- --nocapture`
Expected: PASS (the three existing tests — `SELECT *` now returns the new columns, which `EventRow` maps).

- [ ] **Step 4: Commit**

```bash
git add backend/migrations/0014_google_calendar.sql backend/src/repo/events.rs
git commit -m "feat(google): schema for integration + events sync columns"
```

---

## Task 2: events repo — sync queries, updated_at, source guard

**Files:**
- Modify: `backend/src/repo/events.rs`

- [ ] **Step 1: Write failing tests for the new repo functions**

Append to the `tests` module in `backend/src/repo/events.rs`:
```rust
    #[tokio::test]
    async fn create_sets_local_source_and_updated_at() {
        let db = mem_db().await;
        let e = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        assert_eq!(e.source, "local");
        assert_eq!(e.updated_at.as_deref(), Some(e.created_at.as_str()));
        assert!(e.google_event_id.is_none());
    }

    #[tokio::test]
    async fn unsynced_local_then_marked_synced_drops_out_of_pending() {
        let db = mem_db().await;
        let e = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        // Brand-new local event with no google id is pending push.
        assert_eq!(pending_push(&db).await.unwrap().len(), 1);
        mark_synced(&db, e.id, "gcal-1", "etag-1").await.unwrap();
        // After syncing with synced_at >= updated_at it is no longer pending.
        assert!(pending_push(&db).await.unwrap().is_empty());
        let got = get(&db, e.id).await.unwrap();
        assert_eq!(got.google_event_id.as_deref(), Some("gcal-1"));
        assert_eq!(got.google_etag.as_deref(), Some("etag-1"));
    }

    #[tokio::test]
    async fn upsert_foreign_inserts_then_updates_by_google_id() {
        let db = mem_db().await;
        let id = upsert_foreign(&db, "gid-9", "rapat A", None, None, "2026-06-13T03:00:00Z", "etag-a").await.unwrap();
        let again = upsert_foreign(&db, "gid-9", "rapat A (edit)", Some("zoom"), None, "2026-06-13T03:00:00Z", "etag-b").await.unwrap();
        assert_eq!(id, again, "same google id updates the same row");
        let row = get(&db, id).await.unwrap();
        assert_eq!(row.source, "google");
        assert_eq!(row.title, "rapat A (edit)");
        assert_eq!(row.location.as_deref(), Some("zoom"));
    }

    #[tokio::test]
    async fn cancel_refuses_foreign_events() {
        let db = mem_db().await;
        let id = upsert_foreign(&db, "gid-1", "foreign", None, None, "2026-06-13T03:00:00Z", "etag").await.unwrap();
        // The agent-facing cancel must not touch google-sourced rows.
        assert!(!cancel(&db, id).await.unwrap());
        assert_eq!(get(&db, id).await.unwrap().status, "scheduled");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test repo::events:: -- --nocapture`
Expected: FAIL — `pending_push`, `mark_synced`, `upsert_foreign` not found.

- [ ] **Step 3: Implement the new functions and guard `cancel`**

In `backend/src/repo/events.rs`, set `updated_at` in `create` (replace the INSERT in `create`, lines 25-36):
```rust
    let id = sqlx::query(
        "INSERT INTO events (title, location, notes, start_at, status, source, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'scheduled', 'local', ?, ?)",
    )
    .bind(title)
    .bind(location)
    .bind(notes)
    .bind(start_at)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
```

Change `cancel` (lines 64-71) to only cancel app-owned events:
```rust
/// Cancel a scheduled app-owned event. False when missing, already cancelled,
/// or foreign (source='google' rows are read-only to the assistant).
pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE events SET status = 'cancelled', updated_at = ?
         WHERE id = ? AND status = 'scheduled' AND source = 'local'",
    )
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

Add these functions (after `cancel`):
```rust
/// App-owned events whose local edits are not yet pushed: never synced, or
/// edited since the last successful sync.
pub async fn pending_push(db: &Db) -> anyhow::Result<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT * FROM events
         WHERE source = 'local'
           AND (google_event_id IS NULL OR synced_at IS NULL OR updated_at > synced_at)
         ORDER BY id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Record a successful push: store the Google id/etag and advance synced_at to now.
pub async fn mark_synced(db: &Db, id: i64, google_event_id: &str, etag: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE events SET google_event_id = ?, google_etag = ?, synced_at = ? WHERE id = ?",
    )
    .bind(google_event_id)
    .bind(etag)
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(())
}

/// Find an app row by Google event id (either local or foreign).
pub async fn get_by_google_id(db: &Db, google_event_id: &str) -> anyhow::Result<Option<EventRow>> {
    let row = sqlx::query_as::<_, EventRow>("SELECT * FROM events WHERE google_event_id = ?")
        .bind(google_event_id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

/// Insert or update a foreign (read-only) Google event, keyed by google id.
/// Returns the app row id.
pub async fn upsert_foreign(
    db: &Db,
    google_event_id: &str,
    title: &str,
    location: Option<&str>,
    notes: Option<&str>,
    start_at: &str,
    etag: &str,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    if let Some(existing) = get_by_google_id(db, google_event_id).await? {
        sqlx::query(
            "UPDATE events SET title = ?, location = ?, notes = ?, start_at = ?,
             google_etag = ?, synced_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(title).bind(location).bind(notes).bind(start_at)
        .bind(etag).bind(&now).bind(&now).bind(existing.id)
        .execute(db).await?;
        return Ok(existing.id);
    }
    let id = sqlx::query(
        "INSERT INTO events (title, location, notes, start_at, status, source,
            google_event_id, google_etag, synced_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'scheduled', 'google', ?, ?, ?, ?, ?)",
    )
    .bind(title).bind(location).bind(notes).bind(start_at)
    .bind(google_event_id).bind(etag).bind(&now).bind(&now).bind(&now)
    .execute(db).await?.last_insert_rowid();
    Ok(id)
}

/// Mark a row cancelled regardless of source — used by inbound sync when the
/// Google event was deleted. Distinct from the agent-facing `cancel`.
pub async fn cancel_by_sync(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE events SET status = 'cancelled', synced_at = ?, updated_at = ? WHERE id = ?")
        .bind(&now).bind(&now).bind(id)
        .execute(db).await?;
    Ok(())
}

/// Update an app-owned row from an inbound Google change (Google won this turn).
pub async fn update_from_google(
    db: &Db, id: i64, title: &str, location: Option<&str>, notes: Option<&str>,
    start_at: &str, etag: &str,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "UPDATE events SET title = ?, location = ?, notes = ?, start_at = ?,
         google_etag = ?, synced_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(title).bind(location).bind(notes).bind(start_at)
    .bind(etag).bind(&now).bind(&now).bind(id)
    .execute(db).await?;
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test repo::events:: -- --nocapture`
Expected: PASS (all events tests, old and new).

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/events.rs
git commit -m "feat(google): events repo sync queries + read-only foreign guard"
```

---

## Task 3: Token encryption (AES-GCM)

**Files:**
- Modify: `backend/Cargo.toml` (add `aes-gcm`)
- Create: `backend/src/google/mod.rs`, `backend/src/google/crypto.rs`
- Modify: `backend/src/main.rs` (add `mod google;`)

- [ ] **Step 1: Add the dependency**

In `backend/Cargo.toml` under `[dependencies]`, after `rand = "0.8"`:
```toml
aes-gcm = "0.10"
```

- [ ] **Step 2: Register the module**

Create `backend/src/google/mod.rs`:
```rust
//! Google Calendar integration: OAuth, a mockable Calendar client, and the
//! two-way sync reconciler. Separate from the financial `connectors` module.

pub mod crypto;
```

Add to `backend/src/main.rs` alongside the other `mod` lines:
```rust
mod google;
```

- [ ] **Step 3: Write failing tests for crypto**

Create `backend/src/google/crypto.rs`:
```rust
//! AES-256-GCM encryption for OAuth tokens at rest. The key comes from
//! GOOGLE_TOKEN_ENC_KEY (base64-encoded 32 bytes). Fail closed: callers must
//! treat a missing/invalid key as "cannot connect", never store plaintext.

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] { [7u8; 32] }

    #[test]
    fn round_trips() {
        let k = key();
        let ct = encrypt("ya29.secret-token", &k).unwrap();
        assert_ne!(ct, "ya29.secret-token");
        assert_eq!(decrypt(&ct, &k).unwrap(), "ya29.secret-token");
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt("secret", &key()).unwrap();
        let mut bad = key();
        bad[0] = 0;
        assert!(decrypt(&ct, &bad).is_err());
    }

    #[test]
    fn key_from_base64_validates_length() {
        use base64::Engine;
        let good = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        assert!(key_from_env_value(&good).is_ok());
        let short = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
        assert!(key_from_env_value(&short).is_err());
        assert!(key_from_env_value("not-base64!!!").is_err());
    }
}
```

- [ ] **Step 4: Run to verify failure**

Run: `cargo test google::crypto:: -- --nocapture`
Expected: FAIL — `encrypt`/`decrypt`/`key_from_env_value` not defined.

- [ ] **Step 5: Implement crypto**

Prepend to `backend/src/google/crypto.rs` (above the tests module):
```rust
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use rand::RngCore;

/// Parse a base64-encoded 32-byte key (the GOOGLE_TOKEN_ENC_KEY value).
pub fn key_from_env_value(b64: &str) -> anyhow::Result<[u8; 32]> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| anyhow::anyhow!("GOOGLE_TOKEN_ENC_KEY is not valid base64"))?;
    let key: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("GOOGLE_TOKEN_ENC_KEY must decode to exactly 32 bytes"))?;
    Ok(key)
}

/// Read + parse the key from the environment. Err when unset (fail closed).
pub fn key_from_env() -> anyhow::Result<[u8; 32]> {
    let b64 = std::env::var("GOOGLE_TOKEN_ENC_KEY")
        .map_err(|_| anyhow::anyhow!("GOOGLE_TOKEN_ENC_KEY is not set"))?;
    key_from_env_value(&b64)
}

/// Encrypt to base64(nonce[12] || ciphertext).
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encrypt failed: {e}"))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(blob))
}

/// Decrypt base64(nonce[12] || ciphertext).
pub fn decrypt(b64: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let blob = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| anyhow::anyhow!("ciphertext is not valid base64"))?;
    if blob.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|e| anyhow::anyhow!("decrypt failed: {e}"))?;
    Ok(String::from_utf8(pt)?)
}
```

- [ ] **Step 6: Run to verify pass**

Run: `cargo test google::crypto:: -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 7: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/src/google/mod.rs backend/src/google/crypto.rs backend/src/main.rs
git commit -m "feat(google): AES-256-GCM token encryption (fail-closed key)"
```

---

## Task 4: google_integration repo

**Files:**
- Create: `backend/src/repo/google_integration.rs`
- Modify: `backend/src/repo/mod.rs` (add `pub mod google_integration;`)

- [ ] **Step 1: Write failing tests**

Create `backend/src/repo/google_integration.rs`:
```rust
//! Single-row persistence for the Google connection (see migration 0014).
//! Tokens are stored already-encrypted by the caller (see google::crypto).

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
    pub calendar_sync_token: Option<String>,
    pub status: String,
    pub last_error: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn get_is_none_before_connect() {
        let db = mem_db().await;
        assert!(get(&db).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn upsert_then_get_round_trips_and_stays_single_row() {
        let db = mem_db().await;
        upsert(&db, "enc-access", "enc-refresh", "2026-06-12T10:00:00+00:00", "calendar.events").await.unwrap();
        upsert(&db, "enc-access2", "enc-refresh2", "2026-06-12T11:00:00+00:00", "calendar.events").await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.access_token, "enc-access2");
        assert_eq!(row.status, "connected");
    }

    #[tokio::test]
    async fn sync_token_and_status_and_delete() {
        let db = mem_db().await;
        upsert(&db, "a", "r", "2026-06-12T10:00:00+00:00", "calendar.events").await.unwrap();
        set_sync_token(&db, "tok-123").await.unwrap();
        set_status(&db, "error", Some("invalid_grant")).await.unwrap();
        let row = get(&db).await.unwrap().unwrap();
        assert_eq!(row.calendar_sync_token.as_deref(), Some("tok-123"));
        assert_eq!(row.status, "error");
        assert_eq!(row.last_error.as_deref(), Some("invalid_grant"));
        delete(&db).await.unwrap();
        assert!(get(&db).await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test repo::google_integration:: -- --nocapture`
Expected: FAIL — functions + `pub mod` missing.

- [ ] **Step 3: Register module + implement**

Add to `backend/src/repo/mod.rs`:
```rust
pub mod google_integration;
```

Prepend to `backend/src/repo/google_integration.rs` (above the tests module, after the struct):
```rust
pub async fn get(db: &Db) -> anyhow::Result<Option<IntegrationRow>> {
    let row = sqlx::query_as::<_, IntegrationRow>("SELECT * FROM google_integration WHERE id = 1")
        .fetch_optional(db)
        .await?;
    Ok(row)
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
        "INSERT INTO google_integration
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
    sqlx::query(
        "UPDATE google_integration SET access_token = ?, expiry = ?, updated_at = ? WHERE id = 1",
    )
    .bind(enc_access_token).bind(expiry).bind(&now)
    .execute(db).await?;
    Ok(())
}

pub async fn set_sync_token(db: &Db, token: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE google_integration SET calendar_sync_token = ? WHERE id = 1")
        .bind(token)
        .execute(db).await?;
    Ok(())
}

pub async fn clear_sync_token(db: &Db) -> anyhow::Result<()> {
    sqlx::query("UPDATE google_integration SET calendar_sync_token = NULL WHERE id = 1")
        .execute(db).await?;
    Ok(())
}

pub async fn set_status(db: &Db, status: &str, last_error: Option<&str>) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE google_integration SET status = ?, last_error = ?, updated_at = ? WHERE id = 1")
        .bind(status).bind(last_error).bind(&now)
        .execute(db).await?;
    Ok(())
}

pub async fn delete(db: &Db) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM google_integration WHERE id = 1").execute(db).await?;
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test repo::google_integration:: -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/google_integration.rs backend/src/repo/mod.rs
git commit -m "feat(google): single-row integration repo"
```

---

## Task 5: OAuth module (config, state, consent URL, token exchange/refresh)

**Files:**
- Create: `backend/src/google/oauth.rs`
- Modify: `backend/src/google/mod.rs` (add `pub mod oauth;`)

- [ ] **Step 1: Write failing tests for the pure parts**

Create `backend/src/google/oauth.rs`:
```rust
//! Google OAuth: env config, signed `state`, consent URL, and token
//! exchange/refresh. The `state` is a short-lived JWT signed with JWT_SECRET,
//! so the unauthenticated callback can trust it (CSRF guard).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_url_has_required_params() {
        let url = consent_url("client-123", "https://app/api/google/oauth/callback", "STATEVAL");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=client-123"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("state=STATEVAL"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar.events"));
        // redirect_uri is percent-encoded
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp%2Fapi%2Fgoogle%2Foauth%2Fcallback"));
    }

    #[test]
    fn state_round_trips_and_rejects_tampering() {
        let secret = "test-jwt-secret";
        let now = 1_000_000;
        let token = sign_state_with(secret, now).unwrap();
        // Valid within TTL.
        assert!(verify_state_with(secret, &token, now + 60));
        // Expired after TTL (600s).
        assert!(!verify_state_with(secret, &token, now + 601));
        // Wrong secret rejected.
        assert!(!verify_state_with("other-secret", &token, now + 60));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test google::oauth:: -- --nocapture`
Expected: FAIL — `consent_url`, `sign_state_with`, `verify_state_with` not defined.

- [ ] **Step 3: Implement OAuth (pure parts + HTTP)**

Add `pub mod oauth;` to `backend/src/google/mod.rs`, then prepend to `backend/src/google/oauth.rs` (above tests):
```rust
use serde::{Deserialize, Serialize};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
pub const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
const STATE_TTL_SECS: i64 = 600;

/// OAuth client config from the environment.
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OAuthConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client_id: std::env::var("GOOGLE_CLIENT_ID")
                .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_ID is not set"))?,
            client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
                .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_SECRET is not set"))?,
            redirect_uri: std::env::var("GOOGLE_REDIRECT_URI")
                .map_err(|_| anyhow::anyhow!("GOOGLE_REDIRECT_URI is not set"))?,
        })
    }
}

fn enc(s: &str) -> String {
    // Minimal percent-encoding for query values (RFC 3986 unreserved kept).
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the Google consent URL. `state` is the signed CSRF token.
pub fn consent_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{AUTH_ENDPOINT}?response_type=code&access_type=offline&prompt=consent\
         &client_id={}&redirect_uri={}&scope={}&state={}",
        enc(client_id), enc(redirect_uri), enc(SCOPE), enc(state)
    )
}

#[derive(Serialize, Deserialize)]
struct StateClaims { exp: i64, nonce: String }

pub fn sign_state_with(secret: &str, now: i64) -> anyhow::Result<String> {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let mut nonce = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let claims = StateClaims {
        exp: now + STATE_TTL_SECS,
        nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce),
    };
    Ok(encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))?)
}

pub fn verify_state_with(secret: &str, token: &str, now: i64) -> bool {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    let mut validation = Validation::default();
    validation.validate_exp = false; // we check exp explicitly against `now`
    match decode::<StateClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation) {
        Ok(data) => now <= data.claims.exp,
        Err(_) => false,
    }
}

/// Tokens returned by the code-exchange / refresh endpoints.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Compute an RFC3339 expiry from `expires_in` seconds, with a 60s safety skew.
pub fn expiry_from_now(expires_in: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(expires_in - 60)).to_rfc3339()
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(cfg: &OAuthConfig, code: &str) -> anyhow::Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client.post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
            ("redirect_uri", &cfg.redirect_uri),
        ])
        .send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("token exchange failed: {} {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

/// Refresh the access token using the stored refresh token.
pub async fn refresh_access(cfg: &OAuthConfig, refresh_token: &str) -> anyhow::Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client.post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &cfg.client_id),
            ("client_secret", &cfg.client_secret),
        ])
        .send().await?;
    if !resp.status().is_success() {
        anyhow::bail!("token refresh failed: {} {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

/// Best-effort token revocation on disconnect.
pub async fn revoke(token: &str) -> anyhow::Result<()> {
    reqwest::Client::new().post(REVOKE_ENDPOINT)
        .form(&[("token", token)])
        .send().await?;
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test google::oauth:: -- --nocapture`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/google/oauth.rs backend/src/google/mod.rs
git commit -m "feat(google): OAuth config, signed state, consent URL, token exchange"
```

---

## Task 6: Calendar client (trait + types + HTTP impl)

**Files:**
- Create: `backend/src/google/calendar.rs`
- Modify: `backend/src/google/mod.rs` (add `pub mod calendar;`)

- [ ] **Step 1: Write failing tests for the request/response mapping**

Create `backend/src/google/calendar.rs`:
```rust
//! Thin Google Calendar API v3 client behind a trait so the sync reconciler can
//! be tested with a fake. Only the fields Phase 1 maps are modeled: summary,
//! location, description, start time, plus id/etag/updated and the ownership tag.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_request_body_tags_app_owned_and_sets_start() {
        let body = to_request_body(&EventWrite {
            summary: "rapat".into(),
            location: Some("kantor".into()),
            description: None,
            start_rfc3339_z: "2026-06-13T07:00:00Z".into(),
        });
        assert_eq!(body["summary"], "rapat");
        assert_eq!(body["location"], "kantor");
        assert_eq!(body["start"]["dateTime"], "2026-06-13T07:00:00Z");
        assert_eq!(body["extendedProperties"]["private"]["app"], "portfolio");
    }

    #[test]
    fn parse_event_reads_fields_and_app_tag() {
        let json = serde_json::json!({
            "id": "gid-1", "etag": "\"etag-1\"", "status": "confirmed",
            "summary": "rapat", "location": "kantor", "updated": "2026-06-12T09:00:00Z",
            "start": {"dateTime": "2026-06-13T07:00:00+07:00"},
            "extendedProperties": {"private": {"app": "portfolio"}}
        });
        let ev = parse_event(&json).unwrap();
        assert_eq!(ev.id, "gid-1");
        assert_eq!(ev.summary.as_deref(), Some("rapat"));
        assert!(ev.app_owned);
        assert!(!ev.cancelled);
    }

    #[test]
    fn parse_event_flags_cancelled_status() {
        let json = serde_json::json!({ "id": "gid-2", "etag": "e", "status": "cancelled" });
        let ev = parse_event(&json).unwrap();
        assert!(ev.cancelled);
        assert!(!ev.app_owned);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test google::calendar:: -- --nocapture`
Expected: FAIL — types/functions not defined.

- [ ] **Step 3: Implement types, mapping, trait, and HTTP client**

Add `pub mod calendar;` to `backend/src/google/mod.rs`, then prepend to `backend/src/google/calendar.rs` (above tests):
```rust
use async_trait::async_trait;

pub const APP_TAG: &str = "portfolio";

/// Fields we write to a Google event.
#[derive(Debug, Clone)]
pub struct EventWrite {
    pub summary: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub start_rfc3339_z: String,
}

/// A Google event as we read it.
#[derive(Debug, Clone)]
pub struct GCalEvent {
    pub id: String,
    pub etag: String,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub start_rfc3339: Option<String>,
    pub updated: Option<String>,
    pub cancelled: bool,
    pub app_owned: bool,
}

/// Result of a list call: the page of events plus the next sync token (if final).
pub struct EventPage {
    pub events: Vec<GCalEvent>,
    pub next_sync_token: Option<String>,
}

pub fn to_request_body(w: &EventWrite) -> serde_json::Value {
    serde_json::json!({
        "summary": w.summary,
        "location": w.location,
        "description": w.description,
        "start": { "dateTime": w.start_rfc3339_z },
        "end": { "dateTime": w.start_rfc3339_z },
        "extendedProperties": { "private": { "app": APP_TAG } },
    })
}

pub fn parse_event(v: &serde_json::Value) -> anyhow::Result<GCalEvent> {
    let id = v.get("id").and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("event without id"))?.to_string();
    let status = v.get("status").and_then(|x| x.as_str()).unwrap_or("confirmed");
    let start = v.get("start")
        .and_then(|s| s.get("dateTime").or_else(|| s.get("date")))
        .and_then(|x| x.as_str()).map(String::from);
    let app_owned = v.get("extendedProperties")
        .and_then(|e| e.get("private"))
        .and_then(|p| p.get("app"))
        .and_then(|x| x.as_str()) == Some(APP_TAG);
    Ok(GCalEvent {
        id,
        etag: v.get("etag").and_then(|x| x.as_str()).unwrap_or_default().to_string(),
        summary: v.get("summary").and_then(|x| x.as_str()).map(String::from),
        location: v.get("location").and_then(|x| x.as_str()).map(String::from),
        description: v.get("description").and_then(|x| x.as_str()).map(String::from),
        start_rfc3339: start,
        updated: v.get("updated").and_then(|x| x.as_str()).map(String::from),
        cancelled: status == "cancelled",
        app_owned,
    })
}

/// Error type that distinguishes the cases the sync loop reacts to.
#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    #[error("sync token expired (410)")]
    SyncTokenGone,
    #[error("precondition failed (412)")]
    PreconditionFailed,
    #[error("rate limited or server error: {0}")]
    Transient(String),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("http error: {0}")]
    Http(String),
}

/// The surface the reconciler depends on. Implemented by `HttpCalendar` in
/// production and a fake in tests.
#[async_trait]
pub trait CalendarApi {
    async fn insert(&self, w: &EventWrite) -> Result<GCalEvent, CalendarError>;
    async fn patch(&self, google_event_id: &str, etag: &str, w: &EventWrite) -> Result<GCalEvent, CalendarError>;
    async fn delete(&self, google_event_id: &str) -> Result<(), CalendarError>;
    /// List primary-calendar changes. Pass `sync_token` for incremental, else
    /// `time_min`/`time_max` for the initial window.
    async fn list(
        &self,
        sync_token: Option<&str>,
        time_min: &str,
        time_max: &str,
    ) -> Result<EventPage, CalendarError>;
}

/// Production HTTP client bound to the user's primary calendar.
pub struct HttpCalendar {
    access_token: String,
    client: reqwest::Client,
}

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";

impl HttpCalendar {
    pub fn new(access_token: String) -> Self {
        Self { access_token, client: reqwest::Client::new() }
    }

    fn classify(status: reqwest::StatusCode, body: String) -> CalendarError {
        match status.as_u16() {
            410 => CalendarError::SyncTokenGone,
            412 => CalendarError::PreconditionFailed,
            429 | 500..=599 => CalendarError::Transient(format!("{status}: {body}")),
            other => CalendarError::Api { status: other, body },
        }
    }
}

#[async_trait]
impl CalendarApi for HttpCalendar {
    async fn insert(&self, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
        let resp = self.client.post(BASE)
            .bearer_auth(&self.access_token)
            .json(&to_request_body(w))
            .send().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        if !status.is_success() { return Err(Self::classify(status, v.to_string())); }
        parse_event(&v).map_err(|e| CalendarError::Api { status: 200, body: e.to_string() })
    }

    async fn patch(&self, google_event_id: &str, etag: &str, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
        let resp = self.client.patch(format!("{BASE}/{google_event_id}"))
            .bearer_auth(&self.access_token)
            .header(reqwest::header::IF_MATCH, etag)
            .json(&to_request_body(w))
            .send().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        if !status.is_success() { return Err(Self::classify(status, v.to_string())); }
        parse_event(&v).map_err(|e| CalendarError::Api { status: 200, body: e.to_string() })
    }

    async fn delete(&self, google_event_id: &str) -> Result<(), CalendarError> {
        let resp = self.client.delete(format!("{BASE}/{google_event_id}"))
            .bearer_auth(&self.access_token)
            .send().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        // 404/410 mean it's already gone — treat as success (idempotent delete).
        if status.is_success() || status.as_u16() == 404 || status.as_u16() == 410 {
            return Ok(());
        }
        Err(Self::classify(status, resp.text().await.unwrap_or_default()))
    }

    async fn list(&self, sync_token: Option<&str>, time_min: &str, time_max: &str) -> Result<EventPage, CalendarError> {
        let mut req = self.client.get(BASE)
            .bearer_auth(&self.access_token)
            .query(&[("singleEvents", "true"), ("showDeleted", "true"), ("maxResults", "250")]);
        req = match sync_token {
            Some(tok) => req.query(&[("syncToken", tok)]),
            None => req.query(&[("timeMin", time_min), ("timeMax", time_max)]),
        };
        let resp = req.send().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| CalendarError::Http(e.to_string()))?;
        if !status.is_success() { return Err(Self::classify(status, v.to_string())); }
        let events = v.get("items").and_then(|i| i.as_array()).map(|arr| {
            arr.iter().filter_map(|e| parse_event(e).ok()).collect::<Vec<_>>()
        }).unwrap_or_default();
        let next_sync_token = v.get("nextSyncToken").and_then(|x| x.as_str()).map(String::from);
        Ok(EventPage { events, next_sync_token })
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test google::calendar:: -- --nocapture`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/google/calendar.rs backend/src/google/mod.rs
git commit -m "feat(google): Calendar API client behind a mockable trait"
```

---

## Task 7: Sync reconciler (pure planning functions)

**Files:**
- Create: `backend/src/google/sync.rs`
- Modify: `backend/src/google/mod.rs` (add `pub mod sync;`)

- [ ] **Step 1: Write failing tests for the pure planners**

Create `backend/src/google/sync.rs`:
```rust
//! Pure reconciliation: turn current app + Google state into a list of
//! operations. No DB or network here — the executor (Task 8) runs the ops.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::calendar::GCalEvent;
    use crate::repo::events::EventRow;

    fn local(id: i64, gid: Option<&str>, status: &str) -> EventRow {
        EventRow {
            id, title: "t".into(), location: None, notes: None,
            start_at: "2026-06-13T07:00:00Z".into(), status: status.into(),
            created_at: "2026-06-12T00:00:00+00:00".into(), source: "local".into(),
            google_event_id: gid.map(String::from), google_etag: gid.map(|_| "etag".into()),
            synced_at: gid.map(|_| "2026-06-12T00:00:00+00:00".into()),
            updated_at: Some("2026-06-12T00:00:00+00:00".into()),
        }
    }

    #[test]
    fn outbound_creates_unsynced_patches_synced_deletes_cancelled() {
        let pending = vec![
            local(1, None, "scheduled"),          // never synced -> Create
            local(2, Some("g2"), "scheduled"),    // has id -> Patch
            local(3, Some("g3"), "cancelled"),    // cancelled -> Delete
        ];
        let ops = plan_outbound(&pending);
        assert!(matches!(ops[0], OutboundOp::Create { event_id: 1, .. }));
        assert!(matches!(ops[1], OutboundOp::Patch { event_id: 2, .. }));
        assert!(matches!(ops[2], OutboundOp::Delete { event_id: 3, .. }));
    }

    fn remote(id: &str, app_owned: bool, cancelled: bool, updated: &str) -> GCalEvent {
        GCalEvent {
            id: id.into(), etag: "e".into(), summary: Some("r".into()), location: None,
            description: None, start_rfc3339: Some("2026-06-13T07:00:00Z".into()),
            updated: Some(updated.into()), cancelled, app_owned,
        }
    }

    #[test]
    fn inbound_imports_foreign_as_readonly() {
        let r = remote("gf-1", false, false, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, None);
        assert!(matches!(op, Some(InboundOp::UpsertForeign { .. })));
    }

    #[test]
    fn inbound_removes_deleted_foreign() {
        let existing = local(5, Some("gf-2"), "scheduled");
        let r = remote("gf-2", false, true, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, Some(&existing));
        assert!(matches!(op, Some(InboundOp::RemoveForeign { event_id: 5 })));
    }

    #[test]
    fn inbound_app_owned_newer_in_google_updates_local() {
        // local synced at 08:00, google updated 09:00 -> Google wins.
        let mut existing = local(7, Some("ga-1"), "scheduled");
        existing.synced_at = Some("2026-06-12T08:00:00+00:00".into());
        let r = remote("ga-1", true, false, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, Some(&existing));
        assert!(matches!(op, Some(InboundOp::UpdateLocal { event_id: 7, .. })));
    }

    #[test]
    fn inbound_app_owned_deleted_in_google_cancels_local() {
        let existing = local(8, Some("ga-2"), "scheduled");
        let r = remote("ga-2", true, true, "2026-06-12T09:00:00Z");
        let op = plan_inbound_one(&r, Some(&existing));
        assert!(matches!(op, Some(InboundOp::CancelLocal { event_id: 8 })));
    }

    #[test]
    fn inbound_app_owned_not_newer_is_noop() {
        // local synced at 10:00, google updated 09:00 -> nothing to apply.
        let mut existing = local(9, Some("ga-3"), "scheduled");
        existing.synced_at = Some("2026-06-12T10:00:00+00:00".into());
        let r = remote("ga-3", true, false, "2026-06-12T09:00:00Z");
        assert!(plan_inbound_one(&r, Some(&existing)).is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test google::sync:: -- --nocapture`
Expected: FAIL — ops/functions not defined.

- [ ] **Step 3: Implement the pure planners**

Add `pub mod sync;` to `backend/src/google/mod.rs`, then prepend to `backend/src/google/sync.rs` (above tests):
```rust
use crate::google::calendar::{EventWrite, GCalEvent};
use crate::repo::events::EventRow;

/// Outbound operation derived from a pending-push local row.
pub enum OutboundOp {
    Create { event_id: i64, write: EventWrite },
    Patch { event_id: i64, google_event_id: String, etag: String, write: EventWrite },
    Delete { event_id: i64, google_event_id: String },
}

/// Inbound operation derived from one Google event.
pub enum InboundOp {
    UpsertForeign { google_event_id: String, etag: String, summary: String, location: Option<String>, notes: Option<String>, start_at: String },
    RemoveForeign { event_id: i64 },
    UpdateLocal { event_id: i64, etag: String, summary: String, location: Option<String>, notes: Option<String>, start_at: String },
    CancelLocal { event_id: i64 },
}

fn write_from_local(e: &EventRow) -> EventWrite {
    EventWrite {
        summary: e.title.clone(),
        location: e.location.clone(),
        description: e.notes.clone(),
        start_rfc3339_z: e.start_at.clone(),
    }
}

/// Map each pending-push local row to its outbound op.
pub fn plan_outbound(pending: &[EventRow]) -> Vec<OutboundOp> {
    pending.iter().filter_map(|e| {
        match (&e.google_event_id, e.status.as_str()) {
            (None, "cancelled") => None, // cancelled before it ever synced: nothing to do
            (None, _) => Some(OutboundOp::Create { event_id: e.id, write: write_from_local(e) }),
            (Some(gid), "cancelled") => Some(OutboundOp::Delete { event_id: e.id, google_event_id: gid.clone() }),
            (Some(gid), _) => Some(OutboundOp::Patch {
                event_id: e.id,
                google_event_id: gid.clone(),
                etag: e.google_etag.clone().unwrap_or_default(),
                write: write_from_local(e),
            }),
        }
    }).collect()
}

/// Decide the inbound op for one Google event, given the matching app row (if any).
/// `existing` is looked up by google_event_id by the executor.
pub fn plan_inbound_one(r: &GCalEvent, existing: Option<&EventRow>) -> Option<InboundOp> {
    let start = r.start_rfc3339.clone().unwrap_or_default();
    let summary = r.summary.clone().unwrap_or_else(|| "(untitled)".into());
    match (existing, r.app_owned) {
        // Foreign event (not app-tagged): read-only import / removal.
        (None, false) => {
            if r.cancelled { None } else {
                Some(InboundOp::UpsertForeign {
                    google_event_id: r.id.clone(), etag: r.etag.clone(),
                    summary, location: r.location.clone(), notes: r.description.clone(), start_at: start,
                })
            }
        }
        (Some(row), false) => {
            if r.cancelled { Some(InboundOp::RemoveForeign { event_id: row.id }) }
            else {
                Some(InboundOp::UpsertForeign {
                    google_event_id: r.id.clone(), etag: r.etag.clone(),
                    summary, location: r.location.clone(), notes: r.description.clone(), start_at: start,
                })
            }
        }
        // App-owned event we created: bidirectional, last-write-wins.
        (Some(row), true) => {
            if r.cancelled { return Some(InboundOp::CancelLocal { event_id: row.id }); }
            if google_is_newer(row, r) {
                Some(InboundOp::UpdateLocal {
                    event_id: row.id, etag: r.etag.clone(),
                    summary, location: r.location.clone(), notes: r.description.clone(), start_at: start,
                })
            } else { None }
        }
        // App-tagged in Google but no local row (e.g. deleted locally): ignore.
        (None, true) => None,
    }
}

/// Google wins when its `updated` timestamp is strictly after our last sync.
fn google_is_newer(row: &EventRow, r: &GCalEvent) -> bool {
    let (Some(synced), Some(updated)) = (row.synced_at.as_deref(), r.updated.as_deref()) else {
        return true; // missing data -> prefer applying Google's version
    };
    match (chrono::DateTime::parse_from_rfc3339(synced), chrono::DateTime::parse_from_rfc3339(updated)) {
        (Ok(s), Ok(u)) => u > s,
        _ => true,
    }
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test google::sync:: -- --nocapture`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/google/sync.rs backend/src/google/mod.rs
git commit -m "feat(google): pure two-way reconciler (outbound + inbound planners)"
```

---

## Task 8: Sync executor + spawn loop + token refresh

**Files:**
- Create: `backend/src/google/engine.rs`
- Modify: `backend/src/google/mod.rs` (add `pub mod engine;`)
- Modify: `backend/src/main.rs` (spawn the loop at startup)

- [ ] **Step 1: Write a failing test for the executor against a fake CalendarApi**

Create `backend/src/google/engine.rs`:
```rust
//! Orchestrates one sync pass: execute outbound ops, then apply inbound ops,
//! persisting Google ids/etags and the sync token. Network access goes through
//! the `CalendarApi` trait so this is testable with a fake.

use crate::db::Db;
use crate::google::calendar::{CalendarApi, CalendarError, EventPage, EventWrite, GCalEvent};
use crate::google::sync::{plan_inbound_one, plan_outbound, InboundOp, OutboundOp};
use crate::repo::events;

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeCalendar {
        inserted: Mutex<Vec<String>>,
        page: Mutex<Vec<GCalEvent>>,
    }
    #[async_trait]
    impl CalendarApi for FakeCalendar {
        async fn insert(&self, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
            self.inserted.lock().unwrap().push(w.summary.clone());
            Ok(GCalEvent {
                id: format!("gid-{}", w.summary), etag: "e1".into(), summary: Some(w.summary.clone()),
                location: None, description: None, start_rfc3339: Some(w.start_rfc3339_z.clone()),
                updated: Some("2026-06-12T00:00:00Z".into()), cancelled: false, app_owned: true,
            })
        }
        async fn patch(&self, _id: &str, _etag: &str, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
            Ok(GCalEvent { id: "gid".into(), etag: "e2".into(), summary: Some(w.summary.clone()),
                location: None, description: None, start_rfc3339: Some(w.start_rfc3339_z.clone()),
                updated: Some("2026-06-12T00:00:00Z".into()), cancelled: false, app_owned: true })
        }
        async fn delete(&self, _id: &str) -> Result<(), CalendarError> { Ok(()) }
        async fn list(&self, _t: Option<&str>, _a: &str, _b: &str) -> Result<EventPage, CalendarError> {
            Ok(EventPage { events: self.page.lock().unwrap().clone(), next_sync_token: Some("tok-next".into()) })
        }
    }

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn outbound_creates_then_marks_synced() {
        let db = mem_db().await;
        events::create(&db, "rapat", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        let cal = FakeCalendar::default();
        run_pass(&db, &cal).await.unwrap();
        // The created event now carries a google id and drops out of pending.
        assert_eq!(cal.inserted.lock().unwrap().len(), 1);
        assert!(events::pending_push(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn inbound_imports_foreign_event_readonly() {
        let db = mem_db().await;
        let cal = FakeCalendar::default();
        cal.page.lock().unwrap().push(GCalEvent {
            id: "gf-1".into(), etag: "e".into(), summary: Some("dokter".into()), location: None,
            description: None, start_rfc3339: Some("2026-06-13T03:00:00Z".into()),
            updated: Some("2026-06-12T09:00:00Z".into()), cancelled: false, app_owned: false,
        });
        run_pass(&db, &cal).await.unwrap();
        let row = events::get_by_google_id(&db, "gf-1").await.unwrap().unwrap();
        assert_eq!(row.source, "google");
        assert_eq!(row.title, "dokter");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test google::engine:: -- --nocapture`
Expected: FAIL — `run_pass` not defined.

- [ ] **Step 3: Implement the executor**

Add `pub mod engine;` to `backend/src/google/mod.rs`. Prepend to `backend/src/google/engine.rs` (above tests):
```rust
/// Window for the initial (token-less) inbound list: now .. now + 30 days.
const INBOUND_WINDOW_DAYS: i64 = 30;

/// Execute one outbound+inbound pass with a given client. Pure DB + the trait;
/// no env reads, so tests inject a fake. Returns Ok even if individual items
/// are skipped — see logging.
pub async fn run_pass<C: CalendarApi>(db: &Db, cal: &C) -> anyhow::Result<()> {
    // --- Outbound ---
    for op in plan_outbound(&events::pending_push(db).await?) {
        match op {
            OutboundOp::Create { event_id, write } => match cal.insert(&write).await {
                Ok(ev) => events::mark_synced(db, event_id, &ev.id, &ev.etag).await?,
                Err(e) => tracing::warn!("google insert {event_id} skipped: {e}"),
            },
            OutboundOp::Patch { event_id, google_event_id, etag, write } => {
                match cal.patch(&google_event_id, &etag, &write).await {
                    Ok(ev) => events::mark_synced(db, event_id, &ev.id, &ev.etag).await?,
                    Err(CalendarError::PreconditionFailed) => {
                        tracing::info!("google patch {event_id} lost race; inbound will reconcile");
                    }
                    Err(e) => tracing::warn!("google patch {event_id} skipped: {e}"),
                }
            }
            OutboundOp::Delete { event_id, google_event_id } => match cal.delete(&google_event_id).await {
                Ok(()) => events::mark_synced(db, event_id, &google_event_id, "").await?,
                Err(e) => tracing::warn!("google delete {event_id} skipped: {e}"),
            },
        }
    }

    // --- Inbound ---
    let sync_token = crate::repo::google_integration::get(db).await?
        .and_then(|r| r.calendar_sync_token);
    let now = chrono::Utc::now();
    let time_min = now.to_rfc3339();
    let time_max = (now + chrono::Duration::days(INBOUND_WINDOW_DAYS)).to_rfc3339();

    let page = match cal.list(sync_token.as_deref(), &time_min, &time_max).await {
        Ok(page) => page,
        Err(CalendarError::SyncTokenGone) => {
            crate::repo::google_integration::clear_sync_token(db).await?;
            tracing::info!("google sync token expired; will full-resync next pass");
            return Ok(());
        }
        Err(e) => { tracing::warn!("google list skipped: {e}"); return Ok(()); }
    };

    for r in &page.events {
        let existing = events::get_by_google_id(db, &r.id).await?;
        match plan_inbound_one(r, existing.as_ref()) {
            Some(InboundOp::UpsertForeign { google_event_id, etag, summary, location, notes, start_at }) => {
                events::upsert_foreign(db, &google_event_id, &summary, location.as_deref(), notes.as_deref(), &start_at, &etag).await?;
            }
            Some(InboundOp::RemoveForeign { event_id }) => events::cancel_by_sync(db, event_id).await?,
            Some(InboundOp::UpdateLocal { event_id, etag, summary, location, notes, start_at }) => {
                events::update_from_google(db, event_id, &summary, location.as_deref(), notes.as_deref(), &start_at, &etag).await?;
            }
            Some(InboundOp::CancelLocal { event_id }) => events::cancel_by_sync(db, event_id).await?,
            None => {}
        }
    }

    if let Some(tok) = page.next_sync_token {
        crate::repo::google_integration::set_sync_token(db, &tok).await?;
    }
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cargo test google::engine:: -- --nocapture`
Expected: PASS (2 tests).

- [ ] **Step 5: Add the connected-account driver + spawn loop**

Append to `backend/src/google/engine.rs` (below `run_pass`, above tests):
```rust
use crate::google::oauth::{self, OAuthConfig};

/// Ensure a non-expired access token, refreshing if needed. Returns the
/// plaintext access token, or Err with a reason to record as last_error.
async fn ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8; 32]) -> anyhow::Result<String> {
    let row = crate::repo::google_integration::get(db).await?
        .ok_or_else(|| anyhow::anyhow!("not connected"))?;
    let expired = chrono::DateTime::parse_from_rfc3339(&row.expiry)
        .map(|exp| chrono::Utc::now() >= exp.with_timezone(&chrono::Utc))
        .unwrap_or(true);
    if !expired {
        return crate::google::crypto::decrypt(&row.access_token, key);
    }
    let refresh = crate::google::crypto::decrypt(&row.refresh_token, key)?;
    let tokens = oauth::refresh_access(cfg, &refresh).await?;
    let enc = crate::google::crypto::encrypt(&tokens.access_token, key)?;
    let expiry = oauth::expiry_from_now(tokens.expires_in);
    crate::repo::google_integration::update_access(db, &enc, &expiry).await?;
    Ok(tokens.access_token)
}

/// One full cycle including auth. Sets integration status on failure.
pub async fn run_cycle(db: &Db) -> anyhow::Result<()> {
    let Some(row) = crate::repo::google_integration::get(db).await? else { return Ok(()) };
    if row.status == "disconnected" { return Ok(()); }
    let cfg = OAuthConfig::from_env()?;
    let key = crate::google::crypto::key_from_env()?;
    let token = match ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            crate::repo::google_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(());
        }
    };
    if row.status == "error" {
        crate::repo::google_integration::set_status(db, "connected", None).await?;
    }
    let cal = crate::google::calendar::HttpCalendar::new(token);
    run_pass(db, &cal).await
}

const TICK: std::time::Duration = std::time::Duration::from_secs(300);

/// Spawn the independent 5-minute Google sync loop (no Telegram dependency).
/// No-op when OAuth env is unconfigured.
pub fn spawn(db: Db) {
    if OAuthConfig::from_env().is_err() {
        tracing::info!("GOOGLE_CLIENT_* not set; calendar sync disabled");
        return;
    }
    tokio::spawn(async move {
        loop {
            if let Err(e) = run_cycle(&db).await {
                tracing::warn!("google sync cycle failed: {e:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}
```

- [ ] **Step 6: Spawn at startup**

In `backend/src/main.rs`, find where `crate::assistant::proactive::tick::spawn(...)` (or the proactive spawn) is called during startup and add immediately after it:
```rust
    crate::google::engine::spawn(state.db.clone());
```
(If the proactive spawn uses a bare `db` binding rather than `state.db`, mirror that — use the same `Db` handle that is in scope.)

- [ ] **Step 7: Verify build + full suite**

Run: `cargo test 2>&1 | tail -5`
Expected: PASS, no new warnings. Confirms `spawn` wiring compiles.

- [ ] **Step 8: Commit**

```bash
git add backend/src/google/engine.rs backend/src/google/mod.rs backend/src/main.rs
git commit -m "feat(google): sync executor, token refresh, independent 5-min loop"
```

---

## Task 9: API handlers + routes

**Files:**
- Create: `backend/src/api/google.rs`
- Modify: `backend/src/api/mod.rs` (declare module + register routes)

- [ ] **Step 1: Write a failing route test (callback is public, start is protected)**

Add to the `router_tests` module in `backend/src/api/mod.rs`:
```rust
    #[serial]
    #[tokio::test]
    async fn google_start_is_protected_but_callback_is_public() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-google");

        let app = router(test_state().await);
        // start requires auth
        let res = app.clone().oneshot(
            Request::builder().uri("/google/oauth/start").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        // callback is reachable without a token (it will 400 on missing params,
        // but must NOT be 401).
        let res = app.oneshot(
            Request::builder().uri("/google/oauth/callback").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test router_tests::google -- --nocapture`
Expected: FAIL — routes not registered.

- [ ] **Step 3: Implement handlers**

Create `backend/src/api/google.rs`:
```rust
use crate::error::AppError;
use crate::AppState;
use axum::{extract::{Query, State}, response::Redirect, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct StartOut { pub consent_url: String }

/// Build the Google consent URL (frontend redirects the browser to it).
pub async fn start() -> Result<Json<StartOut>, AppError> {
    let cfg = crate::google::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("google not configured: {e}")))?;
    let secret = crate::auth::jwt_secret()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("JWT_SECRET not set")))?;
    let now = chrono::Utc::now().timestamp();
    let state = crate::google::oauth::sign_state_with(&secret, now).map_err(AppError::Other)?;
    let consent_url = crate::google::oauth::consent_url(&cfg.client_id, &cfg.redirect_uri, &state);
    Ok(Json(StartOut { consent_url }))
}

#[derive(Deserialize)]
pub struct CallbackQuery { pub code: Option<String>, pub state: Option<String> }

/// Public OAuth callback. Guarded by the signed `state` (not JWT) since Google
/// redirects the browser here without the SPA's token. Redirects to /settings.
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
    let cfg = crate::google::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("google not configured: {e}")))?;
    let key = crate::google::crypto::key_from_env().map_err(AppError::Other)?;
    let tokens = crate::google::oauth::exchange_code(&cfg, &code).await.map_err(AppError::Other)?;
    let refresh = tokens.refresh_token.clone()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("no refresh_token returned; re-consent required")))?;
    let enc_access = crate::google::crypto::encrypt(&tokens.access_token, &key).map_err(AppError::Other)?;
    let enc_refresh = crate::google::crypto::encrypt(&refresh, &key).map_err(AppError::Other)?;
    let expiry = crate::google::oauth::expiry_from_now(tokens.expires_in);
    let scope = tokens.scope.unwrap_or_else(|| crate::google::oauth::SCOPE.to_string());
    crate::repo::google_integration::upsert(&s.db, &enc_access, &enc_refresh, &expiry, &scope)
        .await.map_err(AppError::Other)?;
    Ok(Redirect::to("/settings?google=connected"))
}

#[derive(Serialize)]
pub struct StatusOut { pub status: String, pub last_error: Option<String> }

/// Connection status for the Settings UI.
pub async fn status(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    let row = crate::repo::google_integration::get(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(match row {
        Some(r) => StatusOut { status: r.status, last_error: r.last_error },
        None => StatusOut { status: "disconnected".into(), last_error: None },
    }))
}

/// Revoke + delete the connection.
pub async fn disconnect(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    if let Some(row) = crate::repo::google_integration::get(&s.db).await.map_err(AppError::Other)? {
        if let Ok(key) = crate::google::crypto::key_from_env() {
            if let Ok(access) = crate::google::crypto::decrypt(&row.access_token, &key) {
                let _ = crate::google::oauth::revoke(&access).await; // best effort
            }
        }
    }
    crate::repo::google_integration::delete(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(StatusOut { status: "disconnected".into(), last_error: None }))
}
```

- [ ] **Step 4: Register module + routes**

Add to the module list at the top of `backend/src/api/mod.rs`:
```rust
pub mod google;
```

Add the callback to the `public` router (after the `/auth/login` line):
```rust
        .route("/google/oauth/callback", get(google::callback));
```

Add the protected routes to the `protected` router (after the telegram routes, before `/accounts`):
```rust
        .route("/google/oauth/start", get(google::start))
        .route("/google/status", get(google::status))
        .route("/google/disconnect", post(google::disconnect))
```

- [ ] **Step 5: Run to verify pass**

Run: `cargo test router_tests:: -- --nocapture`
Expected: PASS (existing router tests + the new google route test).

- [ ] **Step 6: Commit**

```bash
git add backend/src/api/google.rs backend/src/api/mod.rs
git commit -m "feat(google): OAuth start/callback/status/disconnect endpoints"
```

---

## Task 10: Frontend — Google Calendar card in Settings

**Files:**
- Modify: `frontend/src/api/client.ts` (add google methods)
- Modify: `frontend/src/pages/SettingsPage.tsx` (render the card)
- Create: `frontend/src/components/GoogleCalendarCard.tsx`
- Create: `frontend/src/components/GoogleCalendarCard.test.tsx`

- [ ] **Step 1: Inspect the existing patterns to mirror**

Run:
```bash
cd frontend && sed -n '1,60p' src/api/client.ts && echo "---telegram card pattern---" && sed -n '1,80p' src/pages/TelegramPage.tsx
```
Expected: shows the `api` object shape, the zod-validated `request` helper, and how TelegramPage renders status + an action button. Mirror these (same `request`, same component/styling conventions). Use the existing UI primitives from `src/components/ui`.

- [ ] **Step 2: Add API client methods**

In `frontend/src/api/client.ts`, add a zod schema near the others and methods inside the exported `api` object:
```ts
// near the other schemas
const googleStatusSchema = z.object({
  status: z.enum(["connected", "disconnected", "error"]),
  last_error: z.string().nullable(),
});
const googleStartSchema = z.object({ consent_url: z.string() });

// inside `export const api = { ... }`
  googleStatus: () => request("/google/status", googleStatusSchema),
  googleStart: () => request("/google/oauth/start", googleStartSchema),
  googleDisconnect: () =>
    request("/google/disconnect", googleStatusSchema, { method: "POST" }),
```

- [ ] **Step 3: Write the component test (failing)**

Create `frontend/src/components/GoogleCalendarCard.test.tsx`:
```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import GoogleCalendarCard from "./GoogleCalendarCard";
import { api } from "../api/client";

vi.mock("../api/client", () => ({
  api: { googleStatus: vi.fn(), googleStart: vi.fn(), googleDisconnect: vi.fn() },
}));

describe("GoogleCalendarCard", () => {
  it("shows Connect when disconnected", async () => {
    (api.googleStatus as any).mockResolvedValue({ status: "disconnected", last_error: null });
    render(<GoogleCalendarCard />);
    await waitFor(() => expect(screen.getByRole("button", { name: /hubungkan/i })).toBeInTheDocument());
  });

  it("shows the error reason when status is error", async () => {
    (api.googleStatus as any).mockResolvedValue({ status: "error", last_error: "invalid_grant" });
    render(<GoogleCalendarCard />);
    await waitFor(() => expect(screen.getByText(/invalid_grant/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 4: Run to verify failure**

Run: `cd frontend && npx vitest run src/components/GoogleCalendarCard.test.tsx`
Expected: FAIL — component file does not exist.

- [ ] **Step 5: Implement the component**

Create `frontend/src/components/GoogleCalendarCard.tsx` (mirror TelegramPage's card markup + the UI primitives it imports; adjust import paths to match what Step 1 revealed):
```tsx
import { useEffect, useState } from "react";
import { api } from "../api/client";

type Status = "connected" | "disconnected" | "error" | "loading";

export default function GoogleCalendarCard() {
  const [status, setStatus] = useState<Status>("loading");
  const [error, setError] = useState<string | null>(null);

  async function refresh() {
    try {
      const s = await api.googleStatus();
      setStatus(s.status);
      setError(s.last_error);
    } catch {
      setStatus("disconnected");
    }
  }

  useEffect(() => { refresh(); }, []);

  async function connect() {
    const { consent_url } = await api.googleStart();
    window.location.href = consent_url; // top-level nav to Google
  }

  async function disconnect() {
    await api.googleDisconnect();
    await refresh();
  }

  return (
    <section className="rounded-lg border p-4 space-y-2">
      <h3 className="font-medium">Google Calendar</h3>
      <p className="text-sm text-muted-foreground">
        Sinkronkan agenda asisten dua arah dengan Google Calendar utamamu.
      </p>
      {status === "loading" && <p className="text-sm">Memuat…</p>}
      {status === "connected" && <p className="text-sm text-green-600">Terhubung ✓</p>}
      {status === "error" && (
        <p className="text-sm text-red-600">Bermasalah{error ? `: ${error}` : ""} — hubungkan ulang.</p>
      )}
      {(status === "disconnected" || status === "error") && (
        <button onClick={connect} className="rounded-md border px-3 py-1.5 text-sm">
          Hubungkan Google
        </button>
      )}
      {(status === "connected" || status === "error") && (
        <button onClick={disconnect} className="rounded-md border px-3 py-1.5 text-sm ml-2">
          Putuskan
        </button>
      )}
    </section>
  );
}
```

- [ ] **Step 6: Mount it in Settings**

In `frontend/src/pages/SettingsPage.tsx`, import and render the card within the existing settings layout:
```tsx
import GoogleCalendarCard from "../components/GoogleCalendarCard";
// ...inside the page's section list/grid:
<GoogleCalendarCard />
```

- [ ] **Step 7: Run to verify pass**

Run: `cd frontend && npx vitest run src/components/GoogleCalendarCard.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 8: Commit**

```bash
git add frontend/src/api/client.ts frontend/src/pages/SettingsPage.tsx frontend/src/components/GoogleCalendarCard.tsx frontend/src/components/GoogleCalendarCard.test.tsx
git commit -m "feat(google): Settings card to connect/disconnect Google Calendar"
```

---

## Task 11: Config + deploy wiring + docs

**Files:**
- Modify: `backend/.env.example`, `.env.production.example`
- Modify: `docker-compose.yml`, `docker-compose.prod.yml`
- Modify: `k8s/10-backend.yaml`, `k8s/secret.example.yaml`
- Modify: `docs/superpowers/specs/2026-06-12-google-calendar-sync-design.md` (none — already documents setup)

- [ ] **Step 1: Add env docs**

In `backend/.env.example` add:
```bash
# --- Google Calendar sync (optional; leave unset to disable) ---
# OAuth client from Google Cloud Console (Web application).
GOOGLE_CLIENT_ID=
GOOGLE_CLIENT_SECRET=
# Must exactly match the redirect URI registered in Google Cloud Console.
GOOGLE_REDIRECT_URI=https://portfolio.catalystlabs.id/api/google/oauth/callback
# base64 of 32 random bytes: `openssl rand -base64 32`. Required to connect.
GOOGLE_TOKEN_ENC_KEY=
```

Add the same block to `.env.production.example`.

- [ ] **Step 2: Wire docker-compose (both files)**

In the backend `environment:` block of `docker-compose.yml` and `docker-compose.prod.yml`, after the existing keys:
```yaml
      GOOGLE_CLIENT_ID: ${GOOGLE_CLIENT_ID:-}
      GOOGLE_CLIENT_SECRET: ${GOOGLE_CLIENT_SECRET:-}
      GOOGLE_REDIRECT_URI: ${GOOGLE_REDIRECT_URI:-}
      GOOGLE_TOKEN_ENC_KEY: ${GOOGLE_TOKEN_ENC_KEY:-}
```

- [ ] **Step 3: Wire k8s**

In `k8s/10-backend.yaml`, add to the backend container `env:` (secret-backed, mirroring `ANTHROPIC_API_KEY`):
```yaml
            - name: GOOGLE_REDIRECT_URI
              value: "https://portfolio.catalystlabs.id/api/google/oauth/callback"
            - name: GOOGLE_CLIENT_ID
              valueFrom:
                secretKeyRef: { name: portfolio-secrets, key: GOOGLE_CLIENT_ID }
            - name: GOOGLE_CLIENT_SECRET
              valueFrom:
                secretKeyRef: { name: portfolio-secrets, key: GOOGLE_CLIENT_SECRET }
            - name: GOOGLE_TOKEN_ENC_KEY
              valueFrom:
                secretKeyRef: { name: portfolio-secrets, key: GOOGLE_TOKEN_ENC_KEY }
```

In `k8s/secret.example.yaml`, add under `stringData:`:
```yaml
  GOOGLE_CLIENT_ID: "REPLACE_ME"
  GOOGLE_CLIENT_SECRET: "REPLACE_ME"
  GOOGLE_TOKEN_ENC_KEY: "REPLACE_ME_base64_32_bytes"
```

- [ ] **Step 4: Validate YAML + full backend build/test**

Run:
```bash
cd backend && cargo test 2>&1 | tail -5
cd .. && for f in docker-compose.yml docker-compose.prod.yml k8s/10-backend.yaml k8s/secret.example.yaml; do python3 -c "import yaml; list(yaml.safe_load_all(open('$f'))); print('ok: $f')"; done
```
Expected: all backend tests PASS; all YAML `ok`.

- [ ] **Step 5: Commit**

```bash
git add backend/.env.example .env.production.example docker-compose.yml docker-compose.prod.yml k8s/10-backend.yaml k8s/secret.example.yaml
git commit -m "feat(google): env + compose + k8s wiring for calendar sync"
```

---

## Task 12: End-to-end live smoke test (manual, `#[ignore]`)

**Files:**
- Modify: `backend/src/google/engine.rs` (add an ignored live test)

- [ ] **Step 1: Add a gated live test**

Append to the `tests` module in `backend/src/google/engine.rs`:
```rust
    /// Live round-trip against a real Google account. Requires GOOGLE_CLIENT_ID,
    /// GOOGLE_CLIENT_SECRET, GOOGLE_REDIRECT_URI, GOOGLE_TOKEN_ENC_KEY, and an
    /// already-connected google_integration row in a file DB at GOOGLE_SMOKE_DB.
    /// Run: GOOGLE_SMOKE_DB=sqlite:///tmp/smoke.db cargo test google::engine::tests::live_cycle -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_cycle() {
        let url = match std::env::var("GOOGLE_SMOKE_DB") { Ok(u) => u, Err(_) => return };
        let db = crate::db::connect(&url).await.unwrap();
        run_cycle(&db).await.unwrap();
        let row = crate::repo::google_integration::get(&db).await.unwrap().unwrap();
        assert_eq!(row.status, "connected", "last_error={:?}", row.last_error);
    }
```

- [ ] **Step 2: Verify it compiles (and is skipped by default)**

Run: `cargo test google::engine 2>&1 | tail -5`
Expected: PASS; the live test shows as ignored.

- [ ] **Step 3: Commit**

```bash
git add backend/src/google/engine.rs
git commit -m "test(google): gated live calendar sync smoke test"
```

---

## Manual Deploy Steps (after merge — NOT in CI/CD)

CI/CD only builds the image + `set image`. As with the DeepSeek migration, the secret and manifest must be applied manually:

1. **Google Cloud Console:** create project → enable Calendar API → OAuth consent screen (External; add the owner as a test user) → create OAuth Client (Web) → redirect URI `https://portfolio.catalystlabs.id/api/google/oauth/callback`.
2. **Generate the encryption key:** `openssl rand -base64 32`.
3. **Patch the secret:**
   ```bash
   export KUBECONFIG=~/.kube/config-remote
   kubectl -n portfolio patch secret portfolio-secrets --type merge -p '{"stringData":{
     "GOOGLE_CLIENT_ID":"...","GOOGLE_CLIENT_SECRET":"...","GOOGLE_TOKEN_ENC_KEY":"<base64-32>"}}'
   ```
4. **Apply manifest + rollout:**
   ```bash
   kubectl apply -f k8s/10-backend.yaml
   kubectl -n portfolio rollout restart deploy/backend
   kubectl -n portfolio rollout status deploy/backend
   ```
5. **Connect:** open the app → Settings → Google Calendar → Hubungkan → authorize. Then create an event via chat and confirm it appears in Google Calendar within ~5 min, and a Google-created event appears in the next briefing.

---

## Self-Review Notes

- **Spec coverage:** OAuth (Task 5, 9) · token encryption fail-closed (Task 3) · `google_integration` table + events columns (Task 1) · ownership tagging `extendedProperties.private.app` (Task 6) · outbound create/patch/delete with If-Match (Task 6, 8) · inbound incremental with syncToken + 30-day window + 410 handling (Task 8) · foreign read-only import (Task 7, 8) · last-write-wins (Task 7) · public state-guarded callback vs protected start (Task 9) · independent tick loop (Task 8) · failure→status=error + UI banner (Task 8, 9, 10) · manual setup + env (Task 11). All spec sections map to a task.
- **Out of scope (unchanged):** reminders, todos→Tasks, webhooks, recurrence, multi-account.
- **Type consistency:** `EventRow` fields (Task 1) are consumed identically in Tasks 2/7/8; `OutboundOp`/`InboundOp` defined in Task 7 are matched exhaustively in Task 8; `CalendarApi` (Task 6) is the only network surface used by the executor (Task 8) and faked in tests.
