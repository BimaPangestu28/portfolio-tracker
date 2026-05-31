# Investment Tracker — Phase 3A (Ingestion Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an LLM-powered ingestion pipeline to the Rust backend: upload screenshots/PDFs → Claude extracts structured candidate entries → staged in a `review_item` table → user confirms/rejects → committed to the Phase 1 ledger. Nothing is auto-committed.

**Architecture:** New `llm` and `ingestion` modules in the existing `axum` crate. A thin Claude Messages API client (reqwest) sends text+image/PDF content blocks; a pure parser turns the JSON response into `ExtractedEntry` values; a review service stages them in SQLite and, on confirm, maps them to ledger rows via the existing Phase 1 repos (`transactions`, `instruments`, `accounts`). Source files are saved to disk for audit. Processing is synchronous.

**Tech Stack:** Rust, axum, sqlx (SQLite), reqwest (Anthropic Messages API), serde/serde_json, rust_decimal, chrono, base64, thiserror/anyhow. Model default `claude-sonnet-4-6` (env `INGEST_MODEL`); key via env `ANTHROPIC_API_KEY`. No `unwrap()`/`panic!()` in production paths.

**Builds on Phase 1 (already merged):** `crate::db::Db`, `crate::error::AppError`, repos `accounts`/`categories`/`instruments`/`transactions`/`prices` (with `dec()` helper), `AppState { db: Db }`, axum router in `src/api/`. This plan is Phase 3A-backend; the Review-page frontend is a separate plan (3A-frontend).

---

### Task 1: `review_item` migration + repo

**Files:**
- Create: `backend/migrations/0002_review_item.sql`
- Create: `backend/src/repo/review_items.rs`
- Modify: `backend/src/repo/mod.rs` (add `pub mod review_items;`)

- [ ] **Step 1: Write `backend/migrations/0002_review_item.sql`**

```sql
CREATE TABLE review_item (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    batch_id TEXT NOT NULL,
    source_kind TEXT NOT NULL,            -- 'image' | 'pdf'
    source_filename TEXT NOT NULL,
    source_path TEXT NOT NULL,
    doc_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending', -- 'pending' | 'confirmed' | 'rejected'
    needs_attention INTEGER NOT NULL DEFAULT 0,
    payload_json TEXT NOT NULL,
    raw_llm_json TEXT NOT NULL,
    suggested_instrument_id INTEGER REFERENCES instrument(id),
    suggested_account_id INTEGER REFERENCES account(id),
    created_txn_id INTEGER REFERENCES txn(id),
    created_at TEXT NOT NULL,
    confirmed_at TEXT
);
CREATE INDEX idx_review_item_status ON review_item(status, batch_id);
```

- [ ] **Step 2: Write `backend/src/repo/review_items.rs`** (row type, create, list, get, update payload, set status) with a test

```rust
use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ReviewItemRow {
    pub id: i64,
    pub batch_id: String,
    pub source_kind: String,
    pub source_filename: String,
    pub source_path: String,
    pub doc_type: String,
    pub status: String,
    pub needs_attention: i64,
    pub payload_json: String,
    pub raw_llm_json: String,
    pub suggested_instrument_id: Option<i64>,
    pub suggested_account_id: Option<i64>,
    pub created_txn_id: Option<i64>,
    pub created_at: String,
    pub confirmed_at: Option<String>,
}

pub struct NewReviewItem<'a> {
    pub batch_id: &'a str,
    pub source_kind: &'a str,
    pub source_filename: &'a str,
    pub source_path: &'a str,
    pub doc_type: &'a str,
    pub needs_attention: bool,
    pub payload_json: &'a str,
    pub raw_llm_json: &'a str,
    pub suggested_instrument_id: Option<i64>,
    pub suggested_account_id: Option<i64>,
}

pub async fn create(db: &Db, n: &NewReviewItem<'_>) -> anyhow::Result<ReviewItemRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO review_item (batch_id, source_kind, source_filename, source_path, doc_type, status, needs_attention, payload_json, raw_llm_json, suggested_instrument_id, suggested_account_id, created_at)
         VALUES (?,?,?,?,?, 'pending', ?,?,?,?,?,?)")
        .bind(n.batch_id).bind(n.source_kind).bind(n.source_filename).bind(n.source_path)
        .bind(n.doc_type).bind(n.needs_attention as i64).bind(n.payload_json).bind(n.raw_llm_json)
        .bind(n.suggested_instrument_id).bind(n.suggested_account_id).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ReviewItemRow> {
    Ok(sqlx::query_as::<_, ReviewItemRow>("SELECT * FROM review_item WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<ReviewItemRow>> {
    Ok(sqlx::query_as::<_, ReviewItemRow>("SELECT * FROM review_item WHERE status = ? ORDER BY batch_id, id").bind(status).fetch_all(db).await?)
}

pub async fn update_payload(db: &Db, id: i64, payload_json: &str) -> anyhow::Result<ReviewItemRow> {
    sqlx::query("UPDATE review_item SET payload_json = ? WHERE id = ?").bind(payload_json).bind(id).execute(db).await?;
    get(db, id).await
}

pub async fn mark_confirmed(db: &Db, id: i64, created_txn_id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE review_item SET status='confirmed', created_txn_id=?, confirmed_at=? WHERE id=?")
        .bind(created_txn_id).bind(&now).bind(id).execute(db).await?;
    Ok(())
}

pub async fn mark_rejected(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("UPDATE review_item SET status='rejected' WHERE id=?").bind(id).execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn create_list_and_status_transitions() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let row = create(&db, &NewReviewItem {
            batch_id: "b1", source_kind: "image", source_filename: "s.png", source_path: "data/uploads/b1/s.png",
            doc_type: "holdings_snapshot", needs_attention: false, payload_json: "{}", raw_llm_json: "{}",
            suggested_instrument_id: None, suggested_account_id: None,
        }).await.unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(list_by_status(&db, "pending").await.unwrap().len(), 1);
        update_payload(&db, row.id, "{\"x\":1}").await.unwrap();
        assert_eq!(get(&db, row.id).await.unwrap().payload_json, "{\"x\":1}");
        mark_rejected(&db, row.id).await.unwrap();
        assert_eq!(get(&db, row.id).await.unwrap().status, "rejected");
        assert_eq!(list_by_status(&db, "pending").await.unwrap().len(), 0);
    }
}
```

