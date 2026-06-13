# ClickUp Project Assistant — Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the assistant read freelance tasks ("apa di PT AIS?", "task hari ini?", "yang overdue?") and mark them complete via chat.

**Architecture:** Extend the `ClickUpApi` trait with `list_tasks(list_id)` and `complete_task(task_id)` plus a `Task` DTO, and add `list_tasks`/`complete_task` assistant tools. "today"/"overdue"/"open" scopes are computed in the handler (Rust) over the per-list results, so the trait stays minimal and testable with the existing fake. A small WIB date-bounds helper goes in `assistant::time`.

**Tech Stack:** Rust, reqwest 0.12, async-trait, serde_json, chrono. Binary crate `portfolio-tracker` — `cargo test --bin portfolio-tracker <filter>` from `backend/`.

**Scope note:** Phase 2 of `docs/superpowers/specs/2026-06-13-clickup-project-assistant-design.md`. Builds on Phase 1 (client + list/create_project/create_task). Phases 3 (billable) and 4 (briefing) follow.

---

## File Structure

- `backend/src/clickup/client.rs` — add `Task` DTO; add `list_tasks` + `complete_task` to the `ClickUpApi` trait and `ClickUpClient` impl; `from_env` reads optional `CLICKUP_DONE_STATUS` (default `"complete"`).
- `backend/src/assistant/time.rs` — add `end_of_today_wib_ms(now)` + test.
- `backend/src/assistant/tools.rs` — `list_tasks` + `complete_task` schemas; schema tests.
- `backend/src/assistant/dispatcher.rs` — handlers `clickup_list_tasks`, `clickup_complete_task`; dispatch arms; extend `FakeClickUp`; tests.
- `backend/src/assistant/agent.rs` — `SYSTEM` prompt: reading/completing tasks; prompt test.

All commands run from `backend/`.

---

### Task 1: Extend client — `Task` DTO, `list_tasks`, `complete_task`

**Files:**
- Modify: `backend/src/clickup/client.rs`

- [ ] **Step 1: Add the `Task` DTO**

After the `NewTask` struct in `client.rs`:

```rust
/// A ClickUp task as read back from the API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub status: String,
    pub due_date_ms: Option<i64>,
}
```

- [ ] **Step 2: Add two trait methods**

Add to the `ClickUpApi` trait (after `create_task`):

```rust
    /// Open tasks in a List.
    async fn list_tasks(&self, list_id: &str) -> Result<Vec<Task>, ClickUpError>;
    /// Mark a task complete (sets its status to the configured done status).
    async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError>;
```

- [ ] **Step 3: Store the done-status in the client + from_env**

Add a `done_status: String` field to `ClickUpClient` and set it in `from_env`:

```rust
        let done_status = std::env::var("CLICKUP_DONE_STATUS").unwrap_or_else(|_| "complete".into());
        Ok(Self { http: reqwest::Client::new(), token, space_id, done_status })
```
(Update the struct definition to add `done_status: String`.)

- [ ] **Step 4: Implement the two methods on `ClickUpClient`**

Add inside `impl ClickUpApi for ClickUpClient`:

```rust
    async fn list_tasks(&self, list_id: &str) -> Result<Vec<Task>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/list/{list_id}/task?archived=false");
        let resp = self.http.get(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        let tasks = parsed["tasks"].as_array().map(|arr| {
            arr.iter().filter_map(|t| {
                Some(Task {
                    id: t["id"].as_str()?.to_string(),
                    name: t["name"].as_str()?.to_string(),
                    status: t["status"]["status"].as_str().unwrap_or("").to_string(),
                    // ClickUp returns due_date as a string of epoch ms (or null).
                    due_date_ms: t["due_date"].as_str().and_then(|s| s.parse::<i64>().ok()),
                })
            }).collect()
        }).unwrap_or_default();
        Ok(tasks)
    }

    async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/task/{task_id}");
        let resp = self.http.put(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "status": self.done_status }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        Ok(())
    }
```

- [ ] **Step 5: Add `Task` to the re-export**

