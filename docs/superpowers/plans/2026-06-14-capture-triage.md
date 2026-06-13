# Capture & Triage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A GTD quick-capture inbox, prompt-driven action-item extraction with batch confirm, smart routing (inbox vs create), and voice notes transcribed via OpenAI Whisper into the normal text pipeline.

**Architecture:** New `inbox` table + repo + 3 agent tools. Extraction & sort are prompt-driven over existing create tools. Whisper transcription added to the OpenAI-shaped `NativeLlmClient`; a Telegram voice branch transcribes → echoes → routes the transcript through `handle_message`.

**Tech Stack:** Rust, sqlx (SQLite), reqwest (multipart), serde_json, chrono. Tests: `cargo test <filter>` from `backend/` (BIN crate — never `cargo test --lib`, never `cargo fmt`).

---

## Conventions
- Paths relative to `backend/`. Run cargo from `backend/`. Commit from repo root.
- End commit bodies with: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- `created_at`/`sorted_at` use `chrono::Utc::now().to_rfc3339()` (+00:00), matching the codebase convention for audit timestamps.

---

## Task 1: Inbox migration + repo

**Files:**
- Create: `migrations/0019_inbox.sql`
- Create: `src/repo/inbox.rs`
- Modify: `src/repo/mod.rs` (declare `pub mod inbox;`)

- [ ] **Step 1: Migration**

Create `migrations/0019_inbox.sql`:
```sql
-- Fase 4: GTD quick-capture inbox. Raw captures await batch triage.
CREATE TABLE inbox (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  content TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'sorted', 'dropped')),
  created_at TEXT NOT NULL,
  sorted_at TEXT
);

CREATE INDEX idx_inbox_pending ON inbox (status, id);
```

> If `migrations/0019_*.sql` already exists when you start (main advanced again), STOP and report — the number must be re-coordinated.

- [ ] **Step 2: Declare the module**

In `src/repo/mod.rs`, add `pub mod inbox;` in alphabetical position among the other `pub mod` declarations.

- [ ] **Step 3: Write `inbox.rs` with the repo + tests**

Create `src/repo/inbox.rs`:
```rust
//! Persistence for the GTD quick-capture inbox (see migration 0019).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InboxRow {
    pub id: i64,
    pub content: String,
    pub status: String,
    pub created_at: String,
    pub sorted_at: Option<String>,
}

pub async fn create(db: &Db, content: &str) -> anyhow::Result<InboxRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query("INSERT INTO inbox (content, status, created_at) VALUES (?, 'pending', ?)")
        .bind(content)
        .bind(&now)
        .execute(db)
        .await?
        .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<InboxRow> {
    let row = sqlx::query_as::<_, InboxRow>("SELECT * FROM inbox WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Pending captures, oldest first.
pub async fn list_pending(db: &Db) -> anyhow::Result<Vec<InboxRow>> {
    let rows = sqlx::query_as::<_, InboxRow>(
        "SELECT * FROM inbox WHERE status = 'pending' ORDER BY id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Move the given pending items to `status` ('sorted' or 'dropped'), stamping
/// sorted_at. Only pending rows change. Returns the number of rows affected.
pub async fn resolve(db: &Db, ids: &[i64], status: &str) -> anyhow::Result<u64> {
    let now = chrono::Utc::now().to_rfc3339();
    let mut affected = 0u64;
    for id in ids {
        let result = sqlx::query(
            "UPDATE inbox SET status = ?, sorted_at = ? WHERE id = ? AND status = 'pending'",
        )
        .bind(status)
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
        affected += result.rows_affected();
    }
    Ok(affected)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_then_list_pending() {
        let db = mem_db().await;
        let a = create(&db, "beli kado").await.unwrap();
        let _b = create(&db, "meeting senin").await.unwrap();
        assert_eq!(a.status, "pending");
        assert!(a.sorted_at.is_none());
        let pending = list_pending(&db).await.unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].content, "beli kado");
    }

    #[tokio::test]
    async fn resolve_marks_only_listed_pending_rows() {
        let db = mem_db().await;
        let a = create(&db, "a").await.unwrap();
        let b = create(&db, "b").await.unwrap();
        let c = create(&db, "c").await.unwrap();
        let affected = resolve(&db, &[a.id, b.id], "sorted").await.unwrap();
        assert_eq!(affected, 2);
        let pending = list_pending(&db).await.unwrap();
        assert_eq!(pending.iter().map(|r| r.id).collect::<Vec<_>>(), vec![c.id]);
        // a is sorted with a timestamp; resolving it again affects 0 rows.
        let again = resolve(&db, &[a.id], "sorted").await.unwrap();
        assert_eq!(again, 0);
        assert!(get(&db, a.id).await.unwrap().sorted_at.is_some());
    }

    #[tokio::test]
    async fn resolve_dropped_removes_from_pending() {
        let db = mem_db().await;
        let a = create(&db, "junk").await.unwrap();
        resolve(&db, &[a.id], "dropped").await.unwrap();
        assert!(list_pending(&db).await.unwrap().is_empty());
        assert_eq!(get(&db, a.id).await.unwrap().status, "dropped");
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test inbox::tests`
Expected: PASS (3 tests; migration applies on the in-memory DB).