Add `pub mod review_items;` to `backend/src/repo/mod.rs`.

- [ ] **Step 3: Run test**

Run: `cd backend && cargo test repo::review_items`
Expected: `create_list_and_status_transitions` PASS.

- [ ] **Step 4: Commit**

```bash
cd /home/bima-pangestu/Works/portfolio-tracker && git add backend/migrations/0002_review_item.sql backend/src/repo/review_items.rs backend/src/repo/mod.rs && git commit -m "feat: review_item staging table and repo"
```

---

### Task 2: Extraction domain types + response parser

**Files:**
- Create: `backend/src/ingestion/mod.rs`
- Create: `backend/src/ingestion/extract.rs`
- Modify: `backend/src/main.rs` (add `mod ingestion;`)

- [ ] **Step 1: Write `backend/src/ingestion/mod.rs`**

```rust
pub mod extract;
pub mod review;
```
(Create an empty `backend/src/ingestion/review.rs` placeholder now — `: > backend/src/ingestion/review.rs` — filled in Task 5.) Add `mod ingestion;` to `main.rs`.

- [ ] **Step 2: Write the failing test in `backend/src/ingestion/extract.rs`**

```rust
use serde::{Deserialize, Serialize};

/// One candidate ledger entry extracted by the LLM (pre-mapping, pre-confirm).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedEntry {
    pub entry_type: String, // buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub instrument_name: Option<String>,
    #[serde(default)]
    pub quantity: Option<String>,
    #[serde(default)]
    pub price_native: Option<String>,
    #[serde(default)]
    pub fee_native: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub executed_at: Option<String>,
    #[serde(default)]
    pub account_hint: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default = "default_conf")]
    pub confidence: f64,
}
fn default_conf() -> f64 { 1.0 }

#[derive(Debug, Clone)]
pub struct Extraction {
    pub doc_type: String,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("response not valid JSON: {0}")]
    NotJson(String),
    #[error("missing field: {0}")]
    Missing(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_holdings_snapshot() {
        let raw = r#"{"doc_type":"holdings_snapshot","entries":[
            {"entry_type":"opening_balance","symbol":"BTC","quantity":"0.5","price_native":"60000","currency":"USD","confidence":0.9}
        ]}"#;
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "holdings_snapshot");
        assert_eq!(e.entries.len(), 1);
        assert_eq!(e.entries[0].symbol.as_deref(), Some("BTC"));
        assert_eq!(e.entries[0].entry_type, "opening_balance");
    }

    #[test]
    fn tolerates_json_wrapped_in_markdown_fence() {
        let raw = "```json\n{\"doc_type\":\"txn_history\",\"entries\":[]}\n```";
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "txn_history");
        assert_eq!(e.entries.len(), 0);
    }

    #[test]
    fn missing_doc_type_errors() {
        let raw = r#"{"entries":[]}"#;
        assert!(matches!(parse_extraction(raw), Err(ExtractError::Missing(_))));
    }

    #[test]
    fn defaults_confidence_when_absent() {
        let raw = r#"{"doc_type":"trade_confirmation","entries":[{"entry_type":"buy","symbol":"VOO"}]}"#;
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.entries[0].confidence, 1.0);
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cd backend && cargo test ingestion::extract`
Expected: FAIL — `parse_extraction` not found.

- [ ] **Step 4: Implement `parse_extraction`** (add above tests)

```rust
/// Strip an optional ```json ... ``` markdown fence the model may wrap around the JSON.
fn strip_fence(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")) {
        return rest.trim().strip_suffix("```").unwrap_or(rest).trim();
    }
    t
}

