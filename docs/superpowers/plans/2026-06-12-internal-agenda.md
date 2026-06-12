# Internal Agenda (Phase 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Agenda events created/queried/cancelled via the Telegram agent (3 new tools) with automatic 30-minute pre-event reminders riding the existing at-least-once reminder loop, plus agenda sections in the morning briefing and weekly recap.

**Architecture:** New `events` table + `repo/events.rs`; `reminders` gains a nullable `event_id` (materialized pre-event reminders — Approach A from the spec, chosen for at-least-once delivery). Dispatcher handlers follow the established validation pattern. Briefing/recap gathers gain event range queries. No new send paths, no new failure modes.

**Tech Stack:** Rust, existing deps only.

**Spec:** `docs/superpowers/specs/2026-06-12-internal-agenda-design.md`

**Conventions:**
- Commands run from `backend/`; commit after every task; tests never set env vars.
- `events.start_at` and `reminders.remind_at` use the `%Y-%m-%dT%H:%M:%SZ` format (`assistant::time::to_db_utc`) so lexicographic compare is chronological; `created_at` is audit-only `to_rfc3339()`.
- Baseline: `cargo test` = 342 passed. Migration number 0013 verified against main (highest is 0012) — re-verify against origin/main before merging.

---

### Task 1: Migration + events repo

**Files:**
- Create: `backend/migrations/0013_events.sql`
- Create: `backend/src/repo/events.rs`
- Modify: `backend/src/repo/mod.rs`

- [ ] **Step 1: Write the migration**

`backend/migrations/0013_events.sql`:

```sql
-- Phase 3: agenda events. start_at is TEXT UTC Z-format (lexicographic ==
-- chronological, same as reminders.remind_at); created_at is audit RFC3339.
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  location TEXT,
  notes TEXT,
  start_at TEXT NOT NULL,
  status TEXT NOT NULL DEFAULT 'scheduled'
    CHECK (status IN ('scheduled', 'cancelled')),
  created_at TEXT NOT NULL
);

-- Pre-event reminders are materialized reminder rows linked to their event.
ALTER TABLE reminders ADD COLUMN event_id INTEGER REFERENCES events(id);
```

- [ ] **Step 2: Write failing tests** — create `backend/src/repo/events.rs`:

```rust
//! Persistence for agenda events (see migration 0013).

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
        let event = create(&db, "meeting vendor", Some("kantor"), None, "2026-06-13T07:00:00Z")
            .await
            .unwrap();
        assert_eq!(event.title, "meeting vendor");
        assert_eq!(event.location.as_deref(), Some("kantor"));
        assert!(event.notes.is_none());
        assert_eq!(event.start_at, "2026-06-13T07:00:00Z");
        assert_eq!(event.status, "scheduled");
        assert_eq!(get(&db, event.id).await.unwrap().id, event.id);
    }

    #[tokio::test]
    async fn list_between_is_inclusive_from_exclusive_to_and_skips_cancelled() {
        let db = mem_db().await;
        let at_from = create(&db, "at from", None, None, "2026-06-13T00:00:00Z").await.unwrap();
        let inside = create(&db, "inside", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        create(&db, "at to", None, None, "2026-06-14T00:00:00Z").await.unwrap();
        create(&db, "before", None, None, "2026-06-12T23:59:59Z").await.unwrap();
        let gone = create(&db, "cancelled", None, None, "2026-06-13T08:00:00Z").await.unwrap();
        cancel(&db, gone.id).await.unwrap();

        let events = list_between(&db, "2026-06-13T00:00:00Z", "2026-06-14T00:00:00Z")
            .await
            .unwrap();
        let ids: Vec<i64> = events.iter().map(|e| e.id).collect();
        assert_eq!(ids, vec![at_from.id, inside.id]);
    }

    #[tokio::test]
    async fn cancel_only_works_once_on_scheduled() {
        let db = mem_db().await;
        let event = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        assert!(cancel(&db, event.id).await.unwrap());
        assert_eq!(get(&db, event.id).await.unwrap().status, "cancelled");
        assert!(!cancel(&db, event.id).await.unwrap());
        assert!(!cancel(&db, 999).await.unwrap());
    }
}
```