- [ ] **Step 5: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/migrations/0019_inbox.sql backend/src/repo/inbox.rs backend/src/repo/mod.rs
git commit -m "feat(inbox): quick-capture inbox table + repo (migration 0019)"
```

---

## Task 2: Inbox tools

**Files:**
- Modify: `src/assistant/tools.rs` (3 schemas + registration test)
- Modify: `src/assistant/dispatcher.rs` (3 match arms, 3 handlers, tests)

- [ ] **Step 1: Write failing tests**

Add to `src/assistant/dispatcher.rs` tests (uses the `mem_db()` helper):
```rust
    #[tokio::test]
    async fn capture_to_inbox_stores_and_lists() {
        let db = mem_db().await;
        let out = dispatch(&db, "capture_to_inbox", &serde_json::json!({ "content": "beli kado" })).await.unwrap();
        assert!(out.to_lowercase().contains("inbox"), "{out}");
        let listed = dispatch(&db, "list_inbox", &serde_json::json!({})).await.unwrap();
        assert!(listed.contains("beli kado"), "{listed}");
    }

    #[tokio::test]
    async fn list_inbox_empty_is_explicit() {
        let db = mem_db().await;
        let out = dispatch(&db, "list_inbox", &serde_json::json!({})).await.unwrap();
        assert!(out.to_lowercase().contains("kosong"), "{out}");
    }

    #[tokio::test]
    async fn resolve_inbox_marks_sorted_and_rejects_bad_status() {
        let db = mem_db().await;
        let row = crate::repo::inbox::create(&db, "x").await.unwrap();
        let out = dispatch(&db, "resolve_inbox", &serde_json::json!({ "ids": [row.id], "status": "sorted" })).await.unwrap();
        assert!(out.contains("1"), "{out}");
        assert!(crate::repo::inbox::list_pending(&db).await.unwrap().is_empty());
        let err = dispatch(&db, "resolve_inbox", &serde_json::json!({ "ids": [row.id], "status": "nonsense" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("status"), "{err}");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test capture_to_inbox_stores_and_lists`
Expected: FAIL ("unknown tool: capture_to_inbox").

- [ ] **Step 3: Add dispatch arms**

In the `match name` block in `dispatch`, add (after the todo/reminder arms, before the clickup arms is fine):
```rust
        "capture_to_inbox" => capture_to_inbox(db, input).await,
        "list_inbox" => list_inbox(db).await,
        "resolve_inbox" => resolve_inbox(db, input).await,
```

- [ ] **Step 4: Add handlers**

Add (near the todo handlers):
```rust
async fn capture_to_inbox(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let content = str_arg(input, "content").ok_or("missing required argument 'content'")?;
    let row = crate::repo::inbox::create(db, content).await.map_err(|e| format!("db error: {e}"))?;
    Ok(format!("dicatat ke inbox (#{})", row.id))
}

async fn list_inbox(db: &Db) -> Result<String, String> {
    let rows = crate::repo::inbox::list_pending(db).await.map_err(|e| format!("db error: {e}"))?;
    if rows.is_empty() {
        return Ok("inbox kosong".into());
    }
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!("#{} {}\n", row.id, row.content));
    }
    Ok(out)
}

async fn resolve_inbox(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let status = str_arg(input, "status").ok_or("missing required argument 'status'")?;
    if !matches!(status, "sorted" | "dropped") {
        return Err(format!("invalid status '{status}' — use sorted/dropped"));
    }
    let ids: Vec<i64> = match input.get("ids") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|v| v.as_i64().ok_or_else(|| format!("ids must be integers, got {v}")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("missing required argument 'ids' (array of integers)".into()),
    };
    let affected = crate::repo::inbox::resolve(db, &ids, status).await.map_err(|e| format!("db error: {e}"))?;
    Ok(format!("{affected} item inbox ditandai {status}"))
}
```

- [ ] **Step 5: Register the three tool schemas**

In `src/assistant/tools.rs`, append these objects at the END of the `definitions()` array (add a comma after the current last object, then):
```rust
        {
            "name": "capture_to_inbox",
            "description": "Save a raw quick capture to the GTD inbox for later sorting. Use for vague/ambiguous dumps with no clear single action (e.g. 'inget beliin kado', 'ide fitur X').",
            "input_schema": {
                "type": "object",
                "properties": { "content": { "type": "string", "description": "The raw captured text" } },
                "required": ["content"]
            }
        },
        {
            "name": "list_inbox",
            "description": "List pending inbox captures with ids. Use for 'apa di inbox?' or before sorting.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "resolve_inbox",
            "description": "Mark inbox items as sorted (after you created todos/events/tasks/notes from them) or dropped (discarded). Pass the item ids.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "ids": { "type": "array", "items": { "type": "integer" }, "description": "Inbox item ids to resolve" },
                    "status": { "type": "string", "enum": ["sorted", "dropped"], "description": "sorted (handled) or dropped (discarded)" }
                },
                "required": ["ids", "status"]
            }
        }
