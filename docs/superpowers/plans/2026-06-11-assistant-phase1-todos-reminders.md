# Personal Assistant Phase 1 (Todos & Reminders) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a tool-use agent (Claude Messages API tools) to the Rust backend that manages todos and reminders via Telegram chat, plus a 60-second delivery loop that sends due reminders to the linked Telegram chat.

**Architecture:** A new `assistant/` module holds tool definitions, a dispatcher (`match` on tool name → repo/service calls), and the agent loop (send conversation + tools to Claude, execute `tool_use` blocks, feed back `tool_result`, max 5 iterations). `llm/claude.rs` gains tool-capable request/response helpers. Telegram free-text messages route to the agent instead of the old portfolio-only `service::chat::answer`. A second background loop delivers due reminders every 60s.

**Tech Stack:** Rust (axum/tokio/sqlx/SQLite), Anthropic Messages API with tools, Telegram Bot API. **No new Cargo dependencies** — WIB is a fixed UTC+7 offset (no DST), so `chrono::FixedOffset` suffices; `async-trait` is already a dependency.

**Spec:** `docs/superpowers/specs/2026-06-11-assistant-phase1-todos-reminders-design.md`

**Conventions used throughout:**
- All commands run from `backend/`: `cd backend && cargo test <filter>`.
- Timestamps stored in SQLite as TEXT, UTC, second precision, trailing `Z` (`2026-06-12T02:00:00Z`) via the `to_db_utc` helper — one format everywhere so lexicographic `<=` equals chronological `<=`.
- Tests use `crate::db::connect("sqlite::memory:")` which runs migrations (existing pattern, see `repo/telegram_link.rs`).
- Commit after every task.

---

### Task 1: Migration + todos repo

**Files:**
- Create: `backend/migrations/0010_assistant.sql`
- Create: `backend/src/repo/todos.rs`
- Modify: `backend/src/repo/mod.rs` (module list at top)

- [ ] **Step 1: Write the migration**

`backend/migrations/0010_assistant.sql`:

```sql
-- Personal assistant phase 1: todos and reminders.
-- Timestamps are TEXT, UTC, second precision with trailing Z
-- ("2026-06-12T02:00:00Z") so lexicographic order is chronological order.
CREATE TABLE todos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  notes TEXT,
  due_at TEXT,
  status TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done')),
  created_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE reminders (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  todo_id INTEGER REFERENCES todos(id),
  message TEXT NOT NULL,
  remind_at TEXT NOT NULL,
  recurrence TEXT NOT NULL DEFAULT 'none'
    CHECK (recurrence IN ('none', 'daily', 'weekly', 'monthly')),
  status TEXT NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'sent', 'cancelled')),
  sent_at TEXT
);

CREATE INDEX idx_reminders_due ON reminders (status, remind_at);
```

- [ ] **Step 2: Write failing tests for the todos repo**

Create `backend/src/repo/todos.rs` with ONLY the test module first (plus the imports the tests need):

```rust
//! Persistence for assistant todos (see migration 0010).

use crate::db::Db;
use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let db = mem_db().await;
        let todo = create(&db, "bayar listrik", Some("token PLN"), Some("2026-06-12T02:00:00Z"))
            .await
            .unwrap();
        assert_eq!(todo.title, "bayar listrik");
        assert_eq!(todo.notes.as_deref(), Some("token PLN"));
        assert_eq!(todo.due_at.as_deref(), Some("2026-06-12T02:00:00Z"));
        assert_eq!(todo.status, "open");
        assert!(todo.completed_at.is_none());
        let fetched = get(&db, todo.id).await.unwrap();
        assert_eq!(fetched.id, todo.id);
    }

    #[tokio::test]
    async fn list_open_orders_by_due_then_id_and_excludes_done() {
        let db = mem_db().await;
        let no_due = create(&db, "no due", None, None).await.unwrap();
        let later = create(&db, "later", None, Some("2026-06-20T00:00:00Z")).await.unwrap();
        let sooner = create(&db, "sooner", None, Some("2026-06-12T00:00:00Z")).await.unwrap();
        let finished = create(&db, "done already", None, None).await.unwrap();
        complete(&db, finished.id).await.unwrap();

        let open = list_open(&db).await.unwrap();
        let ids: Vec<i64> = open.iter().map(|t| t.id).collect();
        // Dated todos first (earliest first), undated last; done excluded.
        assert_eq!(ids, vec![sooner.id, later.id, no_due.id]);
    }

    #[tokio::test]
    async fn complete_marks_done_once() {
        let db = mem_db().await;
        let todo = create(&db, "x", None, None).await.unwrap();
        assert!(complete(&db, todo.id).await.unwrap());
        let done = get(&db, todo.id).await.unwrap();
        assert_eq!(done.status, "done");
        assert!(done.completed_at.is_some());
        // Second completion is a no-op signalled by false.
        assert!(!complete(&db, todo.id).await.unwrap());
    }

    #[tokio::test]
    async fn complete_unknown_id_returns_false() {
        let db = mem_db().await;
        assert!(!complete(&db, 999).await.unwrap());
    }
}
```

- [ ] **Step 3: Register the module and run tests to verify they fail**

Add to the module list in `backend/src/repo/mod.rs` (alphabetical-ish; append after `pub mod telegram_link;`):

```rust
pub mod todos;
```

Run: `cd backend && cargo test repo::todos`
Expected: COMPILE ERROR — `create`, `get`, `list_open`, `complete`, `TodoRow` not found.

- [ ] **Step 4: Implement the repo**

Insert between the imports and the test module in `backend/src/repo/todos.rs`:

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct TodoRow {
    pub id: i64,
    pub title: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub status: String,
    pub created_at: String,
    pub completed_at: Option<String>,
}

