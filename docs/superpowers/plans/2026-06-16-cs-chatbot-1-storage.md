# CS Chatbot — Plan 1: Storage Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create the isolated `cs_*` SQLite schema and a `repo/cs.rs` data-access layer (conversations, messages, KB docs/chunks, products, orders, escalations) with full unit-test coverage.

**Architecture:** One new migration (`0023_cs_core.sql`) adds six tables, all prefixed `cs_` and fully separate from the owner-only `chat_message` table. A new `repo/cs.rs` module follows the existing repo convention exactly: `sqlx::FromRow` row structs, runtime `sqlx::query`/`query_as` (no compile-time macros), `anyhow::Result` returns, RFC3339 `chrono::Utc` timestamps, and `#[cfg(test)]` tests against an in-memory DB (`sqlite::memory:`). Embedding vectors are stored as little-endian `f32` BLOBs with encode/decode helpers living in this module.

**Tech Stack:** Rust, axum, sqlx (SQLite), chrono, anyhow.

---

## File Structure

- Create: `backend/migrations/0023_cs_core.sql` — the six `cs_*` tables + indexes.
- Create: `backend/src/repo/cs.rs` — row structs, BLOB helpers, and all data-access functions.
- Modify: `backend/src/repo/mod.rs:25` — register `pub mod cs;`.

> **Note on the backend style convention:** do NOT run `cargo fmt`. Verify with `cargo test` and `cargo clippy`. Match the surrounding hand-maintained layout.

---

## Task 1: Migration — the `cs_*` schema

**Files:**
- Create: `backend/migrations/0023_cs_core.sql`
- Modify: `backend/src/repo/mod.rs` (add `pub mod cs;`)
- Create: `backend/src/repo/cs.rs` (empty stub so the module compiles)

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0023_cs_core.sql`:

```sql
-- CS chatbot (Phase 1): isolated customer-service tables. Fully separate from the
-- owner-only chat_message table — the CS agent must never touch owner data.

CREATE TABLE cs_conversation (
  id            INTEGER PRIMARY KEY AUTOINCREMENT,
  channel       TEXT NOT NULL DEFAULT 'web' CHECK (channel IN ('web', 'whatsapp')),
  visitor_name  TEXT,
  visitor_email TEXT,
  visitor_phone TEXT,
  session_token TEXT NOT NULL UNIQUE,
  status        TEXT NOT NULL DEFAULT 'bot'
    CHECK (status IN ('bot', 'needs_human', 'resolved')),
  created_at    TEXT NOT NULL,
  last_msg_at   TEXT NOT NULL
);
CREATE INDEX idx_cs_conversation_token  ON cs_conversation (session_token);
CREATE INDEX idx_cs_conversation_status ON cs_conversation (status, id);

CREATE TABLE cs_message (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL REFERENCES cs_conversation (id) ON DELETE CASCADE,
  role            TEXT NOT NULL CHECK (role IN ('user', 'assistant', 'system')),
  content         TEXT NOT NULL,
  created_at      TEXT NOT NULL
);
CREATE INDEX idx_cs_message_conv ON cs_message (conversation_id, id);