```
Then update the `defines_all_tools_with_schemas` expected name vector: append `"capture_to_inbox", "list_inbox", "resolve_inbox"` after the current last name (find the actual last entry in the vector — do not assume which tool is last).

- [ ] **Step 6: Run tests**

Run: `cargo test _inbox` and `cargo test tools::tests`
Expected: PASS. `cargo build` clean.

- [ ] **Step 7: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): inbox capture/list/resolve tools"
```

---

## Task 3: Smart-routing, extraction & sort prompt guidance

**Files:**
- Modify: `src/assistant/agent.rs` (`SYSTEM`)

- [ ] **Step 1: Append guidance to `SYSTEM`**

In `src/assistant/agent.rs`, append to the END of the `SYSTEM` string literal, BEFORE the closing `";` (continue the `\`-joined style; ensure the prior fragment ends with ` \`):
```rust
 You also keep a quick-capture inbox. Decide per message: a vague or quick dump \
with no clear single action (e.g. 'inget beliin kado', 'ide fitur X') → call capture_to_inbox; \
a clear single actionable ('bayar pajak besok') → create it directly (todo/event/task) as usual; \
a longer note with several items → extract the items, echo a short summary (e.g. 'Kebaca: 3 todo, \
1 event …') and create them only after the user confirms. For 'apa di inbox?' call list_inbox. \
For 'sortir inbox' / 'beresin inbox': call list_inbox, propose a classification for every pending \
item in ONE message (todo/event/task/note — a note means save it with remember), and after the \
user confirms, create each item with the matching tool and then call resolve_inbox with the \
handled ids and status 'sorted' (use 'dropped' for ones the user discards).
```

- [ ] **Step 2: Build + sanity test**

Run: `cargo build` (clean) and `cargo test agent` (existing agent tests still pass).

- [ ] **Step 3: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/agent.rs
git commit -m "feat(assistant): smart capture routing + inbox sort prompt guidance"
```

---

## Task 4: Whisper transcription on NativeLlmClient

**Files:**
- Modify: `src/llm/native.rs` (`transcribe` + `extract_transcript` + test)

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `src/llm/native.rs`:
```rust
    #[test]
    fn extract_transcript_reads_text_field() {
        let resp = serde_json::json!({ "text": "  catat beli kado  " });
        assert_eq!(extract_transcript(&resp).unwrap(), "catat beli kado");
    }

    #[test]
    fn extract_transcript_errors_without_text() {
        let resp = serde_json::json!({ "error": "nope" });
        assert!(extract_transcript(&resp).is_err());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test extract_transcript_reads_text_field`
Expected: FAIL (`extract_transcript` not defined).

- [ ] **Step 3: Implement**

In `src/llm/native.rs`, add a free function (near `extract_native_text`):
```rust
/// Pull the transcript out of an OpenAI `audio/transcriptions` response
/// (`{ "text": "..." }`), trimmed.
pub fn extract_transcript(resp: &serde_json::Value) -> Result<String, LlmError> {
    resp.get("text")
        .and_then(|t| t.as_str())
        .map(|s| s.trim().to_string())
        .ok_or_else(|| LlmError::Shape("no text field in transcription response".into()))
}
```

Add a method on `impl NativeLlmClient` (after `complete`):
```rust
    /// Transcribe audio bytes via OpenAI Whisper (`/v1/audio/transcriptions`).
    /// `WHISPER_MODEL` overrides the model (default `whisper-1`). Reuses the
    /// vision provider's key/base_url.
    pub async fn transcribe(&self, audio: Vec<u8>, filename: &str, mime: &str) -> Result<String, LlmError> {
        let model = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "whisper-1".into());
        let url = format!("{}/v1/audio/transcriptions", self.base_url);
        let part = reqwest::multipart::Part::bytes(audio)
            .file_name(filename.to_string())
            .mime_str(mime)
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let form = reqwest::multipart::Form::new().text("model", model).part("file", part);
        let resp = self
            .client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
            .multipart(form)
            .send()
            .await
            .map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), body: json.to_string() });
        }
        extract_transcript(&json)
    }
```

> `reqwest`'s `multipart` feature must be enabled. Check `backend/Cargo.toml`: if the `reqwest` dependency's `features` list lacks `"multipart"`, add it. (`send_document` in `telegram/client.rs` already uses multipart, so it is very likely already enabled — verify, and only edit Cargo.toml if the build complains.)

- [ ] **Step 4: Run tests + build**

Run: `cargo test extract_transcript` and `cargo build`.
Expected: tests PASS; build clean (the new `transcribe` method is dead until Task 5 — warning OK).

- [ ] **Step 5: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/llm/native.rs backend/Cargo.toml
git commit -m "feat(llm): Whisper transcription on the OpenAI client"
```
(Only include `Cargo.toml` if you actually edited it.)

---

## Task 5: Telegram voice notes

**Files:**
- Modify: `src/telegram/client.rs` (`TgVoice` struct + `TgMessage.voice` + parse test)
- Modify: `src/telegram/mod.rs` (voice routing branch + helper + reply consts)

- [ ] **Step 1: Add the `TgVoice` struct + message field + parse test**

In `src/telegram/client.rs`, add after `TgDocument`:
```rust
#[derive(Debug, Deserialize)]
pub struct TgVoice {
    pub file_id: String,
    pub mime_type: Option<String>,
}
```
Add to `TgMessage`:
```rust
    /// Voice note (Telegram sends OGG/Opus).
    pub voice: Option<TgVoice>,
```
Add a parse test near `parse_updates_extracts_document_messages`:
```rust
    #[test]
    fn parse_updates_extracts_voice_messages() {
        let raw = serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 1,
                "message": { "chat": { "id": 7 }, "voice": { "file_id": "v1", "mime_type": "audio/ogg" } }
            }]
        }).to_string();
        let updates = parse_updates(&raw).unwrap();
        let voice = updates[0].message.as_ref().unwrap().voice.as_ref().unwrap();
        assert_eq!(voice.file_id, "v1");
        assert_eq!(voice.mime_type.as_deref(), Some("audio/ogg"));
    }