- [ ] **Step 3: Register and verify failure**

Add `pub mod events;` to `backend/src/repo/mod.rs`.
Run: `cd backend && cargo test repo::events` — expect COMPILE ERROR.

- [ ] **Step 4: Implement** — insert between imports and tests:

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
}

pub async fn create(
    db: &Db,
    title: &str,
    location: Option<&str>,
    notes: Option<&str>,
    start_at: &str,
) -> anyhow::Result<EventRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO events (title, location, notes, start_at, status, created_at)
         VALUES (?, ?, ?, ?, 'scheduled', ?)",
    )
    .bind(title)
    .bind(location)
    .bind(notes)
    .bind(start_at)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<EventRow> {
    let row = sqlx::query_as::<_, EventRow>("SELECT * FROM events WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Scheduled events with start_at in [from_z, to_z), ordered by start time.
/// Bounds must use the Z format so string compare is time compare.
pub async fn list_between(db: &Db, from_z: &str, to_z: &str) -> anyhow::Result<Vec<EventRow>> {
    let rows = sqlx::query_as::<_, EventRow>(
        "SELECT * FROM events
         WHERE status = 'scheduled' AND start_at >= ? AND start_at < ?
         ORDER BY start_at",
    )
    .bind(from_z)
    .bind(to_z)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Cancel a scheduled event. False when missing or already cancelled.
pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result =
        sqlx::query("UPDATE events SET status = 'cancelled' WHERE id = ? AND status = 'scheduled'")
            .bind(id)
            .execute(db)
            .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test repo::events` — expect 3 PASS. Full `cargo test` — expect 345 (the migration's ALTER TABLE must not break existing reminder tests).

- [ ] **Step 6: Commit**

```bash
git add backend/migrations/0013_events.sql backend/src/repo/events.rs backend/src/repo/mod.rs
git commit -m "feat(agenda): add events table and repo"
```

---

### Task 2: Reminders gain event linkage

**Files:**
- Modify: `backend/src/repo/reminders.rs`
- Modify: `backend/src/assistant/reminder_tick.rs` (one test-literal fixup)

- [ ] **Step 1: Write failing tests.** Add to the tests module in `backend/src/repo/reminders.rs`:

```rust
    #[tokio::test]
    async fn create_for_event_links_and_flows_through_due() {
        let db = mem_db().await;
        let event = crate::repo::events::create(&db, "meeting", None, None, "2026-06-13T07:00:00Z")
            .await
            .unwrap();
        let r = create_for_event(&db, event.id, "📅 meeting — 30 menit lagi", "2026-06-13T06:30:00Z")
            .await
            .unwrap();
        assert_eq!(r.event_id, Some(event.id));
        assert!(r.todo_id.is_none());
        assert_eq!(r.recurrence, "none");
        let due_rows = due(&db, "2026-06-13T06:30:00Z").await.unwrap();
        assert_eq!(due_rows.len(), 1);
        assert_eq!(due_rows[0].event_id, Some(event.id));
    }

    #[tokio::test]
    async fn cancel_by_event_cancels_only_pending_linked_reminders() {
        let db = mem_db().await;
        let event = crate::repo::events::create(&db, "m", None, None, "2026-06-13T07:00:00Z")
            .await
            .unwrap();
        let linked = create_for_event(&db, event.id, "x", "2026-06-13T06:30:00Z").await.unwrap();
        let unlinked = create(&db, None, "y", "2026-06-13T06:30:00Z", "none").await.unwrap();
        assert!(cancel_by_event(&db, event.id).await.unwrap());
        assert_eq!(get(&db, linked.id).await.unwrap().status, "cancelled");
        assert_eq!(get(&db, unlinked.id).await.unwrap().status, "pending");
        // Second cancel finds nothing pending.
        assert!(!cancel_by_event(&db, event.id).await.unwrap());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test repo::reminders` — expect COMPILE ERROR (`event_id` field, `create_for_event`, `cancel_by_event` not found).

- [ ] **Step 3: Implement.**

(a) Add the field to `ReminderRow` (after `sent_at`):

```rust
    /// Set when this reminder is the automatic pre-event reminder of an
    /// agenda event; cancelled together with the event.
    pub event_id: Option<i64>,
```

(b) Add the two functions (after `reschedule`):

```rust
/// A pre-event reminder: one-shot, linked to its event for cascade-cancel.
pub async fn create_for_event(
    db: &Db,
    event_id: i64,
    message: &str,
    remind_at: &str,
) -> anyhow::Result<ReminderRow> {
    let id = sqlx::query(
        "INSERT INTO reminders (todo_id, message, remind_at, recurrence, status, event_id)
         VALUES (NULL, ?, ?, 'none', 'pending', ?)",
    )
    .bind(message)
    .bind(remind_at)
    .bind(event_id)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}

/// Cancel the pending reminder(s) linked to an event. False when none were.
pub async fn cancel_by_event(db: &Db, event_id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE reminders SET status = 'cancelled' WHERE event_id = ? AND status = 'pending'",
    )
    .bind(event_id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

(c) Fix the one struct-literal construction that now misses the field: in `backend/src/assistant/reminder_tick.rs`, the `reminder_text_is_prefixed` test builds a `ReminderRow { ... sent_at: None, }` — add `event_id: None,` to it.

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test repo::reminders assistant::reminder_tick` — all PASS (9 reminders incl. 2 new + 3 tick). Full `cargo test` — expect 347.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/reminders.rs backend/src/assistant/reminder_tick.rs
git commit -m "feat(agenda): link pre-event reminders to their event"
```

---

### Task 3: Tool schemas + agent prompt

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/agent.rs` (SYSTEM sentence + one test)

- [ ] **Step 1: Update failing tests.**

In `tools.rs`, extend `defines_all_tools_with_schemas`'s expected names to 12 (append after "remember"):

```rust
                "create_event", "list_events", "cancel_event",
```

Extend `required_fields_are_marked`:

```rust
        assert_eq!(
            find("create_event")["input_schema"]["required"],
            serde_json::json!(["title", "start_at"])
        );
        assert_eq!(find("cancel_event")["input_schema"]["required"], serde_json::json!(["id"]));
```

In `agent.rs` tests, add:

```rust
    #[test]
    fn system_prompt_mentions_the_agenda_tools() {
        let prompt = system_prompt("2026-06-12T15:00:00+07:00");
        assert!(prompt.contains("create_event"), "{prompt}");
        assert!(prompt.contains("list_events"), "{prompt}");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test assistant::tools assistant::agent` — expect FAIL.

- [ ] **Step 3: Implement.**

In `tools.rs` `definitions()`, append after the `remember` object:

```rust
        {
            "name": "create_event",
            "description": "Create an agenda event. Use for appointments and meetings ('meeting vendor besok jam 2'). A reminder fires 30 minutes before by default.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "What the event is" },
                    "start_at": { "type": "string", "description": "Start time, RFC3339 with +07:00 offset, must be in the future, e.g. 2026-06-13T14:00:00+07:00" },
                    "location": { "type": "string", "description": "Optional place" },
                    "notes": { "type": "string", "description": "Optional extra detail" },
                    "remind_minutes_before": { "type": "integer", "description": "Minutes before start to remind; default 30; 0 disables the reminder" }
                },
                "required": ["title", "start_at"]
            }
        },
        {
            "name": "list_events",
            "description": "List scheduled agenda events in a time range. Default: the next 7 days. Use for 'besok ada apa?', 'jadwal minggu ini'.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "from": { "type": "string", "description": "Range start, RFC3339 with +07:00; default now" },
                    "to": { "type": "string", "description": "Range end (exclusive), RFC3339 with +07:00; default from + 7 days" }
                }
            }
        },
        {
            "name": "cancel_event",
            "description": "Cancel a scheduled agenda event (its pre-event reminder is cancelled too). Look up the id with list_events first if unsure.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Event id" } },
                "required": ["id"]
            }
        }