In `backend/src/clickup/mod.rs`, the production re-export is `pub use client::{ClickUpApi, ClickUpClient, NewTask};`. `Task` will be used in the dispatcher handler (production), so add it:
`pub use client::{ClickUpApi, ClickUpClient, NewTask, Task};`

- [ ] **Step 6: Build**

Run: `cargo build --bin portfolio-tracker 2>&1 | tail -10`
Expected: builds. The `FakeClickUp` in dispatcher tests does NOT yet implement the two new trait methods, so the TEST build will fail to compile — that is fixed in Task 2. Verify the non-test build first with `cargo build`, then proceed (do not run tests until Task 2).

- [ ] **Step 7: Commit**

```bash
git add src/clickup/
git commit -m "feat(clickup): add list_tasks and complete_task to the client"
```

---

### Task 2: WIB bounds helper + `list_tasks` tool

**Files:**
- Modify: `backend/src/assistant/time.rs`
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Failing test for the bounds helper**

In `backend/src/assistant/time.rs` test module:

```rust
#[test]
fn end_of_today_wib_is_2359_local() {
    // 2026-06-12 20:00 UTC == 2026-06-13 03:00 WIB; end of that WIB day is
    // 2026-06-13 23:59:59 WIB == 2026-06-13 16:59:59 UTC.
    let now = Utc.with_ymd_and_hms(2026, 6, 12, 20, 0, 0).unwrap();
    let end_ms = end_of_today_wib_ms(now);
    let expected = Utc.with_ymd_and_hms(2026, 6, 13, 16, 59, 59).unwrap().timestamp_millis();
    assert_eq!(end_ms, expected);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::time::tests::end_of_today_wib_is_2359_local 2>&1 | tail -12`
Expected: FAIL (`end_of_today_wib_ms` not found).

- [ ] **Step 3: Implement the helper**

In `backend/src/assistant/time.rs` (the `TimeZone` trait is already imported via `use chrono::{... TimeZone ...}`):

```rust
/// Epoch-ms of 23:59:59 WIB on the WIB-local date of `now`. Used to bound a
/// "due today" window for tasks whose due dates are epoch ms.
pub fn end_of_today_wib_ms(now: DateTime<Utc>) -> i64 {
    let today = now.with_timezone(&wib()).date_naive();
    let end = today.and_hms_opt(23, 59, 59).expect("23:59:59 is valid");
    wib().from_local_datetime(&end).single().expect("WIB has no DST gaps").timestamp_millis()
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::time::tests::end_of_today_wib_is_2359_local 2>&1 | tail -12`
Expected: PASS.

- [ ] **Step 5: Add the `list_tasks` schema**

In `backend/src/assistant/tools.rs`, after the `create_task` object:

```rust
{
    "name": "list_tasks",
    "description": "List freelance tasks from ClickUp. Pass a project name to list that project's open tasks; or pass scope 'today' / 'overdue' to list due/overdue tasks across all projects. Default scope 'open' lists all open tasks.",
    "input_schema": {
        "type": "object",
        "properties": {
            "project": { "type": "string", "description": "Optional project (list) name to filter to" },
            "scope": { "type": "string", "enum": ["open", "today", "overdue"], "description": "open (default), today (due today), or overdue" }
        }
    }
}
```
Append `"list_tasks"` to the `defines_all_tools_with_schemas` names vec (after `"create_task"`).

- [ ] **Step 6: Extend `FakeClickUp` + write failing tests**

In `backend/src/assistant/dispatcher.rs` test module, extend the fake to hold tasks per list and record completions. Add a field and the two new trait methods to the existing `impl ClickUpApi for FakeClickUp`:

```rust
// Add to the FakeClickUp struct fields:
        tasks: Mutex<std::collections::HashMap<String, Vec<crate::clickup::client::Task>>>,
        completed: Mutex<Vec<String>>,
```
```rust
// Add to impl ClickUpApi for FakeClickUp:
        async fn list_tasks(&self, list_id: &str) -> Result<Vec<crate::clickup::client::Task>, ClickUpError> {
            Ok(self.tasks.lock().unwrap().get(list_id).cloned().unwrap_or_default())
        }
        async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError> {
            self.completed.lock().unwrap().push(task_id.to_string());
            Ok(())
        }
```
Add this import near the other test imports if not present: `use crate::clickup::client::Task;` (then use `Task` unqualified in tests).