CREATE TABLE cs_kb_doc (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  title      TEXT NOT NULL,
  source     TEXT,
  body       TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE cs_kb_chunk (
  id         INTEGER PRIMARY KEY AUTOINCREMENT,
  doc_id     INTEGER NOT NULL REFERENCES cs_kb_doc (id) ON DELETE CASCADE,
  text       TEXT NOT NULL,
  embedding  BLOB,                 -- little-endian f32 vector; NULL until embedded
  updated_at TEXT NOT NULL
);
CREATE INDEX idx_cs_kb_chunk_doc ON cs_kb_chunk (doc_id);

CREATE TABLE cs_product (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  name        TEXT NOT NULL,
  description TEXT,
  price       REAL,
  currency    TEXT,
  availability TEXT,
  active      INTEGER NOT NULL DEFAULT 1,
  updated_at  TEXT NOT NULL
);

CREATE TABLE cs_order (
  id               INTEGER PRIMARY KEY AUTOINCREMENT,
  external_ref     TEXT NOT NULL UNIQUE,
  customer_name    TEXT,
  customer_contact TEXT,
  status           TEXT NOT NULL,
  details_json     TEXT,
  updated_at       TEXT NOT NULL
);

CREATE TABLE cs_escalation (
  id              INTEGER PRIMARY KEY AUTOINCREMENT,
  conversation_id INTEGER NOT NULL REFERENCES cs_conversation (id) ON DELETE CASCADE,
  reason          TEXT NOT NULL,
  summary         TEXT NOT NULL,
  status          TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'handled')),
  created_at      TEXT NOT NULL,
  handled_at      TEXT
);
CREATE INDEX idx_cs_escalation_open ON cs_escalation (status, id);
```

- [ ] **Step 2: Create an empty repo stub and register the module**

Create `backend/src/repo/cs.rs` with just:

```rust
use crate::db::Db;
use serde::Serialize;
```

Add to `backend/src/repo/mod.rs` after line 25 (`pub mod news;`):

```rust
pub mod cs;
```

- [ ] **Step 3: Write a migration-applies test**

Append to `backend/src/repo/cs.rs`:

```rust
#[cfg(test)]
mod tests {
    use crate::db::Db;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn migration_creates_cs_tables() {
        let db = mem_db().await;
        // If the migration applied, this query against an empty table succeeds.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cs_conversation")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test --lib repo::cs::tests::migration_creates_cs_tables`
Expected: PASS (migrations run on `db::connect`, table exists).

- [ ] **Step 5: Commit**

```bash
git add backend/migrations/0023_cs_core.sql backend/src/repo/cs.rs backend/src/repo/mod.rs
git commit -m "feat(cs): add cs_* schema and repo module stub"
```

---

## Task 2: Conversation repo

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add the `ConversationRow` struct and functions' tests. Put the struct + functions above the existing `mod tests`, and add these test fns inside `mod tests`:

```rust
// --- inside mod tests, add: ---
use super::*;

#[tokio::test]
async fn create_and_fetch_conversation_by_token() {
    let db = mem_db().await;
    let conv = conversation_create(&db, "web", Some("Budi"), Some("budi@mail.com"), None, "tok-abc")
        .await
        .unwrap();
    assert_eq!(conv.channel, "web");
    assert_eq!(conv.status, "bot");
    assert_eq!(conv.visitor_name.as_deref(), Some("Budi"));

    let found = conversation_by_token(&db, "tok-abc").await.unwrap();
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, conv.id);

    let missing = conversation_by_token(&db, "nope").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn set_status_and_touch_update_row() {
    let db = mem_db().await;
    let conv = conversation_create(&db, "web", None, None, None, "tok-1").await.unwrap();

    conversation_set_status(&db, conv.id, "needs_human").await.unwrap();
    let after = conversation_by_token(&db, "tok-1").await.unwrap().unwrap();
    assert_eq!(after.status, "needs_human");

    conversation_touch(&db, conv.id).await.unwrap();
    let touched = conversation_by_token(&db, "tok-1").await.unwrap().unwrap();
    assert!(touched.last_msg_at >= conv.last_msg_at);
}

#[tokio::test]
async fn invalid_status_rejected() {
    let db = mem_db().await;
    let conv = conversation_create(&db, "web", None, None, None, "tok-2").await.unwrap();
    let res = conversation_set_status(&db, conv.id, "bogus").await;
    assert!(res.is_err());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test --lib repo::cs::tests::create_and_fetch_conversation_by_token`
Expected: FAIL — `conversation_create` not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs` (after the `use` lines):

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: i64,
    pub channel: String,
    pub visitor_name: Option<String>,
    pub visitor_email: Option<String>,
    pub visitor_phone: Option<String>,
    pub session_token: String,
    pub status: String,
    pub created_at: String,
    pub last_msg_at: String,
}

pub async fn conversation_create(
    db: &Db,
    channel: &str,
    name: Option<&str>,
    email: Option<&str>,
    phone: Option<&str>,
    session_token: &str,
) -> anyhow::Result<ConversationRow> {
    if !matches!(channel, "web" | "whatsapp") {
        anyhow::bail!("invalid channel '{}': must be 'web' or 'whatsapp'", channel);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_conversation \
         (channel, visitor_name, visitor_email, visitor_phone, session_token, status, created_at, last_msg_at) \
         VALUES (?, ?, ?, ?, ?, 'bot', ?, ?)",
    )
    .bind(channel)
    .bind(name)
    .bind(email)
    .bind(phone)
    .bind(session_token)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, ConversationRow>("SELECT * FROM cs_conversation WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn conversation_by_token(db: &Db, token: &str) -> anyhow::Result<Option<ConversationRow>> {
    let row = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation WHERE session_token = ?",
    )
    .bind(token)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

pub async fn conversation_set_status(db: &Db, id: i64, status: &str) -> anyhow::Result<()> {
    if !matches!(status, "bot" | "needs_human" | "resolved") {
        anyhow::bail!("invalid status '{}'", status);
    }
    sqlx::query("UPDATE cs_conversation SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn conversation_touch(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE cs_conversation SET last_msg_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// Conversations most-recently-active first — for the admin CS inbox.
pub async fn conversation_list_recent(db: &Db, limit: i64) -> anyhow::Result<Vec<ConversationRow>> {
    let rows = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation ORDER BY last_msg_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib repo::cs::tests::`
Expected: PASS for the three conversation tests + the migration test.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): conversation repo (create/lookup/status/touch)"
```

---

## Task 3: Message repo

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
#[tokio::test]
async fn add_messages_and_fetch_in_order() {
    let db = mem_db().await;
    let conv = conversation_create(&db, "web", None, None, None, "tok-m").await.unwrap();

    message_add(&db, conv.id, "user", "halo").await.unwrap();
    message_add(&db, conv.id, "assistant", "halo juga").await.unwrap();
    message_add(&db, conv.id, "user", "harga berapa?").await.unwrap();

    let all = message_all(&db, conv.id).await.unwrap();
    let contents: Vec<&str> = all.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(contents, vec!["halo", "halo juga", "harga berapa?"]);

    let recent = message_recent(&db, conv.id, 2).await.unwrap();
    let recent_contents: Vec<&str> = recent.iter().map(|m| m.content.as_str()).collect();
    assert_eq!(recent_contents, vec!["halo juga", "harga berapa?"]); // last 2, oldest first
}

#[tokio::test]
async fn message_invalid_role_rejected() {
    let db = mem_db().await;
    let conv = conversation_create(&db, "web", None, None, None, "tok-mr").await.unwrap();
    let res = message_add(&db, conv.id, "robot", "x").await;
    assert!(res.is_err());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test --lib repo::cs::tests::add_messages_and_fetch_in_order`
Expected: FAIL — `message_add` not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs`:

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MessageRow {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub async fn message_add(db: &Db, conversation_id: i64, role: &str, content: &str) -> anyhow::Result<MessageRow> {
    if !matches!(role, "user" | "assistant" | "system") {
        anyhow::bail!("invalid role '{}'", role);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_message (conversation_id, role, content, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, MessageRow>("SELECT * FROM cs_message WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Full transcript for one conversation, oldest first.
pub async fn message_all(db: &Db, conversation_id: i64) -> anyhow::Result<Vec<MessageRow>> {
    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT * FROM cs_message WHERE conversation_id = ? ORDER BY id ASC",
    )
    .bind(conversation_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Last `limit` messages for one conversation, oldest first — LLM context window.
pub async fn message_recent(db: &Db, conversation_id: i64, limit: i64) -> anyhow::Result<Vec<MessageRow>> {
    let mut rows = sqlx::query_as::<_, MessageRow>(
        "SELECT * FROM cs_message WHERE conversation_id = ? ORDER BY id DESC LIMIT ?",
    )
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(db)
    .await?;
    rows.reverse();
    Ok(rows)
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib repo::cs::tests::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): message repo (add/all/recent)"
```

---

## Task 4: KB doc + chunk repo (with embedding BLOB helpers)

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
#[tokio::test]
async fn blob_roundtrip_preserves_vector() {
    let v = vec![0.0f32, 1.5, -2.25, 3.125];
    let blob = embedding_to_blob(&v);
    let back = blob_to_embedding(&blob);
    assert_eq!(v, back);
}

#[tokio::test]
async fn kb_doc_and_chunks_lifecycle() {
    let db = mem_db().await;
    let doc_id = kb_doc_insert(&db, "Refund policy", Some("faq"), "You can refund within 7 days.")
        .await
        .unwrap();

    // replace_chunks inserts fresh chunks with NULL embedding
    kb_replace_chunks(&db, doc_id, &["chunk one".into(), "chunk two".into()]).await.unwrap();
    let pending = kb_chunks_without_embedding(&db).await.unwrap();
    assert_eq!(pending.len(), 2);

    // set an embedding on the first chunk
    let first_id = pending[0].0;
    kb_set_chunk_embedding(&db, first_id, &embedding_to_blob(&[0.1, 0.2, 0.3])).await.unwrap();

    let pending_after = kb_chunks_without_embedding(&db).await.unwrap();
    assert_eq!(pending_after.len(), 1);

    let embedded = kb_chunks_with_embedding(&db).await.unwrap();
    assert_eq!(embedded.len(), 1);
    assert_eq!(embedded[0].doc_id, doc_id);
    assert_eq!(embedded[0].vector, vec![0.1f32, 0.2, 0.3]);

    // replacing chunks clears the old ones
    kb_replace_chunks(&db, doc_id, &["only one now".into()]).await.unwrap();
    assert_eq!(kb_chunks_without_embedding(&db).await.unwrap().len(), 1);
    assert_eq!(kb_chunks_with_embedding(&db).await.unwrap().len(), 0);
}

#[tokio::test]
async fn kb_doc_delete_cascades_chunks() {
    let db = mem_db().await;
    let doc_id = kb_doc_insert(&db, "Doc", None, "body").await.unwrap();
    kb_replace_chunks(&db, doc_id, &["a".into()]).await.unwrap();
    kb_doc_delete(&db, doc_id).await.unwrap();
    assert_eq!(kb_chunks_without_embedding(&db).await.unwrap().len(), 0);
    assert!(kb_doc_list(&db).await.unwrap().is_empty());
}
```

> **Note:** the cascade test relies on `ON DELETE CASCADE`, which requires SQLite foreign keys to be ON. The existing `db::connect` enables `PRAGMA foreign_keys`. If the test shows orphaned chunks, the implementation must delete chunks explicitly in `kb_doc_delete` — see the implementation below, which does both for safety.

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test --lib repo::cs::tests::kb_doc_and_chunks_lifecycle`
Expected: FAIL — helpers/functions not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs`:

```rust
/// Encode an f32 vector as little-endian bytes for BLOB storage.
pub fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Decode a little-endian f32 BLOB back into a vector. Trailing bytes (not a
/// multiple of 4) are ignored.
pub fn blob_to_embedding(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct KbDocRow {
    pub id: i64,
    pub title: String,
    pub source: Option<String>,
    pub body: String,
    pub updated_at: String,
}

/// A chunk + decoded embedding, ready for similarity search.
pub struct KbChunkVec {
    pub id: i64,
    pub doc_id: i64,
    pub text: String,
    pub vector: Vec<f32>,
}

pub async fn kb_doc_insert(db: &Db, title: &str, source: Option<&str>, body: &str) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query("INSERT INTO cs_kb_doc (title, source, body, updated_at) VALUES (?, ?, ?, ?)")
        .bind(title)
        .bind(source)
        .bind(body)
        .bind(&now)
        .execute(db)
        .await?
        .last_insert_rowid();
    Ok(id)
}

pub async fn kb_doc_update(db: &Db, id: i64, title: &str, source: Option<&str>, body: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE cs_kb_doc SET title = ?, source = ?, body = ?, updated_at = ? WHERE id = ?")
        .bind(title)
        .bind(source)
        .bind(body)
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn kb_doc_delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // Explicit chunk delete first, so the test passes regardless of PRAGMA state.
    sqlx::query("DELETE FROM cs_kb_chunk WHERE doc_id = ?").bind(id).execute(db).await?;
    sqlx::query("DELETE FROM cs_kb_doc WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

pub async fn kb_doc_list(db: &Db) -> anyhow::Result<Vec<KbDocRow>> {
    let rows = sqlx::query_as::<_, KbDocRow>("SELECT * FROM cs_kb_doc ORDER BY updated_at DESC")
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Replace all chunks for a doc: delete existing, insert fresh ones with NULL embedding.
pub async fn kb_replace_chunks(db: &Db, doc_id: i64, texts: &[String]) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM cs_kb_chunk WHERE doc_id = ?").bind(doc_id).execute(db).await?;
    for text in texts {
        sqlx::query("INSERT INTO cs_kb_chunk (doc_id, text, embedding, updated_at) VALUES (?, ?, NULL, ?)")
            .bind(doc_id)
            .bind(text)
            .bind(&now)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Chunks that still need an embedding computed. Returns (chunk_id, text).
pub async fn kb_chunks_without_embedding(db: &Db) -> anyhow::Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, text FROM cs_kb_chunk WHERE embedding IS NULL ORDER BY id ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn kb_set_chunk_embedding(db: &Db, chunk_id: i64, blob: &[u8]) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE cs_kb_chunk SET embedding = ?, updated_at = ? WHERE id = ?")
        .bind(blob)
        .bind(&now)
        .bind(chunk_id)
        .execute(db)
        .await?;
    Ok(())
}

/// All chunks that have an embedding, decoded for cosine search.
pub async fn kb_chunks_with_embedding(db: &Db) -> anyhow::Result<Vec<KbChunkVec>> {
    let rows: Vec<(i64, i64, String, Vec<u8>)> = sqlx::query_as(
        "SELECT id, doc_id, text, embedding FROM cs_kb_chunk WHERE embedding IS NOT NULL ORDER BY id ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, doc_id, text, blob)| KbChunkVec {
            id,
            doc_id,
            text,
            vector: blob_to_embedding(&blob),
        })
        .collect())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib repo::cs::tests::`
Expected: PASS (blob roundtrip, lifecycle, cascade).

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): KB doc/chunk repo + embedding BLOB helpers"
```

---

## Task 5: Product (pricing) repo

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
#[tokio::test]
async fn product_insert_list_active_and_deactivate() {
    let db = mem_db().await;
    let id = product_insert(&db, "Paket A", Some("Basic"), Some(150000.0), Some("IDR"), Some("ready"))
        .await
        .unwrap();
    product_insert(&db, "Paket B", None, Some(300000.0), Some("IDR"), Some("ready")).await.unwrap();

    let active = product_list_active(&db).await.unwrap();
    assert_eq!(active.len(), 2);

    product_set_active(&db, id, false).await.unwrap();
    let active_after = product_list_active(&db).await.unwrap();
    assert_eq!(active_after.len(), 1);
    assert_eq!(active_after[0].name, "Paket B");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test --lib repo::cs::tests::product_insert_list_active_and_deactivate`
Expected: FAIL — `product_insert` not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs`:

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProductRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub availability: Option<String>,
    pub active: i64,
    pub updated_at: String,
}

pub async fn product_insert(
    db: &Db,
    name: &str,
    description: Option<&str>,
    price: Option<f64>,
    currency: Option<&str>,
    availability: Option<&str>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_product (name, description, price, currency, availability, active, updated_at) \
         VALUES (?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(name)
    .bind(description)
    .bind(price)
    .bind(currency)
    .bind(availability)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    Ok(id)
}

pub async fn product_set_active(db: &Db, id: i64, active: bool) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE cs_product SET active = ?, updated_at = ? WHERE id = ?")
        .bind(if active { 1 } else { 0 })
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn product_list_active(db: &Db) -> anyhow::Result<Vec<ProductRow>> {
    let rows = sqlx::query_as::<_, ProductRow>(
        "SELECT * FROM cs_product WHERE active = 1 ORDER BY name ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// All products including inactive — for the admin pricing manager.
pub async fn product_list_all(db: &Db) -> anyhow::Result<Vec<ProductRow>> {
    let rows = sqlx::query_as::<_, ProductRow>("SELECT * FROM cs_product ORDER BY name ASC")
        .fetch_all(db)
        .await?;
    Ok(rows)
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib repo::cs::tests::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): product/pricing repo"
```

---

## Task 6: Order repo (with anti-enumeration lookup)

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
#[tokio::test]
async fn order_upsert_and_guarded_lookup() {
    let db = mem_db().await;
    order_upsert(&db, "ORD-100", Some("Budi"), Some("budi@mail.com"), "shipped", None).await.unwrap();

    // correct ref + contact returns the order
    let hit = order_lookup(&db, "ORD-100", "budi@mail.com").await.unwrap();
    assert!(hit.is_some());
    assert_eq!(hit.unwrap().status, "shipped");

    // correct ref but wrong contact returns nothing (anti-enumeration)
    let miss = order_lookup(&db, "ORD-100", "someone@else.com").await.unwrap();
    assert!(miss.is_none());

    // upsert again updates status in place (no duplicate ref)
    order_upsert(&db, "ORD-100", Some("Budi"), Some("budi@mail.com"), "delivered", None).await.unwrap();
    let updated = order_lookup(&db, "ORD-100", "budi@mail.com").await.unwrap().unwrap();
    assert_eq!(updated.status, "delivered");
}

#[tokio::test]
async fn order_lookup_contact_is_case_insensitive() {
    let db = mem_db().await;
    order_upsert(&db, "ORD-200", Some("Ani"), Some("Ani@Mail.com"), "processing", None).await.unwrap();
    let hit = order_lookup(&db, "ORD-200", "ani@mail.com").await.unwrap();
    assert!(hit.is_some());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test --lib repo::cs::tests::order_upsert_and_guarded_lookup`
Expected: FAIL — `order_upsert` not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs`:

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderRow {
    pub id: i64,
    pub external_ref: String,
    pub customer_name: Option<String>,
    pub customer_contact: Option<String>,
    pub status: String,
    pub details_json: Option<String>,
    pub updated_at: String,
}

/// Insert or update by `external_ref` (UNIQUE). Owner-populated.
pub async fn order_upsert(
    db: &Db,
    external_ref: &str,
    customer_name: Option<&str>,
    customer_contact: Option<&str>,
    status: &str,
    details_json: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO cs_order (external_ref, customer_name, customer_contact, status, details_json, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(external_ref) DO UPDATE SET \
           customer_name = excluded.customer_name, \
           customer_contact = excluded.customer_contact, \
           status = excluded.status, \
           details_json = excluded.details_json, \
           updated_at = excluded.updated_at",
    )
    .bind(external_ref)
    .bind(customer_name)
    .bind(customer_contact)
    .bind(status)
    .bind(details_json)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

/// Guarded lookup: only returns the order when BOTH the ref and the contact match
/// (contact compared case-insensitively). Prevents enumeration by ref alone.
pub async fn order_lookup(db: &Db, external_ref: &str, contact: &str) -> anyhow::Result<Option<OrderRow>> {
    let row = sqlx::query_as::<_, OrderRow>(
        "SELECT * FROM cs_order WHERE external_ref = ? AND LOWER(customer_contact) = LOWER(?)",
    )
    .bind(external_ref)
    .bind(contact)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// All orders — for the admin orders manager.
pub async fn order_list(db: &Db, limit: i64) -> anyhow::Result<Vec<OrderRow>> {
    let rows = sqlx::query_as::<_, OrderRow>("SELECT * FROM cs_order ORDER BY updated_at DESC LIMIT ?")
        .bind(limit)
        .fetch_all(db)
        .await?;
    Ok(rows)
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib repo::cs::tests::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): order repo with guarded ref+contact lookup"
```

---

## Task 7: Escalation repo

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `mod tests`:

```rust
#[tokio::test]
async fn escalation_create_list_and_handle() {
    let db = mem_db().await;
    let conv = conversation_create(&db, "web", Some("Budi"), Some("b@x.com"), None, "tok-e").await.unwrap();

    let esc = escalation_create(&db, conv.id, "cannot_answer", "Customer asks about custom integration")
        .await
        .unwrap();
    assert_eq!(esc.status, "open");

    let open = escalation_list_open(&db).await.unwrap();
    assert_eq!(open.len(), 1);

    escalation_mark_handled(&db, esc.id).await.unwrap();
    assert!(escalation_list_open(&db).await.unwrap().is_empty());

    let handled = escalation_get(&db, esc.id).await.unwrap().unwrap();
    assert_eq!(handled.status, "handled");
    assert!(handled.handled_at.is_some());
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test --lib repo::cs::tests::escalation_create_list_and_handle`
Expected: FAIL — `escalation_create` not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs`:

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EscalationRow {
    pub id: i64,
    pub conversation_id: i64,
    pub reason: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub handled_at: Option<String>,
}

pub async fn escalation_create(db: &Db, conversation_id: i64, reason: &str, summary: &str) -> anyhow::Result<EscalationRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_escalation (conversation_id, reason, summary, status, created_at) \
         VALUES (?, ?, ?, 'open', ?)",
    )
    .bind(conversation_id)
    .bind(reason)
    .bind(summary)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, EscalationRow>("SELECT * FROM cs_escalation WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn escalation_list_open(db: &Db) -> anyhow::Result<Vec<EscalationRow>> {
    let rows = sqlx::query_as::<_, EscalationRow>(
        "SELECT * FROM cs_escalation WHERE status = 'open' ORDER BY id DESC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn escalation_get(db: &Db, id: i64) -> anyhow::Result<Option<EscalationRow>> {
    let row = sqlx::query_as::<_, EscalationRow>("SELECT * FROM cs_escalation WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

pub async fn escalation_mark_handled(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE cs_escalation SET status = 'handled', handled_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test --lib repo::cs::tests::`
Expected: PASS — all `repo::cs` tests green.

- [ ] **Step 5: Final verification + commit**

Run: `cd backend && cargo test --lib repo::cs && cargo clippy --all-targets 2>&1 | tail -20`
Expected: tests PASS; no new clippy warnings in `repo/cs.rs`.

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): escalation repo (create/list/handle)"
```

---

## Self-Review

**Spec coverage (Plan 1 portion of §6 data model + storage half of §7):**
- `cs_conversation` ✓ Task 2 · `cs_message` ✓ Task 3 · `cs_kb_doc`/`cs_kb_chunk` ✓ Task 4 · `cs_product` ✓ Task 5 · `cs_order` (+ guarded lookup from §7) ✓ Task 6 · `cs_escalation` ✓ Task 7. Indexes ✓ Task 1.
- Embedding BLOB storage (spec §3 "embeddings as BLOB") ✓ Task 4 helpers.
- Order anti-enumeration guard (spec §7 `lookup_order` requires ref + contact) ✓ Task 6 `order_lookup`.
- Items intentionally deferred to later plans: embedding *computation*/cosine search (Plan 2 `cs/kb.rs`), agent/tools (Plan 2), public + admin APIs (Plans 3–4), Telegram notify in escalation (Plan 2 `cs/escalation.rs` — this plan only stores the row).

**Placeholder scan:** No TBD/TODO; every step has complete code and an exact command.

**Type consistency:** Row struct field names match their `SELECT *` columns. Function names referenced across tasks are consistent (`conversation_create`, `message_add`, `kb_replace_chunks`, `kb_chunks_with_embedding`, `embedding_to_blob`/`blob_to_embedding`, `product_list_active`, `order_lookup`, `escalation_create`). `KbChunkVec.vector` is the decoded `Vec<f32>` used by Plan 2's cosine search.

---

## Downstream plans (for context, not part of this plan)

- **Plan 2 — CS brain:** `cs/kb.rs` (chunk + OpenAI embed + cosine using `kb_chunks_with_embedding`), `cs/tools.rs`, `cs/dispatcher.rs`, `cs/agent.rs`, `cs/escalation.rs` (wraps `repo::cs::escalation_create` + Telegram notify).
- **Plan 3 — Public channel:** `api/cs_public.rs`, public-tier CORS allowlist + site-key + session token + rate-limit, `cs-widget.js` bundle.
- **Plan 4 — Admin:** `api/cs_admin.rs` + SPA pages (KB / pricing / orders / inbox) reusing `conversation_list_recent`, `kb_doc_list`, `product_list_all`, `order_list`, `escalation_list_open`.