```

In `agent.rs`, extend the `SYSTEM` const — append inside the string (before the closing quote, after the memory-tools sentence):

```
 You also manage the owner's agenda: create_event (a pre-event reminder is \
created automatically), list_events for schedule questions like 'besok ada \
apa?', and cancel_event.
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::tools assistant::agent` — all PASS (agent gains 1 test → 12; tools stays 2). Full `cargo test` — expect 348.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/tools.rs backend/src/assistant/agent.rs
git commit -m "feat(agenda): define event tool schemas and prompt guidance"
```

---

### Task 4: Dispatcher handlers

**Files:**
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Write failing tests.** Add to the tests module:

```rust
    #[tokio::test]
    async fn create_event_makes_event_and_default_linked_reminder() {
        let db = mem_db().await;
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        let out = dispatch(&db, "create_event", &serde_json::json!({
            "title": "meeting vendor", "start_at": start, "location": "kantor"
        })).await.unwrap();
        assert!(out.contains("meeting vendor"), "{out}");
        let events = crate::repo::events::list_between(&db, "2000-01-01T00:00:00Z", "2099-01-01T00:00:00Z")
            .await.unwrap();
        assert_eq!(events.len(), 1);
        let reminders = crate::repo::reminders::list_pending(&db).await.unwrap();
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].event_id, Some(events[0].id));
        assert!(reminders[0].message.contains("meeting vendor"), "{}", reminders[0].message);
        assert!(reminders[0].message.contains("kantor"), "{}", reminders[0].message);
        assert!(reminders[0].message.contains("30 menit"), "{}", reminders[0].message);
        // remind_at = start - 30 minutes (Z format, second precision).
        let start_dt = chrono::DateTime::parse_from_rfc3339(&start).unwrap();
        let expected = crate::assistant::time::to_db_utc(
            (start_dt - chrono::Duration::minutes(30)).with_timezone(&chrono::Utc),
        );
        assert_eq!(reminders[0].remind_at, expected);
    }

    #[tokio::test]
    async fn create_event_zero_minutes_skips_the_reminder() {
        let db = mem_db().await;
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start, "remind_minutes_before": 0
        })).await.unwrap();
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_event_too_soon_for_the_offset_skips_the_reminder() {
        let db = mem_db().await;
        // Starts in 10 minutes; the default 30-minute reminder would be in the past.
        let start = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
        let out = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start
        })).await.unwrap();
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
        assert!(out.contains("terlalu dekat"), "{out}");
    }

    #[tokio::test]
    async fn create_event_rejects_past_and_bad_input() {
        let db = mem_db().await;
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let err = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": past
        })).await.unwrap_err();
        assert!(err.contains("past"), "{err}");
        let err = dispatch(&db, "create_event", &serde_json::json!({ "title": "x" }))
            .await.unwrap_err();
        assert!(err.contains("start_at"), "{err}");
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        let err = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start, "remind_minutes_before": -5
        })).await.unwrap_err();
        assert!(err.contains("non-negative"), "{err}");
    }

    #[tokio::test]
    async fn list_events_defaults_to_a_week_and_renders_wib() {
        let db = mem_db().await;
        assert_eq!(
            dispatch(&db, "list_events", &serde_json::json!({})).await.unwrap(),
            "no events in that range"
        );
        crate::repo::events::create(
            &db, "meeting", Some("kantor"), None,
            &crate::assistant::time::to_db_utc(chrono::Utc::now() + chrono::Duration::days(2)),
        ).await.unwrap();
        // 9 days out: outside the default window.
        crate::repo::events::create(
            &db, "far away", None, None,
            &crate::assistant::time::to_db_utc(chrono::Utc::now() + chrono::Duration::days(9)),
        ).await.unwrap();
        let out = dispatch(&db, "list_events", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("meeting"), "{out}");
        assert!(out.contains("kantor"), "{out}");
        assert!(out.contains("WIB"), "{out}");
        assert!(!out.contains("far away"), "{out}");
    }

    #[tokio::test]
    async fn cancel_event_cascades_to_its_reminder() {
        let db = mem_db().await;
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        dispatch(&db, "create_event", &serde_json::json!({ "title": "m", "start_at": start }))
            .await.unwrap();
        let event_id = crate::repo::events::list_between(&db, "2000-01-01T00:00:00Z", "2099-01-01T00:00:00Z")
            .await.unwrap()[0].id;
        let out = dispatch(&db, "cancel_event", &serde_json::json!({ "id": event_id }))
            .await.unwrap();
        assert!(out.contains("cancelled"), "{out}");
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
        let err = dispatch(&db, "cancel_event", &serde_json::json!({ "id": event_id }))
            .await.unwrap_err();
        assert!(err.contains("not found or already cancelled"), "{err}");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test assistant::dispatcher` — new tests FAIL on the `unknown tool` arm.

