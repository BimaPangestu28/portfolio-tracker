# Daily Planning & Evening Review Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the morning briefing into a prioritised day plan, add an on-demand `plan_day` chat tool, and add a daily evening review that offers confirm-gated rollover of unfinished todos.

**Architecture:** Add `priority` + `estimate_minutes` to todos (migration 0016). A new `proactive/plan.rs` holds the shared todo-ordering logic and the on-demand day-plan block. A new `proactive/evening_review.rs` is a daily proactive job mirroring the existing briefing/recap pattern. Two new agent tools (`plan_day`, `rollover_todos`) and a repo `rollover` function complete the chat surface.

**Tech Stack:** Rust, sqlx (SQLite), chrono, serde_json, tokio. Tests run with `cargo test <filter>` from `backend/` (this is a bin crate — never use `cargo test --lib`, and never run `cargo fmt`).

---

## Conventions for every task

- Work from `backend/` (all paths below are relative to `backend/`).
- `due_at` / `start_at` are stored UTC `Z`-format strings (lexicographic == chronological). Always write times through `crate::assistant::time::to_db_utc(dt)`.
- `completed_at` is written by `todos::complete` as `chrono::Utc::now().to_rfc3339()` (`+00:00` format). Build `completed_since` bounds with `DateTime::<Utc>::to_rfc3339()` so the string comparison matches.
- WIB timezone helper: `crate::assistant::time::wib()`.
- Commit after each task with the message shown.

---

## Task 1: Add `priority` + `estimate_minutes` to todos

**Files:**
- Create: `migrations/0016_todo_priority_estimate.sql`
- Modify: `src/repo/todos.rs` (struct, `create` signature + INSERT, tests)
- Modify (call sites): `src/assistant/dispatcher.rs:86`, `src/assistant/dispatcher.rs:622`, `src/assistant/dispatcher.rs:630`, `src/telegram/mod.rs:525`

- [ ] **Step 1: Write the migration**

Create `migrations/0016_todo_priority_estimate.sql`:

```sql
-- Fase 2: todo priority + duration estimate for day planning.
-- priority NULL is treated as 'normal' by the application.
ALTER TABLE todos ADD COLUMN priority TEXT
  CHECK (priority IN ('high', 'normal', 'low'));
ALTER TABLE todos ADD COLUMN estimate_minutes INTEGER;
```

- [ ] **Step 2: Extend `TodoRow` and `create`**

In `src/repo/todos.rs`, add the two fields to the struct (after `completed_at`):

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
    pub priority: Option<String>,
    pub estimate_minutes: Option<i64>,
}
```

Replace `create` with the extended signature + INSERT:

```rust
pub async fn create(
    db: &Db,
    title: &str,
    notes: Option<&str>,
    due_at: Option<&str>,
    priority: Option<&str>,
    estimate_minutes: Option<i64>,
) -> anyhow::Result<TodoRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO todos (title, notes, due_at, status, created_at, priority, estimate_minutes) \
         VALUES (?, ?, ?, 'open', ?, ?, ?)",
    )
    .bind(title)
    .bind(notes)
    .bind(due_at)
    .bind(&now)
    .bind(priority)
    .bind(estimate_minutes)
    .execute(db)
    .await?
    .last_insert_rowid();
    get(db, id).await
}
```

- [ ] **Step 3: Update the failing test call sites in `todos.rs`**

In `src/repo/todos.rs` tests, update every `create(&db, ...)` call to pass the two new args. The round-trip test also asserts the new fields:

```rust
    #[tokio::test]
    async fn create_then_get_round_trips() {
        let db = mem_db().await;
        let todo = create(
            &db,
            "bayar listrik",
            Some("token PLN"),
            Some("2026-06-12T02:00:00Z"),
            Some("high"),
            Some(30),
        )
        .await
        .unwrap();
        assert_eq!(todo.title, "bayar listrik");
        assert_eq!(todo.notes.as_deref(), Some("token PLN"));
        assert_eq!(todo.due_at.as_deref(), Some("2026-06-12T02:00:00Z"));
        assert_eq!(todo.status, "open");
        assert!(todo.completed_at.is_none());
        assert_eq!(todo.priority.as_deref(), Some("high"));
        assert_eq!(todo.estimate_minutes, Some(30));
        let fetched = get(&db, todo.id).await.unwrap();
        assert_eq!(fetched.id, todo.id);
    }
```

For the other `create(&db, ...)` calls in this file (lines ~115-150: `no due`, `later`, `sooner`, `done already`, `x`, `old done`, `new open`), append `, None, None` before `.await`. Example:

```rust
        let no_due = create(&db, "no due", None, None, None, None).await.unwrap();
        let later = create(&db, "later", None, Some("2026-06-20T00:00:00Z"), None, None).await.unwrap();
        let sooner = create(&db, "sooner", None, Some("2026-06-12T00:00:00Z"), None, None).await.unwrap();
        let finished = create(&db, "done already", None, None, None, None).await.unwrap();
```
(Apply the same `, None, None` to the `x`, `old done`, `new open` calls.)

- [ ] **Step 4: Update the production + other-test call sites**

`src/assistant/dispatcher.rs:86` — leave as-is for now; it is rewritten in Task 2. To keep the crate compiling between tasks, update it minimally here:

```rust
    let todo = crate::repo::todos::create(db, title, str_arg(input, "notes"), due_at.as_deref(), None, None)
        .await
        .map_err(|e| format!("db error: {e}"))?;
```

`src/assistant/dispatcher.rs:622` and `:630`, and `src/telegram/mod.rs:525` — append `, None, None`:

```rust
        crate::repo::todos::create(&db, "beli kado", None, None, None, None).await.unwrap();