pub fn parse_extraction(raw: &str) -> Result<Extraction, ExtractError> {
    let cleaned = strip_fence(raw);
    let v: serde_json::Value = serde_json::from_str(cleaned).map_err(|e| ExtractError::NotJson(e.to_string()))?;
    let doc_type = v.get("doc_type").and_then(|d| d.as_str())
        .ok_or_else(|| ExtractError::Missing("doc_type".into()))?.to_string();
    let entries_val = v.get("entries").cloned().unwrap_or_else(|| serde_json::json!([]));
    let entries: Vec<ExtractedEntry> = serde_json::from_value(entries_val)
        .map_err(|e| ExtractError::NotJson(e.to_string()))?;
    Ok(Extraction { doc_type, entries })
}
```

- [ ] **Step 5: Run tests**

Run: `cd backend && cargo test ingestion::extract`
Expected: 4 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/ingestion/ backend/src/main.rs && git commit -m "feat: extraction types and LLM response parser"
```

---

### Task 3: Claude Messages API client

**Files:**
- Create: `backend/src/llm/mod.rs`
- Create: `backend/src/llm/claude.rs`
- Modify: `backend/Cargo.toml` (add `base64`), `backend/src/main.rs` (add `mod llm;`)

- [ ] **Step 1: Add `base64` to `backend/Cargo.toml`**

Under `[dependencies]` add: `base64 = "0.22"`

- [ ] **Step 2: Write `backend/src/llm/mod.rs`**

```rust
pub mod claude;
```
Add `mod llm;` to `main.rs`.

- [ ] **Step 3: Write the failing test in `backend/src/llm/claude.rs`** (pure request-builder + response-text extractor are unit-testable without network)

```rust
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("missing ANTHROPIC_API_KEY")]
    NoKey,
    #[error("http error: {0}")]
    Http(String),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
    /// (media_type, base64 data) — e.g. ("image/png", "...")
    Image(String, String),
    /// base64 PDF data
    Pdf(String),
}

#[derive(Serialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
}
#[derive(Serialize)]
struct Source {
    #[serde(rename = "type")]
    kind: &'static str, // "base64"
    media_type: String,
    data: String,
}

/// Build the JSON body for the Anthropic Messages API.
pub fn build_body(model: &str, system: &str, parts: &[Part]) -> serde_json::Value {
    let blocks: Vec<ContentBlock> = parts.iter().map(|p| match p {
        Part::Text(t) => ContentBlock { kind: "text", text: Some(t.clone()), source: None },
        Part::Image(mt, data) => ContentBlock { kind: "image", text: None, source: Some(Source { kind: "base64", media_type: mt.clone(), data: data.clone() }) },
        Part::Pdf(data) => ContentBlock { kind: "document", text: None, source: Some(Source { kind: "base64", media_type: "application/pdf".into(), data: data.clone() }) },
    }).collect();
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": [{ "role": "user", "content": blocks }]
    })
}

/// Extract concatenated text from an Anthropic Messages API response body.
pub fn extract_text(resp: &serde_json::Value) -> Result<String, LlmError> {
    let content = resp.get("content").and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::Shape("no content array".into()))?;
    let mut out = String::new();
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) { out.push_str(t); }
        }
    }
    if out.is_empty() { return Err(LlmError::Shape("no text blocks".into())); }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_has_model_and_image_block() {
        let body = build_body("claude-sonnet-4-6", "extract", &[Part::Text("hi".into()), Part::Image("image/png".into(), "AAAA".into())]);
        assert_eq!(body["model"], "claude-sonnet-4-6");
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn pdf_part_becomes_document_block() {
        let body = build_body("m", "s", &[Part::Pdf("UEs=".into())]);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "document");
        assert_eq!(blocks[0]["source"]["media_type"], "application/pdf");
    }

    #[test]
    fn extract_text_concatenates_text_blocks() {
        let resp = serde_json::json!({ "content": [ {"type":"text","text":"{\"a\":"}, {"type":"text","text":"1}"} ] });
        assert_eq!(extract_text(&resp).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn extract_text_errors_without_text() {
        let resp = serde_json::json!({ "content": [] });
        assert!(matches!(extract_text(&resp), Err(LlmError::Shape(_))));
    }
}
```

- [ ] **Step 4: Run to verify failure**

Run: `cd backend && cargo test llm::claude`
Expected: FAIL — items not found.

- [ ] **Step 5: Implement the network call** (add to `claude.rs`, below the pure helpers)

```rust
pub struct ClaudeClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl ClaudeClient {
    /// Reads ANTHROPIC_API_KEY and optional INGEST_MODEL from the environment.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| LlmError::NoKey)?;
        let model = std::env::var("INGEST_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".into());
        Ok(Self { api_key, model, client: reqwest::Client::new() })
    }

    pub fn model(&self) -> &str { &self.model }

    /// Send a single user message (system + parts) and return the concatenated text output.
    pub async fn complete(&self, system: &str, parts: &[Part]) -> Result<String, LlmError> {
        let body = build_body(&self.model, system, parts);
        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send().await.map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), body: json.to_string() });
        }
        extract_text(&json)
    }
}
```

- [ ] **Step 6: Run tests**