- [ ] **Step 3: Implement.**

Add to the `dispatch` match (before `_`):

```rust
        "create_event" => create_event(db, input).await,
        "list_events" => list_events(db, input).await,
        "cancel_event" => cancel_event(db, input).await,
```

Add handlers (after `remember`):

```rust
/// Default lead time for the automatic pre-event reminder.
const DEFAULT_EVENT_REMIND_MINUTES: i64 = 30;
/// Default lookahead for list_events.
const DEFAULT_EVENT_RANGE_DAYS: i64 = 7;

async fn create_event(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let title = str_arg(input, "title").ok_or("missing required argument 'title'")?;
    let raw = str_arg(input, "start_at").ok_or("missing required argument 'start_at'")?;
    let start = parse_tool_datetime(raw)
        .ok_or_else(|| format!("unparseable start_at '{raw}' — use RFC3339 with +07:00"))?;
    if start <= chrono::Utc::now() {
        return Err(format!("start_at '{raw}' is in the past — ask the user for a future time"));
    }
    let remind_minutes = match input.get("remind_minutes_before") {
        None | Some(serde_json::Value::Null) => DEFAULT_EVENT_REMIND_MINUTES,
        Some(v) => v
            .as_i64()
            .filter(|m| *m >= 0)
            .ok_or("remind_minutes_before must be a non-negative integer")?,
    };
    let event = crate::repo::events::create(
        db,
        title,
        str_arg(input, "location"),
        str_arg(input, "notes"),
        &to_db_utc(start),
    )
    .await
    .map_err(|e| format!("db error: {e}"))?;

    let reminder_note = if remind_minutes == 0 {
        String::new()
    } else {
        let remind_at = start - chrono::Duration::minutes(remind_minutes);
        if remind_at <= chrono::Utc::now() {
            " (terlalu dekat untuk reminder otomatis)".to_string()
        } else {
            let location_part = event
                .location
                .as_deref()
                .map(|l| format!(" di {l}"))
                .unwrap_or_default();
            let message =
                format!("📅 {}{} — {} menit lagi", event.title, location_part, remind_minutes);
            crate::repo::reminders::create_for_event(db, event.id, &message, &to_db_utc(remind_at))
                .await
                .map_err(|e| format!("event created but reminder failed: {e}"))?;
            format!(" (reminder {remind_minutes} menit sebelumnya dibuat)")
        }
    };
    Ok(format!(
        "created event #{} '{}' at {}{}",
        event.id,
        event.title,
        to_wib_display(&event.start_at),
        reminder_note,
    ))
}

async fn list_events(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let now = chrono::Utc::now();
    let from = match str_arg(input, "from") {
        Some(raw) => parse_tool_datetime(raw)
            .ok_or_else(|| format!("unparseable from '{raw}' — use RFC3339 with +07:00"))?,
        None => now,
    };
    let to = match str_arg(input, "to") {
        Some(raw) => parse_tool_datetime(raw)
            .ok_or_else(|| format!("unparseable to '{raw}' — use RFC3339 with +07:00"))?,
        None => from + chrono::Duration::days(DEFAULT_EVENT_RANGE_DAYS),
    };
    let events = crate::repo::events::list_between(db, &to_db_utc(from), &to_db_utc(to))
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if events.is_empty() {
        return Ok("no events in that range".into());
    }
    let mut out = String::new();
    for e in events {
        out.push_str(&format!("- #{} {}: {}", e.id, to_wib_display(&e.start_at), e.title));
        if let Some(location) = &e.location {
            out.push_str(&format!(" ({location})"));
        }
        if let Some(notes) = &e.notes {
            out.push_str(&format!(" — {notes}"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn cancel_event(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    let cancelled =
        crate::repo::events::cancel(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if !cancelled {
        return Err(format!("event #{id} not found or already cancelled"));
    }
    let reminder_cancelled = crate::repo::reminders::cancel_by_event(db, id)
        .await
        .unwrap_or(false);
    Ok(format!(
        "event #{id} cancelled{}",
        if reminder_cancelled { " (its reminder too)" } else { "" }
    ))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::dispatcher` — 19 PASS (13 + 6 new). Full `cargo test` — expect 354.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/dispatcher.rs