pub async fn create(
    db: &Db,
    title: &str,
    notes: Option<&str>,
    due_at: Option<&str>,
) -> anyhow::Result<TodoRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO todos (title, notes, due_at, status, created_at) VALUES (?, ?, ?, 'open', ?)",
    )
    .bind(title)
    .bind(notes)
    .bind(due_at)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<TodoRow> {
    let row = sqlx::query_as::<_, TodoRow>("SELECT * FROM todos WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Open todos, earliest due first, undated last, then insertion order.
pub async fn list_open(db: &Db) -> anyhow::Result<Vec<TodoRow>> {
    let rows = sqlx::query_as::<_, TodoRow>(
        "SELECT * FROM todos WHERE status = 'open' ORDER BY due_at IS NULL, due_at, id",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Mark a todo done. Returns false when the id doesn't exist or is already done.
pub async fn complete(db: &Db, id: i64) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE todos SET status = 'done', completed_at = ? WHERE id = ? AND status = 'open'",
    )
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test repo::todos`
Expected: 4 tests PASS. Also run `cargo test` once — the new migration must not break existing tests.

- [ ] **Step 6: Commit**

```bash
git add backend/migrations/0010_assistant.sql backend/src/repo/todos.rs backend/src/repo/mod.rs
git commit -m "feat(assistant): add todos table and repo"
```

---

### Task 2: Reminders repo

**Files:**
- Create: `backend/src/repo/reminders.rs`
- Modify: `backend/src/repo/mod.rs`

- [ ] **Step 1: Write failing tests**

Create `backend/src/repo/reminders.rs` with imports + tests:

```rust
//! Persistence for assistant reminders (see migration 0010).

use crate::db::Db;
use serde::Serialize;

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_then_get_round_trips() {
        let db = mem_db().await;
        let r = create(&db, None, "bayar listrik", "2026-06-12T02:00:00Z", "none")
            .await
            .unwrap();
        assert_eq!(r.message, "bayar listrik");
        assert_eq!(r.remind_at, "2026-06-12T02:00:00Z");
        assert_eq!(r.recurrence, "none");
        assert_eq!(r.status, "pending");
        assert!(r.todo_id.is_none() && r.sent_at.is_none());
        assert_eq!(get(&db, r.id).await.unwrap().id, r.id);
    }

    #[tokio::test]
    async fn due_returns_only_pending_at_or_before_now() {
        let db = mem_db().await;
        let past = create(&db, None, "past", "2026-06-10T00:00:00Z", "none").await.unwrap();
        let exact = create(&db, None, "exact", "2026-06-11T00:00:00Z", "none").await.unwrap();
        create(&db, None, "future", "2026-06-12T00:00:00Z", "none").await.unwrap();
        let cancelled = create(&db, None, "cancelled", "2026-06-10T00:00:00Z", "none").await.unwrap();
        cancel(&db, cancelled.id).await.unwrap();

        let due_rows = due(&db, "2026-06-11T00:00:00Z").await.unwrap();
        let ids: Vec<i64> = due_rows.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![past.id, exact.id]);
    }

    #[tokio::test]
    async fn cancel_only_works_on_pending() {
        let db = mem_db().await;
        let r = create(&db, None, "x", "2026-06-12T00:00:00Z", "none").await.unwrap();
        assert!(cancel(&db, r.id).await.unwrap());
        assert_eq!(get(&db, r.id).await.unwrap().status, "cancelled");
        assert!(!cancel(&db, r.id).await.unwrap());
        assert!(!cancel(&db, 999).await.unwrap());
    }

    #[tokio::test]
    async fn mark_sent_finalizes_a_one_shot() {
        let db = mem_db().await;
        let r = create(&db, None, "x", "2026-06-11T00:00:00Z", "none").await.unwrap();
        mark_sent(&db, r.id, "2026-06-11T00:01:00Z").await.unwrap();
        let row = get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "sent");
        assert_eq!(row.sent_at.as_deref(), Some("2026-06-11T00:01:00Z"));
    }

    #[tokio::test]
    async fn reschedule_keeps_recurring_pending_with_new_time() {
        let db = mem_db().await;
        let r = create(&db, None, "daily", "2026-06-11T00:00:00Z", "daily").await.unwrap();
        reschedule(&db, r.id, "2026-06-12T00:00:00Z", "2026-06-11T00:01:00Z").await.unwrap();
        let row = get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "pending");
        assert_eq!(row.remind_at, "2026-06-12T00:00:00Z");
        assert_eq!(row.sent_at.as_deref(), Some("2026-06-11T00:01:00Z"));
    }

    #[tokio::test]
    async fn list_pending_orders_by_remind_at() {
        let db = mem_db().await;
        let later = create(&db, None, "later", "2026-06-13T00:00:00Z", "none").await.unwrap();
        let sooner = create(&db, None, "sooner", "2026-06-12T00:00:00Z", "none").await.unwrap();
        let sent = create(&db, None, "sent", "2026-06-10T00:00:00Z", "none").await.unwrap();
        mark_sent(&db, sent.id, "2026-06-10T00:01:00Z").await.unwrap();

        let pending = list_pending(&db).await.unwrap();
        let ids: Vec<i64> = pending.iter().map(|r| r.id).collect();
        assert_eq!(ids, vec![sooner.id, later.id]);
    }
}
```

- [ ] **Step 2: Register and verify failure**

Add `pub mod reminders;` to `backend/src/repo/mod.rs` (after `pub mod prices;`, anywhere in the list is fine).

Run: `cd backend && cargo test repo::reminders`
Expected: COMPILE ERROR — functions not found.

- [ ] **Step 3: Implement**

Insert between imports and tests:

```rust
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ReminderRow {
    pub id: i64,
    pub todo_id: Option<i64>,
    pub message: String,
    pub remind_at: String,
    pub recurrence: String,
    pub status: String,
    pub sent_at: Option<String>,
}