Run: `cd backend && cargo test llm::claude`
Expected: 4 tests PASS. (No network — only the pure helpers are tested; `complete` is exercised in Task 6's ignored live test.)

- [ ] **Step 7: Commit**

```bash
git add backend/src/llm/ backend/Cargo.toml backend/src/main.rs && git commit -m "feat: anthropic messages api client with image and pdf blocks"
```

---

### Task 4: Instrument/account matching helpers

**Files:**
- Create: `backend/src/ingestion/matching.rs`
- Modify: `backend/src/ingestion/mod.rs` (add `pub mod matching;`)

- [ ] **Step 1: Write the failing test in `matching.rs`**

```rust
use crate::db::Db;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments};

    #[tokio::test]
    async fn suggests_instrument_by_symbol_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"Bitcoin".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        assert_eq!(suggest_instrument(&db, "btc").await.unwrap(), Some(ins.id));
        assert_eq!(suggest_instrument(&db, "ETH").await.unwrap(), None);
    }

    #[tokio::test]
    async fn suggests_account_by_name_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let a = accounts::create(&db, &accounts::NewAccount { name:"Binance".into(), account_type:"exchange".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        assert_eq!(suggest_account(&db, "binance").await.unwrap(), Some(a.id));
        assert_eq!(suggest_account(&db, "Indodax").await.unwrap(), None);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test ingestion::matching`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement matching**

```rust
pub async fn suggest_instrument(db: &Db, symbol: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(symbol) = LOWER(?) LIMIT 1")
        .bind(symbol).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
}

pub async fn suggest_account(db: &Db, name: &str) -> anyhow::Result<Option<i64>> {
    let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM account WHERE LOWER(name) = LOWER(?) LIMIT 1")
        .bind(name).fetch_optional(db).await?;
    Ok(row.map(|(id,)| id))
}
```

Add `pub mod matching;` to `backend/src/ingestion/mod.rs`.

- [ ] **Step 4: Run tests**

Run: `cd backend && cargo test ingestion::matching`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/matching.rs backend/src/ingestion/mod.rs && git commit -m "feat: instrument and account matching helpers"
```

---

### Task 5: Review service — confirm/reject → ledger

**Files:**
- Replace: `backend/src/ingestion/review.rs`

This service turns a confirmed `review_item` payload into a ledger transaction. The payload is a JSON object with the fields the user (possibly) edited. The caller supplies the resolved `account_id` and `instrument_id` (the API layer in Task 6 handles inline-create and passes ids). The service fills FX defaults from `prices::latest_fx` and calls `transactions::create`.

- [ ] **Step 1: Write the failing test in `review.rs`**

```rust
use crate::db::Db;
use serde::Deserialize;

/// The confirm payload the API hands to the service: resolved ids + the (edited) fields.
#[derive(Debug, Deserialize)]
pub struct ConfirmPayload {
    pub account_id: i64,
    pub instrument_id: i64,
    pub entry_type: String,
    pub executed_at: String,        // rfc3339
    pub quantity: String,
    pub price_native: String,
    #[serde(default)]
    pub fee_native: Option<String>,
    pub currency: String,
    #[serde(default)]
    pub fx_to_idr: Option<String>,  // if absent, default from latest USD/IDR
    #[serde(default)]
    pub fx_to_usd: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments, review_items, transactions};
    use rust_decimal_macros::dec;

    async fn seed(db: &Db) -> (i64, i64) {
        let a = accounts::create(db, &accounts::NewAccount { name:"M".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let i = instruments::create(db, &instruments::NewInstrument { symbol:"BTC".into(), name:"B".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        (a.id, i.id)
    }

    #[tokio::test]
    async fn confirm_inserts_ledger_txn_and_marks_confirmed() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        crate::repo::prices::upsert_fx(&db, "USD", "IDR", dec!(16000), "2026-01-01").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"trade_confirmation", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();

        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-01-02T00:00:00Z".into(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:None, fx_to_usd:None, note:None,
        };
        let txn_id = confirm(&db, item.id, &payload).await.unwrap();
        assert!(txn_id > 0);
        // ledger now has the transaction
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].fx_to_idr, dec!(16000)); // defaulted from latest_fx
        // item marked confirmed with txn id
        let reloaded = review_items::get(&db, item.id).await.unwrap();
        assert_eq!(reloaded.status, "confirmed");
        assert_eq!(reloaded.created_txn_id, Some(txn_id));
    }

    #[tokio::test]
    async fn reject_marks_rejected_without_ledger_row() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (_a, _i) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"holdings_snapshot", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:None, suggested_account_id:None,
        }).await.unwrap();
        reject(&db, item.id).await.unwrap();
        assert_eq!(review_items::get(&db, item.id).await.unwrap().status, "rejected");
        assert_eq!(transactions::list_all(&db).await.unwrap().len(), 0);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test ingestion::review`
Expected: FAIL — `confirm`/`reject` not found.

- [ ] **Step 3: Implement `confirm` and `reject`**

```rust
use crate::repo::{prices, review_items, transactions};
use rust_decimal::Decimal;