git commit -m "feat(agenda): dispatch create/list/cancel event tools"
```

---

### Task 5: Briefing & recap agenda sections

**Files:**
- Modify: `backend/src/assistant/proactive/briefing.rs`
- Modify: `backend/src/assistant/proactive/recap.rs`

- [ ] **Step 1: Write failing tests.**

In `briefing.rs` tests: add `events_today: vec![]` to the `data()` constructor, and add:

```rust
    #[test]
    fn agenda_section_renders_events_with_wib_time_and_location() {
        let mut d = data();
        d.events_today = vec![crate::repo::events::EventRow {
            id: 1,
            title: "meeting vendor".into(),
            location: Some("kantor".into()),
            notes: None,
            start_at: "2026-06-12T07:00:00Z".into(), // 14:00 WIB
            status: "scheduled".into(),
            created_at: String::new(),
        }];
        let block = render_data_block(&d);
        assert!(block.contains("Agenda hari ini:"), "{block}");
        assert!(block.contains("meeting vendor (kantor)"), "{block}");
        assert!(block.contains("14:00 WIB"), "{block}");
    }
```

(The existing `empty_sections_say_so_instead_of_vanishing` keeps passing because the agenda section also renders `(tidak ada)` when empty.)

In `recap.rs` tests: add `events_next_week: vec![]` to the `RecapData` literal in `block_renders_productivity_finance_and_next_week`, and add:

```rust
    #[test]
    fn next_week_section_includes_events() {
        let mut d = RecapData {
            week_label: "2026-W24".into(),
            todos_completed: 0,
            todos_created: 0,
            reminders_sent: 0,
            net_worth_idr: dec!(0),
            week_delta_idr: None,
            spending_idr: dec!(0),
            spending_skipped_non_idr: 0,
            movers: vec![],
            todos_next_week: vec![],
            reminders_next_week: vec![],
            events_next_week: vec![],
        };
        d.events_next_week = vec![crate::repo::events::EventRow {
            id: 1,
            title: "kontrol gigi".into(),
            location: None,
            notes: None,
            start_at: "2026-06-17T02:00:00Z".into(),
            status: "scheduled".into(),
            created_at: String::new(),
        }];
        let block = render_data_block(&d);
        assert!(block.contains("- event: kontrol gigi"), "{block}");
        assert!(!block.contains("(tidak ada jadwal tercatat)"), "{block}");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test assistant::proactive` — COMPILE ERROR (missing struct fields).

- [ ] **Step 3: Implement.**

`briefing.rs`:
- `BriefingData` gains `pub events_today: Vec<crate::repo::events::EventRow>,` (after `reminders_today`).
- In `gather()`, after the `reminders_today` block:

```rust
    // Today's WIB calendar day expressed as a UTC range.
    let day_start = now_wib
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_local_timezone(crate::assistant::time::wib())
        .single()
        .expect("WIB has no DST gaps")
        .with_timezone(&chrono::Utc);
    let events_today = crate::repo::events::list_between(
        db,
        &crate::assistant::time::to_db_utc(day_start),
        &crate::assistant::time::to_db_utc(day_start + chrono::Duration::days(1)),
    )
    .await?;
```
- Add `events_today,` to the `Ok(BriefingData { ... })` literal.
- In `render_data_block`, after the reminders section:

```rust
    out.push_str("Agenda hari ini:\n");
    if d.events_today.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for e in &d.events_today {
            out.push_str(&format!(
                "- {}: {}",
                crate::assistant::time::to_wib_display(&e.start_at),
                e.title
            ));
            if let Some(location) = &e.location {
                out.push_str(&format!(" ({location})"));
            }
            out.push('\n');
        }
    }