```
```rust
        let todo = crate::repo::todos::create(&db, "x", None, None, None, None).await.unwrap();
```
```rust
        let todo = crate::repo::todos::create(&db, "bayar listrik", None, None, None, None).await.unwrap();
```

- [ ] **Step 5: Run tests**

Run: `cargo test todos::tests`
Expected: PASS (migration applies on the in-memory DB; round-trip asserts new fields).

- [ ] **Step 6: Commit**

```bash
git add migrations/0016_todo_priority_estimate.sql src/repo/todos.rs src/assistant/dispatcher.rs src/telegram/mod.rs
git commit -m "feat(todos): add priority + estimate_minutes columns (migration 0016)"
```

---

## Task 2: Surface priority/estimate through the create_todo tool

**Files:**
- Modify: `src/assistant/tools.rs` (create_todo schema)
- Modify: `src/assistant/dispatcher.rs` (`create_todo`, `list_todos`)

- [ ] **Step 1: Write the failing test**

Add to `src/assistant/dispatcher.rs` tests (near `create_todo_inserts_and_reports`):

```rust
    #[tokio::test]
    async fn create_todo_stores_priority_and_estimate() {
        let db = mem_db().await;
        dispatch(&db, "create_todo", &serde_json::json!({
            "title": "siapin deck",
            "priority": "high",
            "estimate_minutes": 45
        })).await.unwrap();
        let todos = crate::repo::todos::list_open(&db).await.unwrap();
        assert_eq!(todos[0].priority.as_deref(), Some("high"));
        assert_eq!(todos[0].estimate_minutes, Some(45));
    }

    #[tokio::test]
    async fn create_todo_rejects_bad_priority() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_todo", &serde_json::json!({
            "title": "x", "priority": "urgent"
        })).await.unwrap_err();
        assert!(err.contains("priority"), "{err}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test create_todo_stores_priority_and_estimate`
Expected: FAIL (priority/estimate not parsed; stored as None).

- [ ] **Step 3: Update the create_todo tool schema**

In `src/assistant/tools.rs`, replace the `create_todo` `properties` block to add the two optional inputs:

```rust
                "properties": {
                    "title": { "type": "string", "description": "Short task title in the user's words" },
                    "notes": { "type": "string", "description": "Optional extra detail" },
                    "due_at": { "type": "string", "description": "Optional deadline, RFC3339 with +07:00 offset, e.g. 2026-06-12T09:00:00+07:00" },
                    "priority": { "type": "string", "enum": ["high", "normal", "low"], "description": "Optional importance; default normal" },
                    "estimate_minutes": { "type": "integer", "description": "Optional rough effort estimate in minutes, for day planning" }
                },
```

- [ ] **Step 4: Update the `create_todo` dispatcher**

In `src/assistant/dispatcher.rs`, replace `create_todo`:

```rust
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
    let priority = match str_arg(input, "priority") {
        Some(p) if matches!(p, "high" | "normal" | "low") => Some(p),
        Some(p) => return Err(format!("invalid priority '{p}' — use high/normal/low")),
        None => None,
    };
    let estimate_minutes = match input.get("estimate_minutes") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(
            v.as_i64()
                .filter(|m| *m > 0)
                .ok_or_else(|| format!("estimate_minutes must be a positive integer, got {v}"))?,
        ),
    };
    let todo = crate::repo::todos::create(
        db,
        title,
        str_arg(input, "notes"),
        due_at.as_deref(),
        priority,
        estimate_minutes,
    )
    .await
    .map_err(|e| format!("db error: {e}"))?;
    Ok(format!("created todo #{} '{}'", todo.id, todo.title))
}
```

- [ ] **Step 5: Show priority/estimate in `list_todos`**

In `src/assistant/dispatcher.rs`, update the loop body of `list_todos` to annotate priority/estimate:

```rust
    for t in todos {
        out.push_str(&format!("#{} {}", t.id, t.title));
        if let Some(due) = &t.due_at {
            out.push_str(&format!(" (due {})", to_wib_display(due)));
        }
        if let Some(p) = &t.priority {
            if p != "normal" {
                out.push_str(&format!(" [{p}]"));
            }
        }
        if let Some(est) = t.estimate_minutes {
            out.push_str(&format!(" ~{est}m"));
        }
        if let Some(notes) = &t.notes {
            out.push_str(&format!(" — {notes}"));
        }
        out.push('\n');
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test create_todo`
Expected: PASS (both new tests plus the existing create_todo tests).

- [ ] **Step 7: Commit**

```bash
git add src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(assistant): accept priority + estimate in create_todo"
```

---

## Task 3: Shared todo ordering in `proactive/plan.rs`

**Files:**
- Create: `src/assistant/proactive/plan.rs`
- Modify: `src/assistant/proactive/mod.rs` (add `pub mod plan;`)
- Modify: `src/assistant/time.rs` (add `pub fn weekday_id`)

- [ ] **Step 1: Add `weekday_id` to `time.rs`**

Append to `src/assistant/time.rs`:

```rust
/// Indonesian weekday name (used by proactive plan/briefing/review).
pub fn weekday_id(day: chrono::Weekday) -> &'static str {
    match day {
        chrono::Weekday::Mon => "Senin",
        chrono::Weekday::Tue => "Selasa",
        chrono::Weekday::Wed => "Rabu",
        chrono::Weekday::Thu => "Kamis",
        chrono::Weekday::Fri => "Jumat",
        chrono::Weekday::Sat => "Sabtu",
        chrono::Weekday::Sun => "Minggu",
    }
}
```

- [ ] **Step 2: Register the module**

In `src/assistant/proactive/mod.rs`, add to the `pub mod` list (after `pub mod compose;`):

```rust
pub mod plan;
```

- [ ] **Step 3: Write the failing test (ordering)**

Create `src/assistant/proactive/plan.rs` with only the ordering logic + test:

```rust
//! Day-plan assembler: deterministic schedule shared by the morning briefing
//! (ordering), the on-demand plan_day tool, and the evening review (ordering).

use crate::db::Db;
use crate::repo::events::EventRow;
use crate::repo::todos::TodoRow;
use chrono::{DateTime, Utc};

/// Sort rank for priority; NULL/unknown is treated as 'normal'.
fn priority_rank(priority: Option<&str>) -> u8 {
    match priority {
        Some("high") => 0,
        Some("low") => 2,
        _ => 1,
    }
}

/// Order open todos for planning: priority (high→low), then earliest due
/// (undated last), then shortest estimate (unknown last). Stable for ties.
pub fn order_todos(mut todos: Vec<TodoRow>) -> Vec<TodoRow> {
    todos.sort_by(|a, b| {
        priority_rank(a.priority.as_deref())
            .cmp(&priority_rank(b.priority.as_deref()))
            .then_with(|| match (&a.due_at, &b.due_at) {
                (Some(x), Some(y)) => x.cmp(y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| {
                a.estimate_minutes
                    .unwrap_or(i64::MAX)
                    .cmp(&b.estimate_minutes.unwrap_or(i64::MAX))
            })
    });
    todos
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: i64, priority: Option<&str>, due_at: Option<&str>, est: Option<i64>) -> TodoRow {
        TodoRow {
            id,
            title: format!("t{id}"),
            notes: None,
            due_at: due_at.map(|s| s.into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: priority.map(|s| s.into()),
            estimate_minutes: est,
        }
    }

    #[test]
    fn orders_by_priority_then_due_then_estimate() {
        let ordered = order_todos(vec![
            todo(1, Some("low"), Some("2026-06-12T00:00:00Z"), None),
            todo(2, Some("high"), None, Some(60)),
            todo(3, Some("high"), Some("2026-06-12T00:00:00Z"), Some(15)),
            todo(4, None, Some("2026-06-11T00:00:00Z"), None),
        ]);
        let ids: Vec<i64> = ordered.iter().map(|t| t.id).collect();
        // high+due(3) → high+undated(2) → normal(4) → low(1)
        assert_eq!(ids, vec![3, 2, 4, 1]);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test plan::tests::orders_by_priority`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/assistant/proactive/plan.rs src/assistant/proactive/mod.rs src/assistant/time.rs
git commit -m "feat(proactive): shared todo ordering for day plan"
```

---

## Task 4: Day-plan gather + render block in `plan.rs`

**Files:**
- Modify: `src/assistant/proactive/plan.rs`

- [ ] **Step 1: Write the failing test (render)**

Add to `plan.rs` (above the existing `#[cfg(test)]` block, add the types/functions; add tests inside the test module):

In the test module, add:

```rust
    fn event(id: i64, start_at: &str, title: &str) -> EventRow {
        EventRow {
            id,
            title: title.into(),
            location: None,
            notes: None,
            start_at: start_at.into(),
            status: "scheduled".into(),
            created_at: String::new(),
            source: "local".into(),
            google_event_id: None,
            google_etag: None,
            synced_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn render_block_lists_events_and_ordered_todos() {
        let plan = DayPlan {
            date_wib: "2026-06-12".into(),
            weekday: "Jumat".into(),
            events: vec![event(1, "2026-06-12T03:00:00Z", "meeting klien")], // 10:00 WIB
            todos: order_todos(vec![
                todo(7, Some("high"), None, Some(30)),
                todo(8, Some("low"), None, None),
            ]),
        };
        let block = render_plan_block(&plan);
        assert!(block.contains("Jumat, 2026-06-12"), "{block}");
        assert!(block.contains("10:00 WIB"), "{block}");
        assert!(block.contains("meeting klien"), "{block}");
        assert!(block.contains("#7 t7"), "{block}");
        // high-priority todo appears before low-priority one
        let hi = block.find("#7").unwrap();
        let lo = block.find("#8").unwrap();
        assert!(hi < lo, "{block}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test plan::tests::render_block_lists_events`
Expected: FAIL (`DayPlan` / `render_plan_block` not defined).

- [ ] **Step 3: Implement `DayPlan`, `gather`, `render_plan_block`**

Add to `plan.rs` (after `order_todos`):

```rust
pub struct DayPlan {
    pub date_wib: String,
    pub weekday: String,
    pub events: Vec<EventRow>,
    pub todos: Vec<TodoRow>,
}

/// Gather today's (WIB) events and open todos, todos ordered for planning.
pub async fn gather(db: &Db, now_utc: DateTime<Utc>) -> anyhow::Result<DayPlan> {
    let now_wib = now_utc.with_timezone(&crate::assistant::time::wib());
    let date_wib = now_wib.format("%Y-%m-%d").to_string();

    let day_start = now_wib
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_local_timezone(crate::assistant::time::wib())
        .single()
        .expect("WIB has no DST gaps")
        .with_timezone(&Utc);
    let events = crate::repo::events::list_between(
        db,
        &crate::assistant::time::to_db_utc(day_start),
        &crate::assistant::time::to_db_utc(day_start + chrono::Duration::days(1)),
    )
    .await?;

    let todos = order_todos(crate::repo::todos::list_open(db).await?);

    Ok(DayPlan {
        date_wib,
        weekday: crate::assistant::time::weekday_id(now_wib.weekday()).to_string(),
        events,
        todos,
    })
}

/// Deterministic plan block: LLM input and fallback body.
pub fn render_plan_block(plan: &DayPlan) -> String {
    use chrono::Datelike;
    let mut out = format!("Rencana hari: {}, {} (WIB)\n", plan.weekday, plan.date_wib);

    out.push_str("Agenda (jam pasti):\n");
    if plan.events.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for e in &plan.events {
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

    out.push_str("Todo (urut prioritas):\n");
    if plan.todos.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for t in &plan.todos {
            out.push_str(&format!("- #{} {}", t.id, t.title));
            let priority = t.priority.as_deref().unwrap_or("normal");
            out.push_str(&format!(" [{priority}]"));
            if let Some(est) = t.estimate_minutes {
                out.push_str(&format!(" ~{est}m"));
            }
            if let Some(due) = &t.due_at {
                out.push_str(&format!(" (due {})", crate::assistant::time::to_wib_display(due)));
            }
            out.push('\n');
        }
    }
    out
}
```

> Note: the `use chrono::Datelike;` inside `render_plan_block` is not needed there; the `Datelike` trait is used by `gather` via `now_wib.weekday()`. Add `use chrono::Datelike;` to the **module-level** imports instead (top of file, next to the other `use chrono::` line) and remove the inner `use`.

Final module-level chrono import line:

```rust
use chrono::{DateTime, Datelike, Utc};
```

- [ ] **Step 4: Run tests**

Run: `cargo test plan::tests`
Expected: PASS (ordering + render tests).

- [ ] **Step 5: Commit**

```bash
git add src/assistant/proactive/plan.rs
git commit -m "feat(proactive): day-plan gather + render block"
```

---

## Task 5: `plan_day` agent tool

**Files:**
- Modify: `src/assistant/tools.rs` (schema + registration test)
- Modify: `src/assistant/dispatcher.rs` (match arm + handler + test)

- [ ] **Step 1: Write the failing test**

Add to `src/assistant/dispatcher.rs` tests:

```rust
    #[tokio::test]
    async fn plan_day_returns_block_with_ordered_todos() {
        let db = mem_db().await;
        crate::repo::todos::create(&db, "kerja low", None, None, Some("low"), None).await.unwrap();
        crate::repo::todos::create(&db, "kerja high", None, None, Some("high"), None).await.unwrap();
        let out = dispatch(&db, "plan_day", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Rencana hari"), "{out}");
        let hi = out.find("kerja high").unwrap();
        let lo = out.find("kerja low").unwrap();
        assert!(hi < lo, "{out}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test plan_day_returns_block`
Expected: FAIL ("unknown tool: plan_day").

- [ ] **Step 3: Add the dispatch arm + handler**

In `src/assistant/dispatcher.rs`, add to the `match name` block (right after the `"complete_todo"` arm):

```rust
        "plan_day" => plan_day(db).await,
```

Add the handler (near `list_todos`):

```rust
async fn plan_day(db: &Db) -> Result<String, String> {
    let plan = crate::assistant::proactive::plan::gather(db, chrono::Utc::now())
        .await
        .map_err(|e| format!("db error: {e}"))?;
    Ok(crate::assistant::proactive::plan::render_plan_block(&plan))
}
```

- [ ] **Step 4: Register the tool schema**

In `src/assistant/tools.rs`, add this object to the `definitions()` array immediately after the `complete_todo` object:

```rust
        {
            "name": "plan_day",
            "description": "Assemble today's plan: agenda events at their times plus open todos ordered by priority. Use when the user asks to plan the day or what's left today (e.g. 'rencanain hariku', 'sisa hari ini apa aja').",
            "input_schema": { "type": "object", "properties": {} }
        },
```

Update the `defines_all_tools_with_schemas` expected name list — insert `"plan_day"` right after `"complete_todo"`:

```rust
                "create_todo", "list_todos", "complete_todo", "plan_day",
```

- [ ] **Step 5: Run tests**

Run: `cargo test plan_day` and `cargo test tools::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(assistant): plan_day tool for on-demand day plan"
```

---

## Task 6: `todos::rollover` repo function

**Files:**
- Modify: `src/repo/todos.rs`

- [ ] **Step 1: Write the failing test**

Add to `src/repo/todos.rs` tests:

```rust
    #[tokio::test]
    async fn rollover_shifts_overdue_and_today_by_one_day_only() {
        let db = mem_db().await;
        // "now" = 2026-06-12T05:00:00Z == 12:00 WIB.
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let overdue = create(&db, "overdue", None, Some("2026-06-10T02:00:00Z"), None, None).await.unwrap();
        let today = create(&db, "today", None, Some("2026-06-12T02:00:00Z"), None, None).await.unwrap();
        let future = create(&db, "future", None, Some("2026-06-20T02:00:00Z"), None, None).await.unwrap();
        let undated = create(&db, "undated", None, None, None, None).await.unwrap();

        let moved = rollover(&db, None, now).await.unwrap();
        let moved_ids: Vec<i64> = moved.iter().map(|t| t.id).collect();
        assert_eq!(moved_ids, vec![overdue.id, today.id]);

        // due_at advanced by exactly one day, time-of-day preserved.
        let today_after = get(&db, today.id).await.unwrap();
        assert_eq!(today_after.due_at.as_deref(), Some("2026-06-13T02:00:00Z"));
        // future + undated untouched.
        assert_eq!(get(&db, future.id).await.unwrap().due_at.as_deref(), Some("2026-06-20T02:00:00Z"));
        assert_eq!(get(&db, undated.id).await.unwrap().due_at, None);
    }

    #[tokio::test]
    async fn rollover_with_explicit_ids_skips_others_and_future() {
        let db = mem_db().await;
        let now = chrono::DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&chrono::Utc);
        let a = create(&db, "a", None, Some("2026-06-10T02:00:00Z"), None, None).await.unwrap();
        let b = create(&db, "b", None, Some("2026-06-10T02:00:00Z"), None, None).await.unwrap();
        let future = create(&db, "future", None, Some("2026-06-20T02:00:00Z"), None, None).await.unwrap();

        let moved = rollover(&db, Some(&[a.id, future.id]), now).await.unwrap();
        let moved_ids: Vec<i64> = moved.iter().map(|t| t.id).collect();
        // a moved; future skipped (future due); b not in id list.
        assert_eq!(moved_ids, vec![a.id]);
        assert_eq!(get(&db, b.id).await.unwrap().due_at.as_deref(), Some("2026-06-10T02:00:00Z"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test rollover`
Expected: FAIL (`rollover` not defined).

- [ ] **Step 3: Implement `rollover`**

Add to `src/repo/todos.rs` (after `complete`):

```rust
/// Move open todos forward one day. With `ids = None`, rolls every open todo
/// whose due date (WIB) is today or earlier. With explicit `ids`, rolls only
/// those — still skipping undated or future-dated todos. Time-of-day and the
/// stored Z-format are preserved. Returns the moved rows (in id order).
pub async fn rollover(
    db: &Db,
    ids: Option<&[i64]>,
    now_utc: chrono::DateTime<chrono::Utc>,
) -> anyhow::Result<Vec<TodoRow>> {
    let today_wib = now_utc
        .with_timezone(&crate::assistant::time::wib())
        .format("%Y-%m-%d")
        .to_string();
    let mut moved = Vec::new();
    for todo in list_open(db).await? {
        if let Some(allow) = ids {
            if !allow.contains(&todo.id) {
                continue;
            }
        }
        let Some(due_at) = &todo.due_at else { continue };
        let Ok(parsed) = chrono::DateTime::parse_from_rfc3339(due_at) else { continue };
        let due_date_wib = parsed
            .with_timezone(&crate::assistant::time::wib())
            .format("%Y-%m-%d")
            .to_string();
        if due_date_wib.as_str() > today_wib.as_str() {
            continue; // future due dates are left untouched
        }
        let new_due = parsed.with_timezone(&chrono::Utc) + chrono::Duration::days(1);
        let new_due_db = crate::assistant::time::to_db_utc(new_due);
        sqlx::query("UPDATE todos SET due_at = ? WHERE id = ? AND status = 'open'")
            .bind(&new_due_db)
            .bind(todo.id)
            .execute(db)
            .await?;
        moved.push(get(db, todo.id).await?);
    }
    Ok(moved)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test rollover`
Expected: PASS (both rollover tests).

- [ ] **Step 5: Commit**

```bash
git add src/repo/todos.rs
git commit -m "feat(todos): rollover overdue/today todos by one day"
```

---

## Task 7: `rollover_todos` agent tool

**Files:**
- Modify: `src/assistant/tools.rs` (schema + registration test)
- Modify: `src/assistant/dispatcher.rs` (match arm + handler + test)

- [ ] **Step 1: Write the failing test**

Add to `src/assistant/dispatcher.rs` tests:

```rust
    #[tokio::test]
    async fn rollover_todos_default_moves_overdue_and_reports() {
        let db = mem_db().await;
        let yesterday = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        crate::repo::todos::create(&db, "kelar besok", None, Some(&yesterday), None, None).await.unwrap();
        let out = dispatch(&db, "rollover_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("kelar besok"), "{out}");
        assert!(out.contains("digeser"), "{out}");
    }

    #[tokio::test]
    async fn rollover_todos_reports_when_nothing_to_move() {
        let db = mem_db().await;
        let out = dispatch(&db, "rollover_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("nggak ada"), "{out}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test rollover_todos`
Expected: FAIL ("unknown tool: rollover_todos").

- [ ] **Step 3: Add the dispatch arm + handler**

In `src/assistant/dispatcher.rs`, add to the `match name` block (right after the `"plan_day"` arm):

```rust
        "rollover_todos" => rollover_todos(db, input).await,
```

Add the handler (near `plan_day`):

```rust
async fn rollover_todos(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let ids: Option<Vec<i64>> = match input.get("ids") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Array(arr)) => Some(
            arr.iter()
                .map(|v| v.as_i64().ok_or_else(|| format!("ids must be integers, got {v}")))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(v) => return Err(format!("ids must be an array of integers, got {v}")),
    };
    let moved = crate::repo::todos::rollover(db, ids.as_deref(), chrono::Utc::now())
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if moved.is_empty() {
        return Ok("nggak ada todo yang perlu digeser".into());
    }
    let mut out = format!("{} todo digeser ke besok:\n", moved.len());
    for t in moved {
        out.push_str(&format!("- #{} {}\n", t.id, t.title));
    }
    Ok(out)
}
```

- [ ] **Step 4: Register the tool schema**

In `src/assistant/tools.rs`, add this object immediately after the `plan_day` object:

```rust
        {
            "name": "rollover_todos",
            "description": "Move unfinished todos that are overdue or due today to tomorrow (preserving time of day). Call this when the user agrees to the evening review's offer, or asks to push today's leftovers to tomorrow. Omit 'ids' to roll all overdue/today todos.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "ids": { "type": "array", "items": { "type": "integer" }, "description": "Optional specific todo ids to roll; omit to roll all overdue/today todos" }
                }
            }
        },
```

Update the `defines_all_tools_with_schemas` expected name list — insert `"rollover_todos"` right after `"plan_day"`:

```rust
                "create_todo", "list_todos", "complete_todo", "plan_day", "rollover_todos",
```

- [ ] **Step 5: Run tests**

Run: `cargo test rollover_todos` and `cargo test tools::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(assistant): rollover_todos tool"
```

---

## Task 8: Morning briefing reads as a plan

**Files:**
- Modify: `src/assistant/proactive/compose.rs` (`BRIEFING_SYSTEM` + prompt test)
- Modify: `src/assistant/proactive/briefing.rs` (order todos, reuse `time::weekday_id`, fix TodoRow literals)

- [ ] **Step 1: Update `BRIEFING_SYSTEM` to plan-style phrasing**

In `src/assistant/proactive/compose.rs`, replace `BRIEFING_SYSTEM`:

```rust
pub const BRIEFING_SYSTEM: &str = "You write a short daily morning plan in Indonesian \
for the app owner, delivered over Telegram. Use ONLY the data block provided — copy every \
number exactly as written, never invent or recalculate anything. Plain text only: no Markdown, \
no headers, no **bold**, no tables. At most 15 short lines; use emoji sparingly as bullets. \
Frame it as a plan for the day, not a flat list: open with a one-line greeting (day and date), \
then the agenda at its fixed times, then the todos in the order given (highest priority first) — \
suggest a sensible flow around the events. Add a one-or-two-line portfolio summary (net worth, \
change, notable movers, pending reviews when present), remembered facts only if clearly relevant \
today, and one short grounded closing line. Skip any section whose data is empty.";
```

- [ ] **Step 2: Run the prompt invariant test**

Run: `cargo test compose::tests::prompts_demand_exact_numbers_and_plain_text`
Expected: PASS (still contains "indonesian", "exactly", "no markdown").

- [ ] **Step 3: Order briefing todos by priority + reuse shared weekday**

In `src/assistant/proactive/briefing.rs`:

Delete the private `weekday_id` function (lines ~63-74) and replace its call sites. In `gather`, change:

```rust
        weekday: weekday_id(now_wib.weekday()).to_string(),
```
to:
```rust
        weekday: crate::assistant::time::weekday_id(now_wib.weekday()).to_string(),
```

And in the `memory_facts` query string, change `weekday_id(now_wib.weekday())` to `crate::assistant::time::weekday_id(now_wib.weekday())`.

In `gather`, order the classified todo lists for planning. After:

```rust
    let (todos_due_today, todos_overdue) = classify_todos(open_todos, &today);
```
change to:
```rust
    let (todos_due_today, todos_overdue) = classify_todos(open_todos, &today);
    let todos_due_today = super::plan::order_todos(todos_due_today);
    let todos_overdue = super::plan::order_todos(todos_overdue);
```

- [ ] **Step 4: Fix `TodoRow` literals in briefing tests**

`TodoRow` now has two extra fields. In `src/assistant/proactive/briefing.rs` tests, both literal constructions (the `todos_due_today` literal ~line 404 and the `todo_due` helper ~line 459) need the new fields. Update the `todo_due` helper:

```rust
    fn todo_due(id: i64, due_at: &str) -> TodoRow {
        TodoRow {
            id,
            title: format!("t{id}"),
            notes: None,
            due_at: Some(due_at.into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: None,
            estimate_minutes: None,
        }
    }
```

And the inline literal in `todos_and_facts_render_with_details` — add `priority: None,` and `estimate_minutes: None,` after `completed_at: None,`.

- [ ] **Step 5: Run tests**

Run: `cargo test briefing::tests` and `cargo test compose::tests`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/proactive/compose.rs src/assistant/proactive/briefing.rs
git commit -m "feat(proactive): morning briefing reads as a prioritised plan"
```

---

## Task 9: Evening review job

**Files:**
- Create: `src/assistant/proactive/evening_review.rs`
- Modify: `src/assistant/proactive/compose.rs` (`REVIEW_SYSTEM` + extend prompt test)
- Modify: `src/assistant/proactive/mod.rs` (`pub mod evening_review;`)

- [ ] **Step 1: Add `REVIEW_SYSTEM` and cover it in the prompt invariant test**

In `src/assistant/proactive/compose.rs`, add after `RECAP_SYSTEM`:

```rust
pub const REVIEW_SYSTEM: &str = "You write a short daily evening review in Indonesian for the \
app owner, delivered over Telegram. Use ONLY the data block provided — copy every item exactly, \
never invent anything. Plain text only: no Markdown, no headers, no **bold**, no tables. At most \
12 short lines; use emoji sparingly. Structure: one warm opening line; what got done today; what \
is still unfinished (overdue or due today). End with exactly one question offering to move the \
unfinished todos to tomorrow, e.g. 'Mau aku geser yang belum kelar ke besok? Balas iya ya.' If \
nothing is unfinished, congratulate briefly and skip the question.";
```

Update `prompts_demand_exact_numbers_and_plain_text` to include the new prompt:

```rust
        for prompt in [BRIEFING_SYSTEM, RECAP_SYSTEM, REVIEW_SYSTEM] {
```

> Note: `REVIEW_SYSTEM` deliberately contains "exactly" and "no markdown" and "Indonesian" so the existing invariant assertions pass.

- [ ] **Step 2: Register the module**

In `src/assistant/proactive/mod.rs`, add (after `pub mod compose;`):

```rust
pub mod evening_review;
```

- [ ] **Step 3: Write the failing test (gather + render)**

Create `src/assistant/proactive/evening_review.rs`:

```rust
//! Daily evening review: what got done, what's left, and an offer to roll the
//! leftovers to tomorrow. Deterministic gather → compose-and-send.

use crate::db::Db;
use crate::repo::todos::TodoRow;
use chrono::{DateTime, Datelike, Utc};

pub struct ReviewData {
    pub date_wib: String,
    pub weekday: String,
    pub done_today: Vec<TodoRow>,
    pub unfinished: Vec<TodoRow>,
}

/// Open todos whose due date (WIB) is today or earlier — the rollover candidates.
fn unfinished_through_today(open: Vec<TodoRow>, today_wib: &str) -> Vec<TodoRow> {
    open.into_iter()
        .filter(|t| {
            t.due_at
                .as_deref()
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map(|dt| {
                    dt.with_timezone(&crate::assistant::time::wib())
                        .format("%Y-%m-%d")
                        .to_string()
                        .as_str()
                        <= today_wib
                })
                .unwrap_or(false)
        })
        .collect()
}

pub async fn gather(db: &Db, now_utc: DateTime<Utc>) -> anyhow::Result<ReviewData> {
    let now_wib = now_utc.with_timezone(&crate::assistant::time::wib());
    let today_wib = now_wib.format("%Y-%m-%d").to_string();

    // Start of today in WIB, expressed as a +00:00 RFC3339 string to match the
    // format `todos::complete` writes into completed_at.
    let day_start_utc = now_wib
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_local_timezone(crate::assistant::time::wib())
        .single()
        .expect("WIB has no DST gaps")
        .with_timezone(&Utc)
        .to_rfc3339();

    let done_today = crate::repo::todos::completed_since(db, &day_start_utc).await?;
    let unfinished =
        super::plan::order_todos(unfinished_through_today(crate::repo::todos::list_open(db).await?, &today_wib));

    Ok(ReviewData {
        date_wib: today_wib,
        weekday: crate::assistant::time::weekday_id(now_wib.weekday()).to_string(),
        done_today,
        unfinished,
    })
}

pub fn render_data_block(d: &ReviewData) -> String {
    let mut out = format!("Review sore: {}, {} (WIB)\n", d.weekday, d.date_wib);

    out.push_str("Selesai hari ini:\n");
    if d.done_today.is_empty() {
        out.push_str("(belum ada)\n");
    } else {
        for t in &d.done_today {
            out.push_str(&format!("- #{} {}\n", t.id, t.title));
        }
    }

    out.push_str("Belum kelar:\n");
    if d.unfinished.is_empty() {
        out.push_str("(semua kelar)\n");
    } else {
        for t in &d.unfinished {
            out.push_str(&format!("- #{} {}", t.id, t.title));
            if let Some(due) = &t.due_at {
                out.push_str(&format!(" (due {})", crate::assistant::time::to_wib_display(due)));
            }
            out.push('\n');
        }
    }
    out
}

/// Gather → compose → send. The caller has already claimed the dedup key.
pub async fn run(
    db: &Db,
    client: &crate::telegram::client::TelegramClient,
    chat_id: i64,
) -> anyhow::Result<()> {
    let data = gather(db, chrono::Utc::now()).await?;
    let block = render_data_block(&data);
    let text =
        super::compose::compose(super::compose::REVIEW_SYSTEM, &block, "🌙 Review sore (mode ringkas)").await;
    client
        .send_message(chat_id, &text)
        .await
        .map_err(|e| anyhow::anyhow!("evening review send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: i64, due_at: Option<&str>) -> TodoRow {
        TodoRow {
            id,
            title: format!("t{id}"),
            notes: None,
            due_at: due_at.map(|s| s.into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: None,
            estimate_minutes: None,
        }
    }

    #[test]
    fn unfinished_keeps_overdue_and_today_drops_future_and_undated() {
        let kept = unfinished_through_today(
            vec![
                todo(1, Some("2026-06-10T02:00:00Z")), // overdue
                todo(2, Some("2026-06-12T02:00:00Z")), // today
                todo(3, Some("2026-06-20T02:00:00Z")), // future
                todo(4, None),                         // undated
            ],
            "2026-06-12",
        );
        let ids: Vec<i64> = kept.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn render_block_shows_done_and_unfinished_sections() {
        let d = ReviewData {
            date_wib: "2026-06-12".into(),
            weekday: "Jumat".into(),
            done_today: vec![todo(5, None)],
            unfinished: vec![todo(6, Some("2026-06-12T02:00:00Z"))],
        };
        let block = render_data_block(&d);
        assert!(block.contains("Selesai hari ini:"), "{block}");
        assert!(block.contains("#5 t5"), "{block}");
        assert!(block.contains("Belum kelar:"), "{block}");
        assert!(block.contains("#6 t6"), "{block}");
    }

    #[tokio::test]
    async fn gather_works_on_an_empty_db() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let d = gather(&db, chrono::Utc::now()).await.unwrap();
        assert!(d.done_today.is_empty());
        assert!(d.unfinished.is_empty());
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test evening_review::tests` and `cargo test compose::tests`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/assistant/proactive/evening_review.rs src/assistant/proactive/compose.rs src/assistant/proactive/mod.rs
git commit -m "feat(proactive): daily evening review with rollover offer"
```

---

## Task 10: Schedule the evening review in the tick loop

**Files:**
- Modify: `src/assistant/proactive/tick.rs` (`ProactiveConfig`, `from_env`, `evening_review_due`, `run_once`, tests)

- [ ] **Step 1: Write the failing test (due window + default)**

Add to `src/assistant/proactive/tick.rs` tests:

```rust
    #[test]
    fn evening_review_due_inside_the_window_only() {
        // default hour 21, grace 5h → due 21:00..02:00-clamped (window does not wrap).
        assert_eq!(evening_review_due(wib(2026, 6, 12, 20, 59), Some(21)), None);
        assert_eq!(
            evening_review_due(wib(2026, 6, 12, 21, 0), Some(21)),
            Some("evening_review:2026-06-12".to_string())
        );
        assert_eq!(
            evening_review_due(wib(2026, 6, 12, 23, 59), Some(21)),
            Some("evening_review:2026-06-12".to_string())
        );
        // Disabled.
        assert_eq!(evening_review_due(wib(2026, 6, 12, 21, 30), None), None);
    }
```

Extend `config_defaults_are_sane` with:

```rust
        assert_eq!(config.evening_review_hour, Some(21));
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test evening_review_due_inside_the_window_only`
Expected: FAIL (`evening_review_due` and `evening_review_hour` not defined).

- [ ] **Step 3: Add the config field**

In `src/assistant/proactive/tick.rs`, add to `ProactiveConfig`:

```rust
    pub evening_review_hour: Option<u32>,
```

In `from_env`, add (after `recap_hour`):

```rust
            evening_review_hour: parse_hour(std::env::var("EVENING_REVIEW_HOUR_WIB").ok(), 21),
```

In the existing `run_once_claims_and_survives_an_empty_db_without_a_client` test's `ProactiveConfig { ... }` literal, add:

```rust
            evening_review_hour: Some(0),
```

- [ ] **Step 4: Add `evening_review_due`**

In `src/assistant/proactive/tick.rs`, add (next to `briefing_due`):

```rust
/// Dedup key when the evening review is due, else None. Same fixed-hour grace
/// window as the briefing; the day is forfeited past the window.
pub fn evening_review_due(
    now_wib: DateTime<FixedOffset>,
    review_hour: Option<u32>,
) -> Option<String> {
    let hour = review_hour?;
    let h = now_wib.hour();
    if h >= hour && h < hour + GRACE_HOURS {
        Some(format!("evening_review:{}", now_wib.format("%Y-%m-%d")))
    } else {
        None
    }
}
```

- [ ] **Step 5: Claim-then-send in `run_once`**

In `run_once`, add after the recap block (before the alerts loop):

```rust
    if let Some(key) = evening_review_due(now_wib, config.evening_review_hour) {
        if crate::repo::proactive_log::try_claim(db, "evening_review", &key).await? {
            if let Err(e) = super::evening_review::run(db, client, link.chat_id).await {
                tracing::warn!("evening review for {key} forfeited: {e:#}");
            }
        }
    }
```

- [ ] **Step 6: Run tests**

Run: `cargo test tick::tests`
Expected: PASS (new window test + extended defaults + existing tests).

- [ ] **Step 7: Commit**

```bash
git add src/assistant/proactive/tick.rs
git commit -m "feat(proactive): schedule daily evening review (EVENING_REVIEW_HOUR_WIB)"
```

---

## Task 11: Teach the agent about planning + rollover

**Files:**
- Modify: `src/assistant/agent.rs` (`SYSTEM` prompt)

- [ ] **Step 1: Add guidance to the SYSTEM prompt**

In `src/assistant/agent.rs`, the `SYSTEM` constant currently has a sentence beginning "You manage todos and reminders...". Append a planning paragraph to the end of the `SYSTEM` string (before the closing `";`), continuing the existing `\`-joined style:

```rust
 You can assemble a day plan: when the user asks to plan today or what's left \
(e.g. 'rencanain hariku', 'sisa hari ini apa aja', 'hari ini ngapain aja'), call plan_day \
and present its agenda + prioritised todos as a short suggested flow. When the user agrees to \
move unfinished todos to tomorrow (for example replying to the evening review's offer, or saying \
'geser yang belum kelar ke besok'), call rollover_todos — omit ids to roll everything overdue or \
due today, or pass specific ids when they name particular todos — then confirm what moved.
```

- [ ] **Step 2: Verify the crate compiles**

Run: `cargo check`
Expected: Finished with no errors.

- [ ] **Step 3: Commit**

```bash
git add src/assistant/agent.rs
git commit -m "feat(assistant): prompt guidance for plan_day + rollover_todos"
```

---

## Final verification

- [ ] **Run the full test suite**

Run: `cargo test`
Expected: all tests pass.

- [ ] **Type-check once more**

Run: `cargo check`
Expected: clean.

---

## Spec coverage check

- Day-plan assembler shared by morning/on-demand/review → Task 3, 4 (`plan.rs`), reused in Task 8 (briefing) and Task 9 (review).
- Morning briefing as a plan → Task 8.
- On-demand `plan_day` → Task 5.
- Evening review (done vs unfinished + rollover offer) → Task 9.
- Confirm-gated rollover (`rollover_todos` only on user agreement) → Task 6 (repo), Task 7 (tool), Task 11 (prompt: call only on agreement).
- Todo `priority` + `estimate_minutes` (migration 0016) → Task 1, 2.
- Scheduling + `EVENING_REVIEW_HOUR_WIB` config → Task 10.
- ClickUp excluded from the plan → honoured (plan.rs uses only events + todos).
- Error handling: `compose` fallback reused (Task 9 run), rollover "nggak ada" path (Task 7).
- Tests for ordering, due window, rollover, dispatch → Tasks 3, 6, 7, 9, 10.