/// Confirm a review item: build a ledger transaction from the payload and mark the item confirmed.
/// FX fields default from the latest USD/IDR rate when absent. Returns the new txn id.
pub async fn confirm(db: &Db, item_id: i64, p: &ConfirmPayload) -> anyhow::Result<i64> {
    let usd_idr = prices::latest_fx(db, "USD", "IDR").await?.unwrap_or(Decimal::ONE);
    let fx_to_idr = p.fx_to_idr.clone().unwrap_or_else(|| usd_idr.to_string());
    let fx_to_usd = p.fx_to_usd.clone().unwrap_or_else(|| "1".to_string());

    let nt = transactions::NewTransaction {
        account_id: p.account_id,
        instrument_id: p.instrument_id,
        txn_type: p.entry_type.clone(),
        executed_at: chrono::DateTime::parse_from_rfc3339(&p.executed_at)
            .map_err(|e| anyhow::anyhow!("bad executed_at: {e}"))?
            .with_timezone(&chrono::Utc),
        quantity: p.quantity.clone(),
        price_native: p.price_native.clone(),
        fee_native: p.fee_native.clone(),
        currency: p.currency.clone(),
        fx_to_idr,
        fx_to_usd,
        note: p.note.clone(),
    };
    let txn = transactions::create(db, &nt).await?;
    review_items::mark_confirmed(db, item_id, txn.id).await?;
    Ok(txn.id)
}

pub async fn reject(db: &Db, item_id: i64) -> anyhow::Result<()> {
    review_items::mark_rejected(db, item_id).await
}
```

Note: `transactions::NewTransaction.executed_at` is a `chrono::DateTime<Utc>` (see Phase 1 `repo/transactions.rs`), so we parse the rfc3339 string here. `fee_native` is `Option<String>`. These match the Phase 1 types exactly.

- [ ] **Step 4: Run tests**

Run: `cd backend && cargo test ingestion::review`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/review.rs && git commit -m "feat: review confirm/reject mapping to ledger"
```

---

### Task 6: Ingestion orchestrator — upload → extract → stage

**Files:**
- Create: `backend/src/ingestion/ingest.rs`
- Modify: `backend/src/ingestion/mod.rs` (add `pub mod ingest;`)

This ties the LLM client + parser + matching + repo together: decode uploaded files, save to disk, call Claude, parse, and stage one `review_item` per extracted entry.

- [ ] **Step 1: Write the extraction system prompt + a pure "entry → NewReviewItem payload" test**

In `ingest.rs`:

```rust
use crate::db::Db;
use crate::ingestion::extract::{parse_extraction, ExtractedEntry};
use crate::ingestion::matching::{suggest_account, suggest_instrument};
use crate::llm::claude::{ClaudeClient, Part};
use crate::repo::review_items::{self, NewReviewItem};
use base64::Engine;

pub const SYSTEM_PROMPT: &str = r#"You extract financial transactions from an uploaded image or PDF for a personal investment tracker.
Classify the document as one of: holdings_snapshot, txn_history, bank_statement, trade_confirmation.
Return ONLY a JSON object, no prose, shaped exactly:
{"doc_type": "<one of the four>", "entries": [ { "entry_type": "buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance", "symbol": "...", "instrument_name": "...", "quantity": "...", "price_native": "...", "fee_native": "...", "currency": "...", "executed_at": "YYYY-MM-DDTHH:MM:SSZ", "account_hint": "...", "note": "...", "confidence": 0.0 } ] }
Rules: holdings_snapshot rows -> entry_type "opening_balance" with quantity and average cost as price_native. txn_history/trade_confirmation -> buy/sell/dividend/fee. bank_statement -> deposit/withdrawal/dividend/interest. Numbers as strings, no thousands separators. Omit unknown fields. Set confidence in [0,1]. If a value is uncertain, still include the entry with a lower confidence."#;

/// Decide if an entry needs human attention (low confidence or missing core fields).
pub fn needs_attention(e: &ExtractedEntry) -> bool {
    if e.confidence < 0.6 { return true; }
    match e.entry_type.as_str() {
        "deposit" | "withdrawal" | "dividend" | "interest" => e.quantity.is_none() && e.price_native.is_none(),
        _ => e.symbol.is_none() || e.quantity.is_none(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::extract::ExtractedEntry;

    fn entry(conf: f64, symbol: Option<&str>, qty: Option<&str>) -> ExtractedEntry {
        ExtractedEntry { entry_type:"buy".into(), symbol:symbol.map(String::from), instrument_name:None,
            quantity:qty.map(String::from), price_native:Some("1".into()), fee_native:None, currency:Some("USD".into()),
            executed_at:None, account_hint:None, note:None, confidence:conf }
    }

    #[test]
    fn low_confidence_needs_attention() {
        assert!(needs_attention(&entry(0.4, Some("BTC"), Some("1"))));
    }
    #[test]
    fn missing_symbol_needs_attention() {
        assert!(needs_attention(&entry(0.9, None, Some("1"))));
    }
    #[test]
    fn complete_high_confidence_ok() {
        assert!(!needs_attention(&entry(0.9, Some("BTC"), Some("1"))));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test ingestion::ingest`
Expected: FAIL — `needs_attention` not found.