```

`recap.rs`:
- `RecapData` gains `pub events_next_week: Vec<crate::repo::events::EventRow>,` (after `reminders_next_week`).
- In `gather()`, after the `reminders_next_week` block:

```rust
    let events_next_week =
        crate::repo::events::list_between(db, &now_z, &next_week_end).await?;
```
- Add `events_next_week,` to the `Ok(RecapData { ... })` literal.
- In `render_data_block`, change the empty check to include events and render them first in the section:

```rust
    out.push_str("Minggu depan:\n");
    if d.todos_next_week.is_empty()
        && d.reminders_next_week.is_empty()
        && d.events_next_week.is_empty()
    {
        out.push_str("(tidak ada jadwal tercatat)\n");
    } else {
        for e in &d.events_next_week {
            out.push_str(&format!(
                "- event: {} ({})\n",
                e.title,
                crate::assistant::time::to_wib_display(&e.start_at)
            ));
        }
```
(existing todo/reminder loops stay inside the `else`).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test assistant::proactive` — all PASS (+2). Full `cargo test` — expect 356. `cargo build` — 0 warnings.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/proactive
git commit -m "feat(agenda): add agenda sections to briefing and recap"
```

---

### Task 6: Full verification

- [ ] **Step 1:** `cd backend && cargo test` — ALL pass (expect 356; trust measured). `cargo build` — 0 warnings.
- [ ] **Step 2:** Manual smoke (after deploy): "meeting vendor besok jam 2 siang di kantor" → event + reminder note in reply; "besok ada apa?" → the event listed; 30 minutes before → "📅 meeting vendor di kantor — 30 menit lagi" arrives; "batalin meeting vendor" → cancelled, no reminder fires; next morning's briefing shows "Agenda hari ini".
- [ ] **Step 3:** `git status` clean.

---

## Self-Review Notes

- **Spec coverage:** migration + repo (Task 1), reminder linkage with cascade (Task 2), 3 tools + prompt (Tasks 3-4) incl. default-30/zero/too-soon reminder semantics and future-only start, briefing/recap sections (Task 5), delivery via existing loop (no task needed — Task 2's `due` test proves flow-through). Out-of-scope items have no tasks.
- **Type consistency:** `EventRow` fields match across repo (Task 1), dispatcher rendering (Task 4), and briefing/recap tests (Task 5); `create_for_event`/`cancel_by_event` signatures match their Task 4 call sites; `ReminderRow.event_id` addition's single literal fixup is identified explicitly (reminder_tick test).
- **Judgment calls:** (1) `create_for_event` instead of widening `create`'s signature — avoids touching ~10 existing call sites; spec intent (linked reminder) preserved. (2) Event reminders deliberately have no "Selesai" button (todo_id NULL) — matches spec. (3) `list_between` is start-exclusive on `to` so adjacent day ranges never double-count.