```
NOTE: every other place that constructs `TgMessage { ... }` literally (e.g. test helpers in `telegram/mod.rs` around the `msg()` helper) must add `voice: None,`. Grep `TgMessage {` and fix each so the crate compiles.

- [ ] **Step 2: Run the parse test**

Run: `cargo test parse_updates_extracts_voice_messages`
Expected: PASS (after fixing the `TgMessage { ... }` literals).

- [ ] **Step 3: Add reply consts + transcribe helper in `mod.rs`**

Near the other reply consts (LINK_OK_REPLY etc.) in `src/telegram/mod.rs`:
```rust
const VOICE_FAILED_REPLY: &str =
    "Maaf, gagal memproses voice note-nya. Coba lagi atau ketik aja ya.";
const VOICE_UNCLEAR_REPLY: &str =
    "Suaranya nggak kedengeran jelas — coba ulangi atau ketik aja ya.";
```
Add the helper (near `ingest_attachment`):
```rust
/// Download a voice note and transcribe it via Whisper.
async fn transcribe_voice(
    client: &TelegramClient,
    voice: &crate::telegram::client::TgVoice,
) -> anyhow::Result<String> {
    let file_path = client.get_file_path(&voice.file_id).await?;
    let bytes = client.download_file(&file_path).await?;
    let mime = voice.mime_type.clone().unwrap_or_else(|| "audio/ogg".into());
    let llm = crate::llm::native::NativeLlmClient::from_env()
        .map_err(|e| anyhow::anyhow!("transcription unavailable: {e}"))?;
    Ok(llm.transcribe(bytes, "voice.ogg", &mime).await?)
}
```

- [ ] **Step 4: Add the voice branch in `handle_update`**

In `handle_update`, the `Action::Answer` arm currently is `Action::Answer => match pick_attachment(&message) { ... }`. Wrap it so voice is handled first:
```rust
        Action::Answer => {
            if let Some(voice) = &message.voice {
                match transcribe_voice(client, voice).await {
                    Ok(transcript) if !transcript.trim().is_empty() => {
                        send_or_log(client, chat_id, &format!("Aku denger: {transcript}")).await;
                        let reply = answer(db, &transcript).await.unwrap_or_else(|e| {
                            tracing::error!("telegram: voice answer failed: {e:#}");
                            ANSWER_FAILED_REPLY.to_string()
                        });
                        send_or_log(client, chat_id, &reply).await;
                    }
                    Ok(_) => send_or_log(client, chat_id, VOICE_UNCLEAR_REPLY).await,
                    Err(e) => {
                        tracing::error!("telegram: transcription failed: {e:#}");
                        send_or_log(client, chat_id, VOICE_FAILED_REPLY).await;
                    }
                }
                return;
            }
            match pick_attachment(&message) {
                // ... existing AttachmentPick arms UNCHANGED ...
            }
        }
```
Keep the existing `AttachmentPick::Some/Unsupported/None` arms exactly as they are inside the new inner `match`.

- [ ] **Step 5: Build + test**

Run: `cargo build` (clean — no dead_code now that `transcribe` is used) and `cargo test telegram`.
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/telegram/client.rs backend/src/telegram/mod.rs
git commit -m "feat(telegram): voice notes → transcribe → normal pipeline"
```

---

## Final verification

- [ ] Run `cargo test` (all pass) and `cargo build` (0 warnings).

## Spec coverage check
- Quick-capture inbox (table + tools) → Tasks 1, 2.
- Batch sort + action-item extraction (prompt over existing tools) → Task 3.
- Smart routing → Task 3.
- Voice notes (Whisper + Telegram branch → normal pipeline) → Tasks 4, 5.
- "note" → memory (`remember`), no notes table → Task 3 guidance; honoured.
- Migration 0019; behavioral routing change; Whisper reuses OPENAI_API_KEY → throughout.