pub async fn create(
    db: &Db,
    todo_id: Option<i64>,
    message: &str,
    remind_at: &str,
    recurrence: &str,
) -> anyhow::Result<ReminderRow> {
    let id = sqlx::query(
        "INSERT INTO reminders (todo_id, message, remind_at, recurrence, status)
         VALUES (?, ?, ?, ?, 'pending')",
    )
    .bind(todo_id)
    .bind(message)
    .bind(remind_at)
    .bind(recurrence)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ReminderRow> {
    let row = sqlx::query_as::<_, ReminderRow>("SELECT * FROM reminders WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn list_pending(db: &Db) -> anyhow::Result<Vec<ReminderRow>> {
    let rows = sqlx::query_as::<_, ReminderRow>(
        "SELECT * FROM reminders WHERE status = 'pending' ORDER BY remind_at",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Pending reminders due at or before `now`. `now` must use the same
/// "%Y-%m-%dT%H:%M:%SZ" format as stored values so string <= is time <=.
pub async fn due(db: &Db, now: &str) -> anyhow::Result<Vec<ReminderRow>> {
    let rows = sqlx::query_as::<_, ReminderRow>(
        "SELECT * FROM reminders WHERE status = 'pending' AND remind_at <= ? ORDER BY remind_at",
    )
    .bind(now)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Cancel a pending reminder. Returns false when missing or not pending.
pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result =
        sqlx::query("UPDATE reminders SET status = 'cancelled' WHERE id = ? AND status = 'pending'")
            .bind(id)
            .execute(db)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Finalize a delivered one-shot reminder.
pub async fn mark_sent(db: &Db, id: i64, sent_at: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE reminders SET status = 'sent', sent_at = ? WHERE id = ?")
        .bind(sent_at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}

/// Recurring delivery: stay pending, advance remind_at, record sent_at.
pub async fn reschedule(db: &Db, id: i64, next_remind_at: &str, sent_at: &str) -> anyhow::Result<()> {
    sqlx::query("UPDATE reminders SET remind_at = ?, sent_at = ? WHERE id = ?")
        .bind(next_remind_at)
        .bind(sent_at)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test repo::reminders`
Expected: 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/reminders.rs backend/src/repo/mod.rs
git commit -m "feat(assistant): add reminders repo with due query and recurrence updates"
```

---

### Task 3: Time helpers (WIB ↔ UTC) + assistant module skeleton

**Files:**
- Create: `backend/src/assistant/mod.rs`
- Create: `backend/src/assistant/time.rs`
- Modify: `backend/src/main.rs:1-14` (module declarations)

- [ ] **Step 1: Create the module skeleton**

`backend/src/assistant/mod.rs`:

```rust
//! The personal-assistant agent: time helpers, tool definitions, dispatcher,
//! agent loop, and reminder delivery.

pub mod time;
```

In `backend/src/main.rs`, add after `mod api;`:

```rust
mod assistant;
```

- [ ] **Step 2: Write failing tests**

`backend/src/assistant/time.rs` with imports + tests:

```rust
//! Time helpers: the assistant speaks WIB (UTC+7), storage is UTC.

use chrono::{DateTime, FixedOffset, NaiveDateTime, TimeZone, Utc};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone as _;

    #[test]
    fn to_db_utc_uses_second_precision_z_format() {
        let dt = Utc.with_ymd_and_hms(2026, 6, 12, 2, 0, 0).unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:00Z");
    }

    #[test]
    fn parses_rfc3339_with_offset_to_utc() {
        // 09:00 WIB == 02:00 UTC
        let dt = parse_tool_datetime("2026-06-12T09:00:00+07:00").unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:00Z");
    }

    #[test]
    fn parses_naive_datetime_as_wib() {
        let dt = parse_tool_datetime("2026-06-12T09:00").unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:00Z");
        let dt = parse_tool_datetime("2026-06-12T09:00:30").unwrap();
        assert_eq!(to_db_utc(dt), "2026-06-12T02:00:30Z");
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_tool_datetime("besok jam 9").is_none());
        assert!(parse_tool_datetime("2026-06-12").is_none());
    }

    #[test]
    fn renders_stored_utc_as_wib() {
        assert_eq!(to_wib_display("2026-06-12T02:00:00Z"), "2026-06-12 09:00 WIB");
        // Unparseable values pass through untouched rather than panicking.
        assert_eq!(to_wib_display("oops"), "oops");
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend && cargo test assistant::time`
Expected: COMPILE ERROR — helpers not found.

- [ ] **Step 4: Implement**

Insert between imports and tests:

```rust
/// WIB (Asia/Jakarta) is UTC+7 year-round — no DST, so a fixed offset is safe.
pub fn wib() -> FixedOffset {
    FixedOffset::east_opt(7 * 3600).expect("+07:00 is a valid offset")
}

/// Format a UTC instant the way the assistant tables store timestamps:
/// second precision, trailing Z. One format everywhere keeps lexicographic
/// order equal to chronological order in SQL comparisons.
pub fn to_db_utc(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// Parse a datetime from a tool argument: RFC3339 (any offset) or a naive
/// "YYYY-MM-DDTHH:MM[:SS]" assumed WIB. Returns UTC; None when unparseable.
pub fn parse_tool_datetime(raw: &str) -> Option<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(raw) {
        return Some(dt.with_timezone(&Utc));
    }
    let naive = NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
        .or_else(|_| NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M"))
        .ok()?;
    wib().from_local_datetime(&naive)
        .single()
        .map(|dt| dt.with_timezone(&Utc))
}

/// Render a stored UTC timestamp as WIB for user-facing text. Unparseable
/// input is returned as-is (display helper — never fails).
pub fn to_wib_display(raw: &str) -> String {
    match DateTime::parse_from_rfc3339(raw) {
        Ok(dt) => dt.with_timezone(&wib()).format("%Y-%m-%d %H:%M WIB").to_string(),
        Err(_) => raw.to_string(),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::time`
Expected: 5 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant backend/src/main.rs
git commit -m "feat(assistant): add WIB/UTC time helpers and module skeleton"
```

---

### Task 4: Recurrence computation

**Files:**
- Create: `backend/src/assistant/recurrence.rs`
- Modify: `backend/src/assistant/mod.rs`

- [ ] **Step 1: Write failing tests**

`backend/src/assistant/recurrence.rs`:

```rust
//! Advance recurring reminders to their next occurrence.

use chrono::{DateTime, Duration, Months, Utc};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn utc(y: i32, mo: u32, d: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, 9, 0, 0).unwrap()
    }

    #[test]
    fn advances_daily_weekly_monthly() {
        assert_eq!(next_occurrence(utc(2026, 6, 11), "daily"), Some(utc(2026, 6, 12)));
        assert_eq!(next_occurrence(utc(2026, 6, 11), "weekly"), Some(utc(2026, 6, 18)));
        assert_eq!(next_occurrence(utc(2026, 6, 11), "monthly"), Some(utc(2026, 7, 11)));
    }

    #[test]
    fn monthly_clamps_to_month_end() {
        // Jan 31 + 1 month clamps to Feb 28 (2026 is not a leap year).
        assert_eq!(next_occurrence(utc(2026, 1, 31), "monthly"), Some(utc(2026, 2, 28)));
    }

    #[test]
    fn one_shot_has_no_next() {
        assert_eq!(next_occurrence(utc(2026, 6, 11), "none"), None);
        assert_eq!(next_occurrence(utc(2026, 6, 11), "yearly"), None);
    }

    #[test]
    fn next_after_skips_past_occurrences() {
        // A daily reminder delivered 3 days late schedules for tomorrow,
        // not for a time still in the past.
        let next = next_after(utc(2026, 6, 8), "daily", utc(2026, 6, 11)).unwrap();
        assert_eq!(next, utc(2026, 6, 12));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod recurrence;` to `backend/src/assistant/mod.rs`.

Run: `cd backend && cargo test assistant::recurrence`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement**

```rust
/// The next occurrence after `current`, or None for one-shot/unknown patterns.
pub fn next_occurrence(current: DateTime<Utc>, recurrence: &str) -> Option<DateTime<Utc>> {
    match recurrence {
        "daily" => Some(current + Duration::days(1)),
        "weekly" => Some(current + Duration::days(7)),
        "monthly" => current.checked_add_months(Months::new(1)),
        _ => None,
    }
}

/// Next occurrence strictly after `now` — repeatedly advances so a reminder
/// delivered late doesn't immediately fire again.
pub fn next_after(
    current: DateTime<Utc>,
    recurrence: &str,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let mut next = next_occurrence(current, recurrence)?;
    while next <= now {
        next = next_occurrence(next, recurrence)?;
    }
    Some(next)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::recurrence`
Expected: 4 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(assistant): add recurrence advancement for recurring reminders"
```

---

### Task 5: Tool-capable Claude client

**Files:**
- Modify: `backend/src/llm/claude.rs`

- [ ] **Step 1: Write failing tests**

Add to the existing `mod tests` in `backend/src/llm/claude.rs`:

```rust
    #[test]
    fn build_tools_body_includes_tools_and_messages_verbatim() {
        let messages = vec![serde_json::json!({ "role": "user", "content": "hi" })];
        let tools = serde_json::json!([{ "name": "create_todo" }]);
        let body = build_tools_body("claude-sonnet-4-6", "sys", &messages, &tools);
        assert_eq!(body["model"], "claude-sonnet-4-6");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["tools"][0]["name"], "create_todo");
    }

    #[test]
    fn extract_blocks_splits_text_and_tool_use() {
        let resp = serde_json::json!({ "content": [
            { "type": "text", "text": "checking" },
            { "type": "tool_use", "id": "tu_1", "name": "create_todo",
              "input": { "title": "bayar listrik" } }
        ]});
        let blocks = extract_blocks(&resp).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], ResponseBlock::Text("checking".into()));
        let ResponseBlock::ToolUse { id, name, input } = &blocks[1] else {
            panic!("expected tool_use")
        };
        assert_eq!(id, "tu_1");
        assert_eq!(name, "create_todo");
        assert_eq!(input["title"], "bayar listrik");
    }

    #[test]
    fn extract_blocks_rejects_empty_content() {
        let resp = serde_json::json!({ "content": [] });
        assert!(matches!(extract_blocks(&resp), Err(LlmError::Shape(_))));
    }

    #[test]
    fn extract_blocks_rejects_tool_use_without_name() {
        let resp = serde_json::json!({ "content": [
            { "type": "tool_use", "id": "tu_1", "input": {} }
        ]});
        assert!(matches!(extract_blocks(&resp), Err(LlmError::Shape(_))));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test llm::claude`
Expected: COMPILE ERROR — `build_tools_body`, `extract_blocks`, `ResponseBlock` not found.

- [ ] **Step 3: Implement**

Add after `extract_text` in `backend/src/llm/claude.rs`:

```rust
/// Build a Messages API body with tool definitions. `messages` are raw
/// message values — the agent loop threads tool_use/tool_result blocks
/// through verbatim.
pub fn build_tools_body(
    model: &str,
    system: &str,
    messages: &[serde_json::Value],
    tools: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": messages,
        "tools": tools,
    })
}

/// One content block of an API response, as the agent loop consumes it.
#[derive(Debug, PartialEq)]
pub enum ResponseBlock {
    Text(String),
    ToolUse { id: String, name: String, input: serde_json::Value },
}

/// Split a Messages API response into text and tool_use blocks.
pub fn extract_blocks(resp: &serde_json::Value) -> Result<Vec<ResponseBlock>, LlmError> {
    let content = resp
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::Shape("no content array".into()))?;
    let mut out = Vec::new();
    for block in content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    out.push(ResponseBlock::Text(t.to_string()));
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LlmError::Shape("tool_use without id".into()))?
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LlmError::Shape("tool_use without name".into()))?
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                out.push(ResponseBlock::ToolUse { id, name, input });
            }
            _ => {}
        }
    }
    if out.is_empty() {
        return Err(LlmError::Shape("no usable blocks".into()));
    }
    Ok(out)
}
```

In `impl ClaudeClient`, add `complete_tools` and refactor `post` to share a raw-JSON helper:

```rust
    /// Send a tool-enabled conversation; returns the FULL response JSON so
    /// the agent loop can replay assistant content verbatim.
    pub async fn complete_tools(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value, LlmError> {
        let body = build_tools_body(&self.model, system, messages, tools);
        self.post_json(body).await
    }

    async fn post(&self, body: serde_json::Value) -> Result<String, LlmError> {
        let json = self.post_json(body).await?;
        extract_text(&json)
    }

    /// POST to the Messages API and return the raw success-response JSON.
    async fn post_json(&self, body: serde_json::Value) -> Result<serde_json::Value, LlmError> {
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
        Ok(json)
    }
```

(The old `post` body moves into `post_json`; `post` keeps its signature so `complete`/`complete_chat` are untouched.)

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test llm::claude`
Expected: all tests PASS (existing 6 + new 4).

- [ ] **Step 5: Commit**

```bash
git add backend/src/llm/claude.rs
git commit -m "feat(llm): add tool-use request body and response-block parsing"
```

---

### Task 6: Tool definitions

**Files:**
- Create: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/mod.rs`

- [ ] **Step 1: Write failing tests**

`backend/src/assistant/tools.rs`:

```rust
//! JSON-schema tool definitions for the assistant agent (Messages API format).

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defines_all_phase1_tools_with_schemas() {
        let defs = definitions();
        let names: Vec<&str> = defs
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            vec![
                "create_todo", "list_todos", "complete_todo",
                "create_reminder", "list_reminders", "cancel_reminder",
                "get_portfolio_summary",
            ]
        );
        for tool in defs.as_array().unwrap() {
            assert!(tool["description"].is_string(), "{} needs a description", tool["name"]);
            assert_eq!(tool["input_schema"]["type"], "object");
        }
    }

    #[test]
    fn required_fields_are_marked() {
        let defs = definitions();
        let find = |name: &str| {
            defs.as_array().unwrap().iter()
                .find(|t| t["name"] == name).unwrap().clone()
        };
        assert_eq!(find("create_todo")["input_schema"]["required"], serde_json::json!(["title"]));
        assert_eq!(
            find("create_reminder")["input_schema"]["required"],
            serde_json::json!(["message", "remind_at"])
        );
        assert_eq!(find("complete_todo")["input_schema"]["required"], serde_json::json!(["id"]));
        assert_eq!(find("cancel_reminder")["input_schema"]["required"], serde_json::json!(["id"]));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod tools;` to `backend/src/assistant/mod.rs`.

Run: `cd backend && cargo test assistant::tools`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement**

```rust
/// All Phase 1 tools in Messages-API `tools` format.
pub fn definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "create_todo",
            "description": "Create a todo item for the user. Use when the user mentions a task they need to do.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Short task title in the user's words" },
                    "notes": { "type": "string", "description": "Optional extra detail" },
                    "due_at": { "type": "string", "description": "Optional deadline, RFC3339 with +07:00 offset, e.g. 2026-06-12T09:00:00+07:00" }
                },
                "required": ["title"]
            }
        },
        {
            "name": "list_todos",
            "description": "List the user's open todos with ids, titles, and due dates.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "complete_todo",
            "description": "Mark a todo as done. Look up the id with list_todos first if unsure.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Todo id" } },
                "required": ["id"]
            }
        },
        {
            "name": "create_reminder",
            "description": "Schedule a reminder message to be sent to the user at a specific time, optionally recurring.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "What to remind the user about" },
                    "remind_at": { "type": "string", "description": "When to fire, RFC3339 with +07:00 offset, must be in the future" },
                    "recurrence": { "type": "string", "enum": ["none", "daily", "weekly", "monthly"], "description": "Repeat pattern, default none" },
                    "todo_id": { "type": "integer", "description": "Optional todo this reminder belongs to" }
                },
                "required": ["message", "remind_at"]
            }
        },
        {
            "name": "list_reminders",
            "description": "List the user's pending reminders with ids, messages, and times.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "cancel_reminder",
            "description": "Cancel a pending reminder. Look up the id with list_reminders first if unsure.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Reminder id" } },
                "required": ["id"]
            }
        },
        {
            "name": "get_portfolio_summary",
            "description": "Get the user's current investment portfolio snapshot: net worth, P&L, XIRR, allocation, holdings. Use for any finance/portfolio question.",
            "input_schema": { "type": "object", "properties": {} }
        }
    ])
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::tools`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(assistant): define phase-1 tool schemas"
```

---

### Task 7: Tool dispatcher

**Files:**
- Create: `backend/src/assistant/dispatcher.rs`
- Modify: `backend/src/assistant/mod.rs`

- [ ] **Step 1: Write failing tests**

`backend/src/assistant/dispatcher.rs` (imports + tests first):

```rust
//! Execute one tool call against the database. Ok(text) feeds back to the
//! model as a tool_result; Err(text) becomes an is_error tool_result so the
//! model can self-correct or ask the user.

use crate::db::Db;
use super::time::{parse_tool_datetime, to_db_utc, to_wib_display};

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    /// A future remind_at the model would plausibly emit (WIB offset).
    fn future_wib() -> String {
        (chrono::Utc::now() + chrono::Duration::days(1))
            .with_timezone(&super::super::time::wib())
            .to_rfc3339()
    }

    #[tokio::test]
    async fn create_todo_inserts_and_reports() {
        let db = mem_db().await;
        let out = dispatch(&db, "create_todo", &serde_json::json!({
            "title": "bayar listrik", "due_at": "2026-06-12T09:00:00+07:00"
        })).await.unwrap();
        assert!(out.contains("bayar listrik"), "{out}");
        let todos = crate::repo::todos::list_open(&db).await.unwrap();
        assert_eq!(todos.len(), 1);
        // 09:00 WIB stored as 02:00 UTC.
        assert_eq!(todos[0].due_at.as_deref(), Some("2026-06-12T02:00:00Z"));
    }

    #[tokio::test]
    async fn create_todo_requires_title() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_todo", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("title"), "{err}");
    }

    #[tokio::test]
    async fn create_todo_rejects_bad_due_at() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_todo", &serde_json::json!({
            "title": "x", "due_at": "besok"
        })).await.unwrap_err();
        assert!(err.contains("besok"), "{err}");
    }

    #[tokio::test]
    async fn list_todos_renders_rows_or_empty_note() {
        let db = mem_db().await;
        assert_eq!(dispatch(&db, "list_todos", &serde_json::json!({})).await.unwrap(), "no open todos");
        crate::repo::todos::create(&db, "beli kado", None, None).await.unwrap();
        let out = dispatch(&db, "list_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("beli kado"), "{out}");
    }

    #[tokio::test]
    async fn complete_todo_round_trips_and_errors_when_done() {
        let db = mem_db().await;
        let todo = crate::repo::todos::create(&db, "x", None, None).await.unwrap();
        let out = dispatch(&db, "complete_todo", &serde_json::json!({ "id": todo.id })).await.unwrap();
        assert!(out.contains("done"), "{out}");
        let err = dispatch(&db, "complete_todo", &serde_json::json!({ "id": todo.id })).await.unwrap_err();
        assert!(err.contains("already done") || err.contains("not found"), "{err}");
    }

    #[tokio::test]
    async fn create_reminder_validates_and_inserts() {
        let db = mem_db().await;
        let out = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "bayar listrik", "remind_at": future_wib(), "recurrence": "daily"
        })).await.unwrap();
        assert!(out.contains("bayar listrik"), "{out}");
        let pending = crate::repo::reminders::list_pending(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].recurrence, "daily");
    }

    #[tokio::test]
    async fn create_reminder_rejects_past_times() {
        let db = mem_db().await;
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": past
        })).await.unwrap_err();
        assert!(err.contains("past"), "{err}");
    }

    #[tokio::test]
    async fn create_reminder_rejects_unknown_recurrence_and_todo() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": future_wib(), "recurrence": "hourly"
        })).await.unwrap_err();
        assert!(err.contains("hourly"), "{err}");
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": future_wib(), "todo_id": 999
        })).await.unwrap_err();
        assert!(err.contains("999"), "{err}");
    }

    #[tokio::test]
    async fn cancel_reminder_round_trips() {
        let db = mem_db().await;
        let r = crate::repo::reminders::create(&db, None, "x", "2099-01-01T00:00:00Z", "none")
            .await.unwrap();
        let out = dispatch(&db, "cancel_reminder", &serde_json::json!({ "id": r.id })).await.unwrap();
        assert!(out.contains("cancelled"), "{out}");
        let err = dispatch(&db, "cancel_reminder", &serde_json::json!({ "id": r.id })).await.unwrap_err();
        assert!(err.contains("not"), "{err}");
    }

    #[tokio::test]
    async fn unknown_tool_is_an_error() {
        let db = mem_db().await;
        let err = dispatch(&db, "fly_to_moon", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("fly_to_moon"), "{err}");
    }

    #[tokio::test]
    async fn portfolio_summary_renders_for_an_empty_db() {
        let db = mem_db().await;
        let out = dispatch(&db, "get_portfolio_summary", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Net worth"), "{out}");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod dispatcher;` to `backend/src/assistant/mod.rs`.

Run: `cd backend && cargo test assistant::dispatcher`
Expected: COMPILE ERROR — `dispatch` not found.

- [ ] **Step 3: Implement**

Insert between imports and tests:

```rust
/// Route one tool call by name.
pub async fn dispatch(db: &Db, name: &str, input: &serde_json::Value) -> Result<String, String> {
    match name {
        "create_todo" => create_todo(db, input).await,
        "list_todos" => list_todos(db).await,
        "complete_todo" => complete_todo(db, input).await,
        "create_reminder" => create_reminder(db, input).await,
        "list_reminders" => list_reminders(db).await,
        "cancel_reminder" => cancel_reminder(db, input).await,
        "get_portfolio_summary" => portfolio_summary(db).await,
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn str_arg<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty())
}

fn id_arg(input: &serde_json::Value, key: &str) -> Result<i64, String> {
    input
        .get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("missing integer argument '{key}'"))
}

async fn create_todo(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let title = str_arg(input, "title").ok_or("missing required argument 'title'")?;
    let due_at = match str_arg(input, "due_at") {
        Some(raw) => {
            let dt = parse_tool_datetime(raw)
                .ok_or_else(|| format!("unparseable due_at '{raw}' — use RFC3339 with +07:00"))?;
            Some(to_db_utc(dt))
        }
        None => None,
    };
    let todo = crate::repo::todos::create(db, title, str_arg(input, "notes"), due_at.as_deref())
        .await
        .map_err(|e| format!("db error: {e}"))?;
    Ok(format!("created todo #{} '{}'", todo.id, todo.title))
}

async fn list_todos(db: &Db) -> Result<String, String> {
    let todos = crate::repo::todos::list_open(db).await.map_err(|e| format!("db error: {e}"))?;
    if todos.is_empty() {
        return Ok("no open todos".into());
    }
    let mut out = String::new();
    for t in todos {
        out.push_str(&format!("#{} {}", t.id, t.title));
        if let Some(due) = &t.due_at {
            out.push_str(&format!(" (due {})", to_wib_display(due)));
        }
        if let Some(notes) = &t.notes {
            out.push_str(&format!(" — {notes}"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn complete_todo(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    let done = crate::repo::todos::complete(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if done {
        Ok(format!("todo #{id} marked done"))
    } else {
        Err(format!("todo #{id} not found or already done"))
    }
}

async fn create_reminder(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let message = str_arg(input, "message").ok_or("missing required argument 'message'")?;
    let raw = str_arg(input, "remind_at").ok_or("missing required argument 'remind_at'")?;
    let remind_at = parse_tool_datetime(raw)
        .ok_or_else(|| format!("unparseable remind_at '{raw}' — use RFC3339 with +07:00"))?;
    if remind_at <= chrono::Utc::now() {
        return Err(format!("remind_at '{raw}' is in the past — ask the user for a future time"));
    }
    let recurrence = str_arg(input, "recurrence").unwrap_or("none");
    if !matches!(recurrence, "none" | "daily" | "weekly" | "monthly") {
        return Err(format!("invalid recurrence '{recurrence}' — use none/daily/weekly/monthly"));
    }
    let todo_id = input.get("todo_id").and_then(|v| v.as_i64());
    if let Some(tid) = todo_id {
        crate::repo::todos::get(db, tid).await.map_err(|_| format!("todo #{tid} not found"))?;
    }
    let reminder =
        crate::repo::reminders::create(db, todo_id, message, &to_db_utc(remind_at), recurrence)
            .await
            .map_err(|e| format!("db error: {e}"))?;
    Ok(format!(
        "created reminder #{} '{}' at {}{}",
        reminder.id,
        reminder.message,
        to_wib_display(&reminder.remind_at),
        if reminder.recurrence == "none" { String::new() } else { format!(" (repeats {})", reminder.recurrence) },
    ))
}

async fn list_reminders(db: &Db) -> Result<String, String> {
    let reminders =
        crate::repo::reminders::list_pending(db).await.map_err(|e| format!("db error: {e}"))?;
    if reminders.is_empty() {
        return Ok("no pending reminders".into());
    }
    let mut out = String::new();
    for r in reminders {
        out.push_str(&format!("#{} '{}' at {}", r.id, r.message, to_wib_display(&r.remind_at)));
        if r.recurrence != "none" {
            out.push_str(&format!(" (repeats {})", r.recurrence));
        }
        if let Some(todo_id) = r.todo_id {
            out.push_str(&format!(" [todo #{todo_id}]"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn cancel_reminder(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    let cancelled =
        crate::repo::reminders::cancel(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if cancelled {
        Ok(format!("reminder #{id} cancelled"))
    } else {
        Err(format!("reminder #{id} not found or not pending"))
    }
}

async fn portfolio_summary(db: &Db) -> Result<String, String> {
    let summary = crate::service::portfolio::build_summary(db)
        .await
        .map_err(|e| format!("summary error: {e}"))?;
    let instruments =
        crate::repo::instruments::list(db).await.map_err(|e| format!("db error: {e}"))?;
    Ok(crate::service::chat::build_context(&summary, &instruments))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::dispatcher`
Expected: 11 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(assistant): add tool dispatcher with argument validation"
```

---

### Task 8: Agent loop

**Files:**
- Create: `backend/src/assistant/agent.rs`
- Modify: `backend/src/assistant/mod.rs`

- [ ] **Step 1: Write failing tests**

`backend/src/assistant/agent.rs` (imports + constants stubs come in step 3; write the file with imports and tests first):

```rust
//! The tool-use agent loop: conversation in, tools executed, final text out.

use crate::db::Db;
use crate::llm::claude::{extract_blocks, ClaudeClient, LlmError, ResponseBlock};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    /// Scripted model: pops one canned response per call, counts calls.
    struct ScriptedModel {
        responses: Mutex<VecDeque<serde_json::Value>>,
        calls: Mutex<usize>,
    }

    impl ScriptedModel {
        fn new(responses: Vec<serde_json::Value>) -> Self {
            Self { responses: Mutex::new(responses.into()), calls: Mutex::new(0) }
        }
        fn call_count(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    #[async_trait::async_trait]
    impl ToolModel for ScriptedModel {
        async fn complete_tools(
            &self,
            _system: &str,
            _messages: &[serde_json::Value],
            _tools: &serde_json::Value,
        ) -> Result<serde_json::Value, LlmError> {
            *self.calls.lock().unwrap() += 1;
            Ok(self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(serde_json::json!({ "content": [
                    { "type": "tool_use", "id": "loop", "name": "list_todos", "input": {} }
                ]})))
        }
    }

    fn text_response(text: &str) -> serde_json::Value {
        serde_json::json!({ "content": [{ "type": "text", "text": text }] })
    }

    #[tokio::test]
    async fn plain_text_response_is_returned_and_stored() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![text_response("halo!")]);
        let reply = handle_message(&db, &model, "telegram", "halo").await.unwrap();
        assert_eq!(reply, "halo!");
        let history = crate::repo::chat::recent_by_channel(&db, "telegram", 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "halo");
        assert_eq!(history[1].content, "halo!");
    }

    #[tokio::test]
    async fn tool_use_executes_against_the_db_then_replies() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![
            serde_json::json!({ "content": [
                { "type": "tool_use", "id": "tu_1", "name": "create_todo",
                  "input": { "title": "bayar listrik" } }
            ]}),
            text_response("Sip, todo dibuat."),
        ]);
        let reply = handle_message(&db, &model, "telegram", "catat: bayar listrik").await.unwrap();
        assert_eq!(reply, "Sip, todo dibuat.");
        assert_eq!(model.call_count(), 2);
        let todos = crate::repo::todos::list_open(&db).await.unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "bayar listrik");
    }

    #[tokio::test]
    async fn tool_errors_feed_back_and_the_model_recovers() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![
            serde_json::json!({ "content": [
                { "type": "tool_use", "id": "tu_1", "name": "complete_todo",
                  "input": { "id": 999 } }
            ]}),
            text_response("Todo #999 tidak ada."),
        ]);
        let reply = handle_message(&db, &model, "telegram", "selesaikan todo 999").await.unwrap();
        assert_eq!(reply, "Todo #999 tidak ada.");
    }

    #[tokio::test]
    async fn iteration_cap_returns_apology() {
        let db = mem_db().await;
        // Empty script: every call falls back to a tool_use response, forever.
        let model = ScriptedModel::new(vec![]);
        let reply = handle_message(&db, &model, "telegram", "x").await.unwrap();
        assert_eq!(reply, ITERATION_CAP_REPLY);
        assert_eq!(model.call_count(), MAX_ITERATIONS);
    }

    #[test]
    fn build_messages_drops_leading_assistant_history() {
        let history = vec![
            ("assistant".to_string(), "a0".to_string()),
            ("user".to_string(), "q1".to_string()),
            ("assistant".to_string(), "a1".to_string()),
        ];
        let messages = build_messages(&history, "q2");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[2]["content"], "q2");
    }

    #[test]
    fn system_prompt_embeds_the_current_time() {
        let prompt = system_prompt("2026-06-11T15:00:00+07:00");
        assert!(prompt.contains("2026-06-11T15:00:00+07:00"));
        assert!(prompt.contains("+07:00"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod agent;` to `backend/src/assistant/mod.rs`.

Run: `cd backend && cargo test assistant::agent`
Expected: COMPILE ERROR — `ToolModel`, `handle_message`, etc. not found.

- [ ] **Step 3: Implement**

Insert between imports and tests:

```rust
/// Hard cap on model round-trips per user message (cost / runaway guard).
pub const MAX_ITERATIONS: usize = 5;

/// How many prior messages of the channel's conversation the model sees.
const HISTORY_LIMIT: i64 = 12;

pub const ITERATION_CAP_REPLY: &str =
    "Maaf, permintaan ini terlalu rumit untuk diproses sekaligus. Coba pecah jadi beberapa pesan ya.";

const SYSTEM: &str = "You are a personal assistant for the app owner, reachable via Telegram. \
You manage todos and reminders and can answer questions about the owner's investment portfolio \
via the get_portfolio_summary tool. Reply in the user's language (usually Indonesian). \
Execute todo/reminder actions immediately without asking for confirmation, then summarize what \
you did, including ids and times (times in WIB). All datetimes in tool arguments must be RFC3339 \
with the +07:00 offset — the user's timezone is WIB (Asia/Jakarta). You are replying inside a \
plain-text messenger: do NOT use any Markdown (no tables, no headers, no **bold**). Write short \
lines; for lists use simple dashes or emoji.";

/// The slice of the LLM client the agent loop needs — a seam for test doubles.
#[async_trait::async_trait]
pub trait ToolModel {
    async fn complete_tools(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value, LlmError>;
}

#[async_trait::async_trait]
impl ToolModel for ClaudeClient {
    async fn complete_tools(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value, LlmError> {
        ClaudeClient::complete_tools(self, system, messages, tools).await
    }
}

/// System prompt with the current WIB time embedded, so the model can resolve
/// "besok jam 9" itself — no hand-written date parser.
fn system_prompt(now_wib: &str) -> String {
    format!("{SYSTEM}\n\nCurrent datetime: {now_wib}")
}

/// Prior turns as plain-text messages, then the new user message. Leading
/// assistant turns are dropped (API requires the first message to be a user's).
fn build_messages(history: &[(String, String)], user_msg: &str) -> Vec<serde_json::Value> {
    let first_user = history.iter().position(|(role, _)| role == "user").unwrap_or(history.len());
    let mut messages: Vec<serde_json::Value> = history[first_user..]
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    messages.push(serde_json::json!({ "role": "user", "content": user_msg }));
    messages
}

/// Render one dispatcher outcome as a tool_result block.
fn tool_result_block(id: &str, outcome: &Result<String, String>) -> serde_json::Value {
    match outcome {
        Ok(text) => serde_json::json!({
            "type": "tool_result", "tool_use_id": id, "content": text
        }),
        Err(text) => serde_json::json!({
            "type": "tool_result", "tool_use_id": id, "content": text, "is_error": true
        }),
    }
}

/// Run the agent loop for one inbound message. Stores the user message and
/// the final reply in chat history only on success (no orphaned rows).
pub async fn handle_message<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    channel: &str,
    user_msg: &str,
) -> anyhow::Result<String> {
    let now_wib = chrono::Utc::now().with_timezone(&super::time::wib()).to_rfc3339();
    let system = system_prompt(&now_wib);
    let tools = super::tools::definitions();
    let history: Vec<(String, String)> =
        crate::repo::chat::recent_by_channel(db, channel, HISTORY_LIMIT)
            .await
            .unwrap_or_default()
            .into_iter()
            .map(|m| (m.role, m.content))
            .collect();
    let mut messages = build_messages(&history, user_msg);

    for _ in 0..MAX_ITERATIONS {
        let resp = model
            .complete_tools(&system, &messages, &tools)
            .await
            .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
        let blocks = extract_blocks(&resp).map_err(|e| anyhow::anyhow!("llm shape error: {e}"))?;
        let tool_uses: Vec<(String, String, serde_json::Value)> = blocks
            .iter()
            .filter_map(|b| match b {
                ResponseBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if tool_uses.is_empty() {
            let reply: String = blocks
                .into_iter()
                .filter_map(|b| match b {
                    ResponseBlock::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            crate::repo::chat::add(db, "user", user_msg, channel).await?;
            crate::repo::chat::add(db, "assistant", &reply, channel).await?;
            return Ok(reply);
        }

        // Replay the assistant turn verbatim, then answer every tool_use.
        messages.push(serde_json::json!({ "role": "assistant", "content": resp["content"].clone() }));
        let mut results = Vec::new();
        for (id, name, input) in &tool_uses {
            let outcome = super::dispatcher::dispatch(db, name, input).await;
            tracing::info!(
                "assistant tool {name}: {}",
                if outcome.is_ok() { "ok" } else { "error" }
            );
            results.push(tool_result_block(id, &outcome));
        }
        messages.push(serde_json::json!({ "role": "user", "content": results }));
    }

    crate::repo::chat::add(db, "user", user_msg, channel).await?;
    crate::repo::chat::add(db, "assistant", ITERATION_CAP_REPLY, channel).await?;
    Ok(ITERATION_CAP_REPLY.to_string())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::agent`
Expected: 6 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant
git commit -m "feat(assistant): add tool-use agent loop with iteration cap"
```

---

### Task 9: Telegram — route text to the agent, add "Selesai" callback

**Files:**
- Modify: `backend/src/telegram/mod.rs` (CallbackAction at ~line 59-74, handle_callback at ~line 431, answer at ~line 343, tests)

- [ ] **Step 1: Write failing tests**

In `backend/src/telegram/mod.rs` tests, extend `parses_confirm_and_reject_callbacks` and add a todo-done test:

```rust
    #[test]
    fn parses_confirm_and_reject_callbacks() {
        assert_eq!(parse_callback("confirm:42"), Some(CallbackAction::Confirm(42)));
        assert_eq!(parse_callback("reject:7"), Some(CallbackAction::Reject(7)));
        assert_eq!(parse_callback("tododone:9"), Some(CallbackAction::TodoDone(9)));
        assert_eq!(parse_callback("nope:1"), None);
        assert_eq!(parse_callback("confirm:abc"), None);
        assert_eq!(parse_callback("confirm"), None);
    }

    #[tokio::test]
    async fn todo_done_text_completes_open_todos_once() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let todo = crate::repo::todos::create(&db, "bayar listrik", None, None).await.unwrap();
        let first = todo_done_text(&db, todo.id).await;
        assert!(first.contains("selesai"), "{first}");
        let again = todo_done_text(&db, todo.id).await;
        assert!(again.contains("sudah") || again.contains("tidak ditemukan"), "{again}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test telegram`
Expected: COMPILE ERROR — `TodoDone` variant and `todo_done_text` not found.

- [ ] **Step 3: Implement**

In `backend/src/telegram/mod.rs`:

1. Extend the enum and parser (replace the existing `CallbackAction` enum and `parse_callback`):

```rust
/// A parsed inline-button press.
#[derive(Debug, PartialEq, Eq)]
pub enum CallbackAction {
    Confirm(i64),
    Reject(i64),
    /// "✅ Selesai" on a reminder notification: mark its todo done.
    TodoDone(i64),
}

/// Parse callback_data ("confirm:<review_id>" / "reject:<review_id>" /
/// "tododone:<todo_id>").
pub fn parse_callback(data: &str) -> Option<CallbackAction> {
    let (action, id) = data.split_once(':')?;
    let id: i64 = id.parse().ok()?;
    match action {
        "confirm" => Some(CallbackAction::Confirm(id)),
        "reject" => Some(CallbackAction::Reject(id)),
        "tododone" => Some(CallbackAction::TodoDone(id)),
        _ => None,
    }
}
```

2. In `handle_callback`, replace the outcome/text block:

```rust
    let Some(action) = callback.data.as_deref().and_then(parse_callback) else { return };
    let text = match action {
        CallbackAction::Confirm(item_id) => {
            review_callback_text(item_id, confirm_item(db, item_id).await)
        }
        CallbackAction::Reject(item_id) => {
            review_callback_text(item_id, reject_item(db, item_id).await)
        }
        CallbackAction::TodoDone(todo_id) => todo_done_text(db, todo_id).await,
    };
    if let Err(e) = client.edit_message_text(chat_id, message.message_id, &text).await {
        tracing::error!("telegram: editMessageText failed: {e:#}");
    }
```

(The old `let (item_id, outcome) = match action { ... }` plus the two lines building `status`/`text` are removed.)

3. Add the two helpers next to `confirm_item`/`reject_item`:

```rust
/// Result line for review confirm/reject button presses.
fn review_callback_text(item_id: i64, outcome: anyhow::Result<String>) -> String {
    let status = outcome.unwrap_or_else(|e| format!("⚠️ {e:#}"));
    format!("🧾 Review #{item_id} — {status}")
}

/// Result line for the "✅ Selesai" button on a reminder notification.
async fn todo_done_text(db: &Db, todo_id: i64) -> String {
    match crate::repo::todos::complete(db, todo_id).await {
        Ok(true) => format!("✅ Todo #{todo_id} selesai."),
        Ok(false) => format!("Todo #{todo_id} sudah selesai atau tidak ditemukan."),
        Err(e) => format!("⚠️ {e:#}"),
    }
}
```

4. Route free text to the agent — replace the body of `answer`:

```rust
/// Answer a linked owner message via the assistant agent (tool-use loop).
async fn answer(db: &Db, text: &str) -> anyhow::Result<String> {
    let llm = crate::llm::claude::ClaudeClient::from_env()
        .map_err(|e| anyhow::anyhow!("chat unavailable: {e}"))?;
    crate::assistant::agent::handle_message(db, &llm, "telegram", text).await
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test telegram`
Expected: all telegram tests PASS (existing + 1 new, 1 extended).

- [ ] **Step 5: Commit**

```bash
git add backend/src/telegram/mod.rs
git commit -m "feat(telegram): route chat to the assistant agent, add todo-done button"
```

---

### Task 10: Reminder delivery loop + wire into main

**Files:**
- Create: `backend/src/assistant/reminder_tick.rs`
- Modify: `backend/src/assistant/mod.rs`
- Modify: `backend/src/main.rs:41-42` (spawn call)

- [ ] **Step 1: Write failing tests**

`backend/src/assistant/reminder_tick.rs` (imports + tests):

```rust
//! 60-second tick loop: deliver due reminders to the linked Telegram chat.

use crate::db::Db;
use crate::repo::reminders::ReminderRow;
use crate::telegram::client::TelegramClient;

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn one_shot_is_marked_sent() {
        let db = mem_db().await;
        let r = crate::repo::reminders::create(&db, None, "x", "2026-06-10T00:00:00Z", "none")
            .await.unwrap();
        finalize_delivered(&db, &r).await.unwrap();
        let row = crate::repo::reminders::get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "sent");
        assert!(row.sent_at.is_some());
    }

    #[tokio::test]
    async fn recurring_is_rescheduled_into_the_future() {
        let db = mem_db().await;
        // remind_at far in the past: next occurrence must land after now,
        // not at the next slot after the stale remind_at.
        let r = crate::repo::reminders::create(&db, None, "x", "2026-01-01T02:00:00Z", "daily")
            .await.unwrap();
        finalize_delivered(&db, &r).await.unwrap();
        let row = crate::repo::reminders::get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "pending");
        assert!(row.sent_at.is_some());
        let next = chrono::DateTime::parse_from_rfc3339(&row.remind_at).unwrap();
        assert!(next.with_timezone(&chrono::Utc) > chrono::Utc::now(), "{}", row.remind_at);
    }

    #[test]
    fn reminder_text_is_prefixed() {
        let row = ReminderRow {
            id: 1, todo_id: None, message: "bayar listrik".into(),
            remind_at: "2026-06-10T00:00:00Z".into(), recurrence: "none".into(),
            status: "pending".into(), sent_at: None,
        };
        assert_eq!(reminder_text(&row), "⏰ bayar listrik");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Add `pub mod reminder_tick;` to `backend/src/assistant/mod.rs`.

Run: `cd backend && cargo test assistant::reminder_tick`
Expected: COMPILE ERROR.

- [ ] **Step 3: Implement**

Insert between imports and tests:

```rust
const TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// Spawn the delivery loop when TELEGRAM_BOT_TOKEN is configured. Without the
/// token reminders still accumulate; they deliver once a token is set and the
/// backend restarts.
pub fn spawn(db: Db) {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else {
        tracing::info!("TELEGRAM_BOT_TOKEN not set; reminder delivery disabled");
        return;
    };
    tokio::spawn(async move {
        let client = TelegramClient::new(token);
        loop {
            if let Err(e) = tick(&db, &client).await {
                tracing::warn!("reminder tick failed: {e:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// The user-facing notification line for one reminder.
fn reminder_text(reminder: &ReminderRow) -> String {
    format!("⏰ {}", reminder.message)
}

/// Deliver every due reminder. A failed send leaves the reminder 'pending'
/// so the next tick retries — slightly late beats lost.
async fn tick(db: &Db, client: &TelegramClient) -> anyhow::Result<()> {
    let now = super::time::to_db_utc(chrono::Utc::now());
    let due = crate::repo::reminders::due(db, &now).await?;
    if due.is_empty() {
        return Ok(());
    }
    let Some(link) = crate::repo::telegram_link::get(db).await? else {
        tracing::warn!("{} reminder(s) due but no Telegram chat is linked", due.len());
        return Ok(());
    };
    for reminder in due {
        let text = reminder_text(&reminder);
        let send_result = match reminder.todo_id {
            Some(todo_id) => {
                let callback = format!("tododone:{todo_id}");
                client
                    .send_message_with_buttons(
                        link.chat_id,
                        &text,
                        &[("✅ Selesai", callback.as_str())],
                    )
                    .await
            }
            None => client.send_message(link.chat_id, &text).await,
        };
        if let Err(e) = send_result {
            tracing::warn!("reminder #{} send failed (will retry): {e:#}", reminder.id);
            continue;
        }
        finalize_delivered(db, &reminder).await?;
    }
    Ok(())
}

/// After a successful send: one-shots become 'sent'; recurring reminders are
/// rescheduled strictly past now so a late delivery doesn't refire at once.
async fn finalize_delivered(db: &Db, reminder: &ReminderRow) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let sent_at = super::time::to_db_utc(now);
    let current = chrono::DateTime::parse_from_rfc3339(&reminder.remind_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or(now);
    match super::recurrence::next_after(current, &reminder.recurrence, now) {
        Some(next) => {
            crate::repo::reminders::reschedule(
                db,
                reminder.id,
                &super::time::to_db_utc(next),
                &sent_at,
            )
            .await
        }
        None => crate::repo::reminders::mark_sent(db, reminder.id, &sent_at).await,
    }
}
```

- [ ] **Step 4: Wire into main**

In `backend/src/main.rs`, before `scheduler::spawn(db, ...)`:

```rust
    assistant::reminder_tick::spawn(db.clone());
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::reminder_tick`
Expected: 3 tests PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant backend/src/main.rs
git commit -m "feat(assistant): deliver due reminders via 60s Telegram tick loop"
```

---

### Task 11: Full verification + manual smoke test

- [ ] **Step 1: Run the full suite**

Run: `cd backend && cargo test`
Expected: ALL tests pass (the `#[ignore]` live LLM smoke tests stay ignored).

- [ ] **Step 2: Build release to catch warnings-as-surprises**

Run: `cd backend && cargo build`
Expected: clean build (warnings worth reading if any appear).

- [ ] **Step 3: Manual smoke test (requires real tokens — coordinate with the user)**

With `ANTHROPIC_API_KEY` and `TELEGRAM_BOT_TOKEN` set and the owner chat linked:

1. Start the backend (`cd backend && cargo run`).
2. Telegram: "ingetin aku bayar listrik besok jam 9 pagi" → expect a reply naming the reminder with the correct WIB time; `SELECT * FROM reminders;` shows a pending row with UTC `remind_at` (02:00:00Z).
3. Telegram: "todo: beli kado ulang tahun, catat juga ingetin aku nanti malam jam 8" → expect a todo plus a reminder linked to it (multi-tool message).
4. Telegram: "apa aja todo ku?" → expect the list.
5. Telegram: "berapa net worth ku?" → expect the portfolio answer (old capability intact).
6. Create a reminder 2 minutes out; wait for the ⏰ message. If attached to a todo, press "✅ Selesai" and confirm the message edits to "✅ Todo #N selesai."

- [ ] **Step 4: Final commit (if anything changed during verification)**

```bash
git status   # should be clean; commit any fixes with a descriptive message
```

---

## Self-Review Notes

- **Spec coverage:** agent loop + 5-iteration cap (Task 8), 7 tools (Tasks 6-7), migration/tables (Task 1-2), 60s tick + retry-on-failure + catch-up after downtime (Task 10 — `due` uses `<=`), "✅ Selesai" button (Tasks 9-10), WIB handling via model-emitted RFC3339 (Tasks 3, 6-8), existing flows untouched (Task 9 only swaps `answer`'s internals), apology on cap (Task 8). Out-of-scope items from the spec have no tasks, as intended.
- **Type consistency:** `dispatch(db, name, input) -> Result<String, String>` used by agent (Task 8) matches Task 7; `ToolModel::complete_tools` matches `ClaudeClient::complete_tools` (Task 5); repo signatures match dispatcher/tick call sites; `next_after` (Task 4) matches `finalize_delivered` (Task 10).
- **Known judgment call:** recurring reminders that fired while the backend was down deliver once, then reschedule past now (`next_after`) — they do not replay every missed occurrence.