Tests:

```rust
#[tokio::test]
async fn list_tasks_for_a_project_shows_open_tasks() {
    let fake = FakeClickUp::default();
    fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
    fake.tasks.lock().unwrap().insert("l1".into(), vec![
        Task { id: "t1".into(), name: "bikin kontrak".into(), status: "to do".into(), due_date_ms: None },
    ]);
    let out = clickup_list_tasks(&fake, &serde_json::json!({ "project": "PT AIS" })).await.unwrap();
    assert!(out.contains("bikin kontrak"), "{out}");
    assert!(out.contains("t1"), "task id shown for complete_task: {out}");
}

#[tokio::test]
async fn list_tasks_overdue_filters_across_projects() {
    let fake = FakeClickUp::default();
    fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
    // due far in the past → overdue; second task no due → excluded from overdue.
    fake.tasks.lock().unwrap().insert("l1".into(), vec![
        Task { id: "t1".into(), name: "lewat deadline".into(), status: "to do".into(), due_date_ms: Some(1_000) },
        Task { id: "t2".into(), name: "tanpa due".into(), status: "to do".into(), due_date_ms: None },
    ]);
    let out = clickup_list_tasks(&fake, &serde_json::json!({ "scope": "overdue" })).await.unwrap();
    assert!(out.contains("lewat deadline"), "{out}");
    assert!(!out.contains("tanpa due"), "no-due task must not be overdue: {out}");
}

#[tokio::test]
async fn list_tasks_empty_is_explicit() {
    let fake = FakeClickUp::default();
    let out = clickup_list_tasks(&fake, &serde_json::json!({ "scope": "today" })).await.unwrap();
    assert!(out.contains("tidak ada task"), "{out}");
}
```

- [ ] **Step 7: Run to verify they fail**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_tasks 2>&1 | tail -15`
Expected: FAIL (`clickup_list_tasks` not found).

- [ ] **Step 8: Implement the handler + dispatch arm**

In `backend/src/assistant/dispatcher.rs`:

```rust
async fn clickup_list_tasks(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let scope = str_arg(input, "scope").unwrap_or("open");
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    // Which projects to scan: one named, or all.
    let targets: Vec<&crate::clickup::Project> = match str_arg(input, "project") {
        Some(name) => {
            let p = projects.iter().find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("project '{name}' belum ada"))?;
            vec![p]
        }
        None => projects.iter().collect(),
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let end_today = crate::assistant::time::end_of_today_wib_ms(chrono::Utc::now());
    let mut out = String::new();
    for project in targets {
        let tasks = api.list_tasks(&project.id).await.map_err(|e| format!("{e}"))?;
        let mut lines = String::new();
        for t in &tasks {
            let keep = match scope {
                "overdue" => t.due_date_ms.is_some_and(|d| d < now_ms),
                "today" => t.due_date_ms.is_some_and(|d| d >= now_ms && d <= end_today),
                _ => true, // "open"
            };
            if !keep { continue; }
            lines.push_str(&format!("  [{}] {}\n", t.id, t.name));
        }
        if !lines.is_empty() {
            out.push_str(&format!("{}:\n{lines}", project.name));
        }
    }
    if out.is_empty() {
        return Ok("tidak ada task".into());
    }
    Ok(out)
}
```

Dispatch arm after `"create_task" => ...`:

```rust
"list_tasks" => match crate::clickup::ClickUpClient::from_env() {
    Ok(api) => clickup_list_tasks(&api, input).await,
    Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
},
```

- [ ] **Step 9: Run to verify they pass**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_tasks 2>&1 | tail -15`
Expected: PASS (all three).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 10: Commit**

```bash
git add src/assistant/time.rs src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(clickup): add list_tasks tool with today/overdue scopes"
```

---

### Task 3: `complete_task` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, after the `list_tasks` object:

```rust
{
    "name": "complete_task",
    "description": "Mark a freelance task complete in ClickUp. Get the task_id from list_tasks (shown in brackets).",
    "input_schema": {
        "type": "object",
        "properties": { "task_id": { "type": "string", "description": "ClickUp task id from list_tasks" } },
        "required": ["task_id"]
    }
}
```
Append `"complete_task"` to the names vec (after `"list_tasks"`). Add to `required_fields_are_marked`:
`assert_eq!(find("complete_task")["input_schema"]["required"], serde_json::json!(["task_id"]));`

- [ ] **Step 2: Write the failing test**

```rust
#[tokio::test]
async fn complete_task_marks_done() {
    let fake = FakeClickUp::default();
    let out = clickup_complete_task(&fake, &serde_json::json!({ "task_id": "t1" })).await.unwrap();
    assert!(out.contains("selesai"), "{out}");
    assert_eq!(fake.completed.lock().unwrap().as_slice(), &["t1".to_string()]);
}

#[tokio::test]
async fn complete_task_requires_id() {
    let fake = FakeClickUp::default();
    let err = clickup_complete_task(&fake, &serde_json::json!({})).await.unwrap_err();
    assert!(err.contains("task_id"), "{err}");
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::complete_task 2>&1 | tail -12`
Expected: FAIL (`clickup_complete_task` not found).

- [ ] **Step 4: Implement handler + dispatch arm**

```rust
async fn clickup_complete_task(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let task_id = str_arg(input, "task_id").ok_or("missing required argument 'task_id'")?;
    api.complete_task(task_id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("task {task_id} ditandai selesai"))
}
```

Dispatch arm after `"list_tasks" => ...`:

```rust
"complete_task" => match crate::clickup::ClickUpClient::from_env() {
    Ok(api) => clickup_complete_task(&api, input).await,
    Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
},
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::complete_task 2>&1 | tail -12`
Expected: PASS (both).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(clickup): add complete_task assistant tool"
```

---

### Task 4: System prompt for reading/completing tasks

**Files:**
- Modify: `backend/src/assistant/agent.rs`

- [ ] **Step 1: Write the failing test**

In `backend/src/assistant/agent.rs` test module:

```rust
#[test]
fn system_prompt_mentions_task_reading_tools() {
    let prompt = system_prompt("2026-06-13T10:00:00+07:00");
    assert!(prompt.contains("list_tasks"), "{prompt}");
    assert!(prompt.contains("complete_task"), "{prompt}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_task_reading_tools 2>&1 | tail -12`
Expected: FAIL.

- [ ] **Step 3: Extend the `SYSTEM` const**

Append to the END of the `SYSTEM` literal (keep existing text; backslash-newline style), after the Phase-1 ClickUp paragraph:

```
 To answer 'ada task apa di <project>?' or 'task hari ini / yang overdue?', call list_tasks (pass a project name, or scope 'today'/'overdue'). It shows each task id in brackets; pass that id to complete_task when the user says a task is done.
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_task_reading_tools 2>&1 | tail -12`
Expected: PASS.

- [ ] **Step 5: Full suite + build**

Run: `cargo test --bin portfolio-tracker 2>&1 | tail -8` → report counts, 0 failed.
Run: `cargo build --bin portfolio-tracker 2>&1 | grep -c warning` → expect 0.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/agent.rs
git commit -m "feat(clickup): teach assistant to read and complete tasks"
```

---

## Self-Review Notes

- **Spec coverage (Phase 2):** read tasks (per-project + today/overdue) → Task 2; complete tasks → Task 3; due dates already created in Phase 1, surfaced here via scopes; prompt → Task 4.
- **Type consistency:** `Task { id, name, status, due_date_ms }`, trait `list_tasks(list_id)->Vec<Task>` / `complete_task(task_id)->()`, handlers `clickup_list_tasks`/`clickup_complete_task`, tool names `list_tasks`/`complete_task` appended after `create_task`. `end_of_today_wib_ms(now)` used by the handler and unit-tested independently.
- **Known limitation:** `complete_task` sets status to `CLICKUP_DONE_STATUS` (default `"complete"`); a space whose done status has a different name needs the env override. Documented in the env notes.
- **No-token degradation** preserved in every new dispatch arm.