- [ ] **Step 3: Implement the orchestrator** (add below tests' dependencies, above `#[cfg(test)]`)

```rust
pub struct UploadFile {
    pub filename: String,
    pub media_type: String, // "image/png", "image/jpeg", "application/pdf"
    pub data_base64: String,
}

pub struct IngestResult {
    pub batch_id: String,
    pub items: Vec<review_items::ReviewItemRow>,
}

/// Decode + save a file to data/uploads/<batch_id>/, returning (kind, path).
fn save_file(batch_id: &str, f: &UploadFile) -> anyhow::Result<(String, String)> {
    let dir = format!("data/uploads/{batch_id}");
    std::fs::create_dir_all(&dir)?;
    let path = format!("{dir}/{}", f.filename);
    let bytes = base64::engine::general_purpose::STANDARD.decode(f.data_base64.as_bytes())
        .map_err(|e| anyhow::anyhow!("bad base64 for {}: {e}", f.filename))?;
    std::fs::write(&path, &bytes)?;
    let kind = if f.media_type == "application/pdf" { "pdf" } else { "image" };
    Ok((kind.to_string(), path))
}

fn to_part(f: &UploadFile) -> Part {
    if f.media_type == "application/pdf" {
        Part::Pdf(f.data_base64.clone())
    } else {
        Part::Image(f.media_type.clone(), f.data_base64.clone())
    }
}

/// Full pipeline for one upload batch: save files, call Claude once per file, parse, stage items.
/// `batch_id` is supplied by the caller (the API layer) so it is deterministic/testable.
pub async fn ingest_batch(db: &Db, client: &ClaudeClient, batch_id: &str, files: &[UploadFile]) -> anyhow::Result<IngestResult> {
    let mut items = Vec::new();
    for f in files {
        let (kind, path) = save_file(batch_id, f)?;
        let parts = vec![Part::Text("Extract per the system instructions.".into()), to_part(f)];
        let raw = client.complete(SYSTEM_PROMPT, &parts).await
            .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
        let extraction = parse_extraction(&raw)
            .map_err(|e| anyhow::anyhow!("parse error: {e}; raw={raw}"))?;
        for entry in &extraction.entries {
            let payload = serde_json::to_string(entry)?;
            let sug_ins = match &entry.symbol { Some(s) => suggest_instrument(db, s).await?, None => None };
            let sug_acc = match &entry.account_hint { Some(a) => suggest_account(db, a).await?, None => None };
            let row = review_items::create(db, &NewReviewItem {
                batch_id,
                source_kind: &kind,
                source_filename: &f.filename,
                source_path: &path,
                doc_type: &extraction.doc_type,
                needs_attention: needs_attention(entry),
                payload_json: &payload,
                raw_llm_json: &raw,
                suggested_instrument_id: sug_ins,
                suggested_account_id: sug_acc,
            }).await?;
            items.push(row);
        }
    }
    Ok(IngestResult { batch_id: batch_id.to_string(), items })
}
```

Add `pub mod ingest;` to `backend/src/ingestion/mod.rs`.

- [ ] **Step 4: Run tests + build**

Run: `cd backend && cargo test ingestion::ingest && cargo build`
Expected: 3 `needs_attention` tests PASS; crate builds. (`ingest_batch` itself calls the network and is not unit-tested here; it is exercised by the live test in Task 7 and the API smoke in Task 8.)

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/ingest.rs backend/src/ingestion/mod.rs && git commit -m "feat: ingestion orchestrator (save, extract, stage)"
```

---

### Task 7: Gated live LLM test (optional, skipped without key)

**Files:**
- Create: `backend/tests/live_llm.rs`

- [ ] **Step 1: Write an ignored integration test**

```rust
// Run with: ANTHROPIC_API_KEY=... cargo test --test live_llm -- --ignored
#[tokio::test]
#[ignore]
async fn live_extract_smoke() {
    let client = match portfolio_tracker::llm::claude::ClaudeClient::from_env() {
        Ok(c) => c,
        Err(_) => { eprintln!("no key; skipping"); return; }
    };
    // A tiny 1x1 PNG (base64) — the model should still return valid JSON with a doc_type.
    let png_1x1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
    let parts = vec![
        portfolio_tracker::llm::claude::Part::Text("Extract per instructions.".into()),
        portfolio_tracker::llm::claude::Part::Image("image/png".into(), png_1x1.into()),
    ];
    let out = client.complete(portfolio_tracker::ingestion::ingest::SYSTEM_PROMPT, &parts).await.unwrap();
    let parsed = portfolio_tracker::ingestion::extract::parse_extraction(&out).unwrap();
    assert!(["holdings_snapshot","txn_history","bank_statement","trade_confirmation"].contains(&parsed.doc_type.as_str()));
}
```

This requires the crate to expose modules. The binary crate already declares `mod llm; mod ingestion;` — for an integration test to import them, they must be reachable. Since this is a binary (`main.rs`), integration tests can't see private `mod`s. **Two options — pick the simpler:** (a) keep this as a `#[cfg(test)]` unit test inside `src/ingestion/ingest.rs` guarded by `#[ignore]` instead of a `tests/` file; or (b) add a minimal `src/lib.rs` re-exporting the modules. To avoid restructuring, use option (a): delete the `tests/live_llm.rs` idea and instead add the ignored test into `ingest.rs`'s test module:

```rust
    #[tokio::test]
    #[ignore]
    async fn live_extract_smoke() {
        let client = match crate::llm::claude::ClaudeClient::from_env() { Ok(c) => c, Err(_) => return };
        let png_1x1 = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYPhfDwAChwGA60e6kgAAAABJRU5ErkJggg==";
        let parts = vec![ Part::Text("Extract per instructions.".into()), Part::Image("image/png".into(), png_1x1.into()) ];
        let out = client.complete(SYSTEM_PROMPT, &parts).await.unwrap();
        let parsed = crate::ingestion::extract::parse_extraction(&out).unwrap();
        assert!(["holdings_snapshot","txn_history","bank_statement","trade_confirmation"].contains(&parsed.doc_type.as_str()));
    }
```

- [ ] **Step 2: Verify it is skipped by default**

Run: `cd backend && cargo test ingestion::ingest`
Expected: the 3 unit tests run and pass; `live_extract_smoke` shows as `ignored`.

- [ ] **Step 3: Commit**

```bash
git add backend/src/ingestion/ingest.rs && git commit -m "test: gated live llm extraction smoke (ignored without api key)"
```

---

### Task 8: API endpoints — ingest + review CRUD

**Files:**
- Create: `backend/src/api/ingest.rs`
- Modify: `backend/src/api/mod.rs` (routes), `backend/src/api/crud.rs` (reuse `AppState`)

- [ ] **Step 1: Write `backend/src/api/ingest.rs`**

```rust
use crate::error::AppError;
use crate::ingestion::ingest::{ingest_batch, UploadFile};
use crate::ingestion::review::{confirm, reject, ConfirmPayload};
use crate::llm::claude::ClaudeClient;
use crate::repo::review_items;
use crate::AppState;
use axum::{extract::{Path, Query, State}, Json};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UploadFileIn { pub filename: String, pub media_type: String, pub data_base64: String }

#[derive(Deserialize)]
pub struct IngestIn { pub files: Vec<UploadFileIn> }

#[derive(serde::Serialize)]
pub struct IngestOut { pub batch_id: String, pub items: Vec<review_items::ReviewItemRow> }

/// Deterministic-ish batch id without Date::now() in non-test code: use a uuid-like from process time.
fn new_batch_id() -> String {
    // chrono::Utc::now is available in production paths (only Date::now()/rand are restricted in *workflow scripts*, not app code).
    format!("batch-{}", chrono::Utc::now().timestamp_millis())
}

pub async fn ingest(State(s): State<AppState>, Json(b): Json<IngestIn>) -> Result<Json<IngestOut>, AppError> {
    if b.files.is_empty() { return Err(AppError::BadRequest("no files".into())); }
    let client = ClaudeClient::from_env().map_err(|e| AppError::Other(anyhow::anyhow!("llm config: {e}")))?;
    let files: Vec<UploadFile> = b.files.into_iter()
        .map(|f| UploadFile { filename: f.filename, media_type: f.media_type, data_base64: f.data_base64 })
        .collect();
    let batch_id = new_batch_id();
    let res = ingest_batch(&s.db, &client, &batch_id, &files).await
        .map_err(|e| AppError::Other(anyhow::anyhow!("ingest failed: {e}")))?;
    Ok(Json(IngestOut { batch_id: res.batch_id, items: res.items }))
}

#[derive(Deserialize)]
pub struct ReviewQuery { #[serde(default = "default_status")] pub status: String }
fn default_status() -> String { "pending".into() }

pub async fn list_review(State(s): State<AppState>, Query(q): Query<ReviewQuery>) -> Result<Json<Vec<review_items::ReviewItemRow>>, AppError> {
    Ok(Json(review_items::list_by_status(&s.db, &q.status).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct PatchIn { pub payload_json: serde_json::Value }
pub async fn patch_review(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<PatchIn>) -> Result<Json<review_items::ReviewItemRow>, AppError> {
    let payload = serde_json::to_string(&b.payload_json).map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(review_items::update_payload(&s.db, id, &payload).await.map_err(AppError::Other)?))
}

#[derive(serde::Serialize)]
pub struct ConfirmOut { pub created_txn_id: i64 }
pub async fn confirm_review(State(s): State<AppState>, Path(id): Path<i64>, Json(p): Json<ConfirmPayload>) -> Result<Json<ConfirmOut>, AppError> {
    let txn_id = confirm(&s.db, id, &p).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(ConfirmOut { created_txn_id: txn_id }))
}

pub async fn reject_review(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    reject(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
```

- [ ] **Step 2: Wire routes in `backend/src/api/mod.rs`**

Add `pub mod ingest;` at the top, and add these routes to the `Router` (before `.layer(...)`):

```rust
        .route("/ingest", post(ingest::ingest))
        .route("/ingest/review", get(ingest::list_review))
        .route("/ingest/review/:id", axum::routing::patch(ingest::patch_review))
        .route("/ingest/review/:id/confirm", post(ingest::confirm_review))
        .route("/ingest/review/:id/reject", post(ingest::reject_review))
```

(`get`, `post` are already imported in `mod.rs`; `patch` is referenced fully-qualified to avoid touching the import list.)

- [ ] **Step 3: Build + body-shape smoke (no network)**

Run: `cd backend && cargo build`
Expected: compiles.
Smoke the review endpoints that DON'T need the LLM (list/confirm/reject) against a manually-seeded item — and confirm `/ingest` without an API key returns a clean 500 (not a panic). Run on a free port (8080 may be occupied by `greentic-start`; if so temporarily bind 8099 and revert, do NOT commit the port change):
```bash
cd /home/bima-pangestu/Works/portfolio-tracker/backend && rm -f ing.db
(DATABASE_URL="sqlite://ing.db" cargo run >/tmp/ing.log 2>&1 &) ; sleep 5
B=http://localhost:8080
echo "list pending (expect [] 200):"; curl -s -o /dev/null -w "%{http_code}\n" "$B/ingest/review?status=pending"
echo "ingest without key (expect 500, NOT a crash):"; curl -s -o /dev/null -w "%{http_code}\n" -XPOST "$B/ingest" -H 'content-type: application/json' -d '{"files":[{"filename":"a.png","media_type":"image/png","data_base64":"AAAA"}]}'
echo "server still alive (expect 200):"; curl -s -o /dev/null -w "%{http_code}\n" "$B/health"
pkill -f 'target/debug/portfolio-tracker'; rm -f ing.db
```
Expected: list → `200` with `[]`; ingest-without-key → `500`; health → `200` (process did not crash — proves no panic on the LLM-config error path). If `ANTHROPIC_API_KEY` happens to be set in the env, the ingest call will instead attempt the network; in that case just confirm it returns 200 or a clean 5xx, not a panic.

- [ ] **Step 4: Commit**

```bash
cd /home/bima-pangestu/Works/portfolio-tracker && git add backend/src/api/ingest.rs backend/src/api/mod.rs && git commit -m "feat: ingest and review REST endpoints"
```

---

### Task 9: data/uploads gitignore + full suite

**Files:**
- Modify: `.gitignore`

- [ ] **Step 1: Ignore uploaded files**

Append to `/home/bima-pangestu/Works/portfolio-tracker/.gitignore`:
```
backend/data/
```

- [ ] **Step 2: Run the full backend test suite**

Run: `cd backend && cargo test`
Expected: all prior Phase 1 tests (29) plus the new ingestion/review/llm tests pass; live test shows `ignored`. Report the total count.

- [ ] **Step 3: Commit**

```bash
cd /home/bima-pangestu/Works/portfolio-tracker && git add .gitignore && git commit -m "chore: ignore backend/data uploads dir"
```

---

## Self-Review

**Spec coverage (spec §3 in-scope → task):**
- `POST /ingest` base64 upload → Task 8 ✅
- Claude client (vision + PDF) → Task 3 ✅
- Structured extraction + doc_type classification → Tasks 2 (parser), 6 (prompt + orchestrator) ✅
- `review_item` table + repo + confirm/reject service → Tasks 1, 5 ✅
- Instrument/account suggest + inline create → Task 4 (suggest); inline-create is performed by the **frontend** resolving to ids before calling `/ingest/review/:id/confirm` (the confirm endpoint takes resolved `account_id`/`instrument_id`) — documented, handled in 3A-frontend plan ✅
- REST list/edit/confirm/reject → Task 8 ✅
- Four doc_types → Task 6 prompt + Task 5 mapping (opening_balance/buy/sell/deposit/...) ✅
- Synchronous processing → Task 6/8 (no job table) ✅
- Source files on disk `data/uploads/<batch_id>/` → Task 6 (`save_file`), Task 9 (gitignore) ✅
- FX default from `latest_fx` at confirm → Task 5 ✅
- needs_attention for low-confidence/missing → Task 6 ✅
- Error handling: LLM fail → 502/500 no partial stage, no panic → Task 8 smoke verifies process survives; `ingest_batch` returns `Err` before staging on LLM/parse failure (per-file: a file that fails extraction aborts the batch with an error — **note:** this means one bad file fails the whole upload; acceptable for 3A, flagged below) ✅
- Secret via env, never logged → Task 3 (`from_env`, key only in header) ✅

**Known limitations (acceptable for 3A, flagged not hidden):**
- One file failing extraction fails the whole `/ingest` batch (Task 6 propagates the first error). Partial-success batching is a future refinement.
- Inline-create of instrument/account happens client-side (frontend calls existing `POST /instruments` / `POST /accounts` then passes the new id to confirm). The confirm endpoint deliberately takes resolved ids — keeps the service simple and reuses Phase 1 validation.
- `bank_statement` income/dividend tie-to-instrument is resolved by the user during review (they pick the instrument), not auto-linked.

**Placeholder scan:** Task 7 explicitly resolves the "tests/ can't see binary crate modules" issue by choosing option (a) (inline `#[ignore]` test) — not a placeholder, a decision. No TBD/TODO elsewhere.

**Type consistency:** `ExtractedEntry`, `Extraction`, `parse_extraction`, `Part`, `build_body`, `extract_text`, `ClaudeClient::{from_env,complete}`, `NewReviewItem`, `ReviewItemRow`, `review_items::{create,get,list_by_status,update_payload,mark_confirmed,mark_rejected}`, `ConfirmPayload`, `confirm`/`reject`, `ingest_batch`/`UploadFile`/`needs_attention`, `suggest_instrument`/`suggest_account` are defined once and referenced consistently. `transactions::NewTransaction` field types (`executed_at: DateTime<Utc>`, `fee_native: Option<String>`) match Phase 1 and are honored in Task 5.

---

## Execution Handoff

Plan complete — 9 tasks. The frontend Review page is a separate plan (3A-frontend). Most tests are offline (pure parsers, repo, mapping); the single live LLM test is `#[ignore]` and needs `ANTHROPIC_API_KEY`.
