# ClickUp Project Assistant — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the Telegram assistant create freelance projects and add tasks to them in ClickUp via natural conversation, asking which project when unclear and offering to create a missing project.

**Architecture:** A new `clickup` module wraps the ClickUp REST API behind an `#[async_trait] ClickUpApi` trait (a test seam, mirroring the `ToolModel` LLM seam). Three new assistant tools (`list_projects`, `create_project`, `create_task`) are added to `assistant::tools`/`assistant::dispatcher` following the existing tool pattern; their handlers take `&dyn ClickUpApi` so tests drive them with a fake client, while the real `dispatch` arms construct `ClickUpClient::from_env()`. The `SYSTEM` prompt gains disambiguation guidance.

**Tech Stack:** Rust, reqwest 0.12 (json, rustls-tls — already a dependency), async-trait (already used), serde_json, tokio. Binary crate `portfolio-tracker` (NO lib target) — run tests with `cargo test --bin portfolio-tracker <filter>` from `backend/`.

**Scope note:** This plan is Phase 1 of the ClickUp assistant spec
(`docs/superpowers/specs/2026-06-13-clickup-project-assistant-design.md`).
Phases 2 (list/complete tasks, due dates), 3 (billable/amount), and 4 (briefing
section) get their own plans. Phase 1 alone is shippable: add projects/tasks by
chat.

**Deliberate deviation from spec:** The spec says ClickUp tools are "not
registered" without a token. For a deterministic tool-schema test, Phase 1
ALWAYS registers the three schemas; the dispatch arms construct
`ClickUpClient::from_env()` which returns a clear error when `CLICKUP_API_TOKEN`
is unset. Net behavior still matches intent: the bot runs fine without ClickUp;
the tools just return "clickup belum dikonfigurasi" if invoked.

---

## File Structure

- `backend/src/clickup/mod.rs` — module root; `pub mod client;` + re-exports.
- `backend/src/clickup/client.rs` — `ClickUpApi` trait, DTOs (`Project`,
  `NewTask`), `ClickUpClient` (reqwest impl), `ClickUpClient::from_env`,
  `ClickUpError`.
- `backend/src/main.rs` — add `mod clickup;` declaration.
- `backend/src/assistant/tools.rs` — three tool schemas + schema-test updates.
- `backend/src/assistant/dispatcher.rs` — three handlers (take `&dyn
  ClickUpApi`), three dispatch arms (construct client from env), a test fake +
  tests.
- `backend/src/assistant/agent.rs` — `SYSTEM` prompt disambiguation section +
  prompt test.

All commands run from `backend/`.

---

### Task 1: ClickUp client module — trait, DTOs, reqwest impl, from_env

**Files:**
- Create: `backend/src/clickup/mod.rs`
- Create: `backend/src/clickup/client.rs`
- Modify: `backend/src/main.rs` (add `mod clickup;`)

The HTTP impl can't be unit-tested without a live server, so TDD here targets
`from_env` (pure env logic). The `ClickUpApi` trait + a fake are exercised by
the dispatcher tests in later tasks; the reqwest impl is verified manually
against the real API.

- [ ] **Step 1: Declare the module**

In `backend/src/main.rs`, add `mod clickup;` alongside the other top-level
`mod` declarations (e.g. near `mod google;`). Run `grep -n "^mod " src/main.rs`
first to place it consistently.

- [ ] **Step 2: Write the module root**

Create `backend/src/clickup/mod.rs`:

```rust
//! ClickUp REST integration: a thin client behind a trait seam so the
//! assistant's project/task tools can be tested with a fake.
pub mod client;

pub use client::{ClickUpApi, ClickUpClient, ClickUpError, NewTask, Project};
```

- [ ] **Step 3: Write the failing test for `from_env`**

Create `backend/src/clickup/client.rs` with the test first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_errors_without_token() {
        // Save/clear the relevant vars for a deterministic check.
        let prev = std::env::var("CLICKUP_API_TOKEN").ok();
        std::env::remove_var("CLICKUP_API_TOKEN");
        let result = ClickUpClient::from_env();
        if let Some(v) = prev { std::env::set_var("CLICKUP_API_TOKEN", v); }
        assert!(result.is_err(), "missing token must be an error");
    }
}
```

- [ ] **Step 4: Run it to verify it fails**

Run: `cargo test --bin portfolio-tracker clickup::client::tests::from_env_errors_without_token 2>&1 | tail -15`
Expected: FAIL to compile (`ClickUpClient` not defined).

- [ ] **Step 5: Implement the trait, DTOs, client, and from_env**

Prepend to `backend/src/clickup/client.rs` (above the test module):

```rust
use async_trait::async_trait;

/// A ClickUp List, surfaced to the assistant as a "project".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// Fields for creating a task. Phase 1 uses title + optional due (epoch ms).
#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub name: String,
    pub due_date_ms: Option<i64>,
}

#[derive(Debug)]
pub enum ClickUpError {
    NoToken,
    Http(String),
    Api { status: u16, body: String },
}

impl std::fmt::Display for ClickUpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClickUpError::NoToken => write!(f, "CLICKUP_API_TOKEN tidak diset"),
            ClickUpError::Http(e) => write!(f, "gangguan jaringan ClickUp: {e}"),
            ClickUpError::Api { status, body } => write!(f, "ClickUp error {status}: {body}"),
        }
    }
}
impl std::error::Error for ClickUpError {}

/// The seam the assistant tools depend on. A fake implements this in tests;
/// `ClickUpClient` implements it against the real API.
#[async_trait]
pub trait ClickUpApi: Send + Sync {
    /// Lists in the configured Space (= projects).
    async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError>;
    /// Create a List in the configured Space; returns the new project.
    async fn create_project(&self, name: &str) -> Result<Project, ClickUpError>;
    /// Create a task in the given List; returns the new task id.
    async fn create_task(&self, list_id: &str, task: &NewTask) -> Result<String, ClickUpError>;
}

/// Real reqwest-backed client. Reads token + space id from env.
pub struct ClickUpClient {
    http: reqwest::Client,
    token: String,
    space_id: String,
}

impl ClickUpClient {
    /// Build from env: `CLICKUP_API_TOKEN` (required), `CLICKUP_SPACE_ID`
    /// (required). `CLICKUP_WORKSPACE_ID` is accepted for documentation but the
    /// v2 endpoints used here are space-scoped, so it is not required.
    pub fn from_env() -> Result<Self, ClickUpError> {
        let token = std::env::var("CLICKUP_API_TOKEN").map_err(|_| ClickUpError::NoToken)?;
        if token.trim().is_empty() {
            return Err(ClickUpError::NoToken);
        }
        let space_id = std::env::var("CLICKUP_SPACE_ID")
            .map_err(|_| ClickUpError::Api { status: 0, body: "CLICKUP_SPACE_ID tidak diset".into() })?;
        Ok(Self { http: reqwest::Client::new(), token, space_id })
    }

    fn classify(status: reqwest::StatusCode, body: String) -> ClickUpError {
        ClickUpError::Api { status: status.as_u16(), body }
    }
}

#[async_trait]
impl ClickUpApi for ClickUpClient {
    async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/space/{}/list?archived=false", self.space_id);
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
        let projects = parsed["lists"].as_array().map(|arr| {
            arr.iter().filter_map(|l| {
                Some(Project {
                    id: l["id"].as_str()?.to_string(),
                    name: l["name"].as_str()?.to_string(),
                })
            }).collect()
        }).unwrap_or_default();
        Ok(projects)
    }

    async fn create_project(&self, name: &str) -> Result<Project, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/space/{}/list", self.space_id);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "name": name }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        Ok(Project {
            id: parsed["id"].as_str().unwrap_or_default().to_string(),
            name: parsed["name"].as_str().unwrap_or(name).to_string(),
        })
    }

    async fn create_task(&self, list_id: &str, task: &NewTask) -> Result<String, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/list/{list_id}/task");
        let mut payload = serde_json::json!({ "name": task.name });
        if let Some(ms) = task.due_date_ms {
            payload["due_date"] = serde_json::json!(ms);
        }
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&payload)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        Ok(parsed["id"].as_str().unwrap_or_default().to_string())
    }
}
```

- [ ] **Step 6: Run the test + build**

Run: `cargo test --bin portfolio-tracker clickup:: 2>&1 | tail -15`
Expected: PASS (`from_env_errors_without_token`).
Run: `cargo build --bin portfolio-tracker 2>&1 | tail -10`
Expected: builds (warnings about unused trait methods are fine until Task 2).

- [ ] **Step 7: Commit**

```bash
git add backend/src/clickup/ backend/src/main.rs
git commit -m "feat(clickup): add REST client behind a ClickUpApi trait seam"
```

---

### Task 2: `list_projects` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

The dispatcher handlers for ClickUp tools take `&dyn ClickUpApi` so tests pass a
fake; the `dispatch` match arms construct the real client from env. First add a
reusable test fake.

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, inside `definitions()`'s `json!([ ... ])`
array, after the last existing object (currently `list_instruments`; add a comma
after its closing brace), add:

```rust
{
    "name": "list_projects",
    "description": "List the owner's freelance projects (ClickUp lists). Use to find which project a task belongs to, and before create_project to avoid duplicates.",
    "input_schema": { "type": "object", "properties": {} }
}
```
Append `"list_projects"` to the `defines_all_tools_with_schemas` names vec
(after the current last entry, matching JSON-array order).

- [ ] **Step 2: Add the test fake + a failing handler test**

In `backend/src/assistant/dispatcher.rs`'s `#[cfg(test)] mod tests`, add a fake
(reused by Tasks 3–4) and the test:

```rust
use crate::clickup::{ClickUpApi, ClickUpError, NewTask, Project};
use std::sync::Mutex;

#[derive(Default)]
struct FakeClickUp {
    projects: Mutex<Vec<Project>>,
    created_tasks: Mutex<Vec<(String, String)>>, // (list_id, title)
}

#[async_trait::async_trait]
impl ClickUpApi for FakeClickUp {
    async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError> {
        Ok(self.projects.lock().unwrap().clone())
    }
    async fn create_project(&self, name: &str) -> Result<Project, ClickUpError> {
        let p = Project { id: format!("list_{name}"), name: name.to_string() };
        self.projects.lock().unwrap().push(p.clone());
        Ok(p)
    }
    async fn create_task(&self, list_id: &str, task: &NewTask) -> Result<String, ClickUpError> {
        self.created_tasks.lock().unwrap().push((list_id.to_string(), task.name.clone()));
        Ok(format!("task_{}", task.name))
    }
}

#[tokio::test]
async fn list_projects_formats_known_projects() {
    let fake = FakeClickUp::default();
    fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
    let out = clickup_list_projects(&fake).await.unwrap();
    assert!(out.contains("PT AIS"), "{out}");
}

#[tokio::test]
async fn list_projects_empty_is_explicit() {
    let fake = FakeClickUp::default();
    let out = clickup_list_projects(&fake).await.unwrap();
    assert!(out.contains("belum ada project"), "{out}");
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_projects 2>&1 | tail -15`
Expected: FAIL (`clickup_list_projects` not found).

- [ ] **Step 4: Implement the handler + dispatch arm**

In `backend/src/assistant/dispatcher.rs`, add near the other handlers:

```rust
async fn clickup_list_projects(api: &dyn crate::clickup::ClickUpApi) -> Result<String, String> {
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    if projects.is_empty() {
        return Ok("belum ada project di ClickUp".into());
    }
    let mut out = String::new();
    for p in projects {
        out.push_str(&format!("#{} {}\n", p.id, p.name));
    }
    Ok(out)
}
```

Add the dispatch arm in the `match name` block, after the last existing arm
(`"list_instruments" => ...`):

```rust
"list_projects" => match crate::clickup::ClickUpClient::from_env() {
    Ok(api) => clickup_list_projects(&api).await,
    Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
},
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_projects 2>&1 | tail -15`
Expected: PASS (both).
Run the full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat(clickup): add list_projects assistant tool"
```

---

### Task 3: `create_project` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, after the `list_projects` object (comma
after its brace):

```rust
{
    "name": "create_project",
    "description": "Create a new freelance project (a ClickUp list) in the configured space. Always ask the user to confirm before calling — this creates data in ClickUp.",
    "input_schema": {
        "type": "object",
        "properties": { "name": { "type": "string", "description": "Project name, e.g. PT AIS" } },
        "required": ["name"]
    }
}
```
Append `"create_project"` to the `defines_all_tools_with_schemas` names vec
(after `"list_projects"`). Also add to `required_fields_are_marked`:
`assert_eq!(find("create_project")["input_schema"]["required"], serde_json::json!(["name"]));`

- [ ] **Step 2: Write the failing test**

In the dispatcher test module:

```rust
#[tokio::test]
async fn create_project_creates_and_reports() {
    let fake = FakeClickUp::default();
    let out = clickup_create_project(&fake, &serde_json::json!({ "name": "Klien Baru" })).await.unwrap();
    assert!(out.contains("Klien Baru"), "{out}");
    assert!(fake.projects.lock().unwrap().iter().any(|p| p.name == "Klien Baru"));
}

#[tokio::test]
async fn create_project_requires_name() {
    let fake = FakeClickUp::default();
    let err = clickup_create_project(&fake, &serde_json::json!({})).await.unwrap_err();
    assert!(err.contains("name"), "{err}");
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_project 2>&1 | tail -15`
Expected: FAIL (`clickup_create_project` not found).

- [ ] **Step 4: Implement handler + dispatch arm**

```rust
async fn clickup_create_project(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let name = str_arg(input, "name").ok_or("missing required argument 'name'")?;
    let project = api.create_project(name).await.map_err(|e| format!("{e}"))?;
    Ok(format!("project '{}' dibuat di ClickUp", project.name))
}
```

Dispatch arm after `"list_projects" => ...`:

```rust
"create_project" => match crate::clickup::ClickUpClient::from_env() {
    Ok(api) => clickup_create_project(&api, input).await,
    Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
},
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_project 2>&1 | tail -15`
Expected: PASS (both).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat(clickup): add create_project assistant tool"
```

---

### Task 4: `create_task` tool (project resolution + due date)

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

`create_task` resolves the project by name (case-insensitive) against
`list_projects`. If not found, it returns a clear error so the agent can offer
`create_project` (the prompt drives that in Task 5). Optional `due` is parsed
with the existing `parse_tool_datetime` and converted to epoch ms.

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, after the `create_project` object:

```rust
{
    "name": "create_task",
    "description": "Add a task to a freelance project (ClickUp). Pass the project name; if you don't know which project the user means and there is more than one, ask first. If the named project doesn't exist, offer to create it with create_project before retrying.",
    "input_schema": {
        "type": "object",
        "properties": {
            "project": { "type": "string", "description": "Project (ClickUp list) name the task belongs to" },
            "title": { "type": "string", "description": "What the task is" },
            "due": { "type": "string", "description": "Optional due date, RFC3339 with +07:00 offset, e.g. 2026-06-14T17:00:00+07:00" }
        },
        "required": ["project", "title"]
    }
}
```
Append `"create_task"` to the names vec (after `"create_project"`). Add to
`required_fields_are_marked`:
`assert_eq!(find("create_task")["input_schema"]["required"], serde_json::json!(["project", "title"]));`

- [ ] **Step 2: Write the failing tests**

In the dispatcher test module:

```rust
#[tokio::test]
async fn create_task_adds_to_matching_project() {
    let fake = FakeClickUp::default();
    fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
    let out = clickup_create_task(&fake, &serde_json::json!({
        "project": "pt ais", "title": "bikin kontrak"
    })).await.unwrap();
    assert!(out.contains("bikin kontrak"), "{out}");
    let created = fake.created_tasks.lock().unwrap();
    assert_eq!(created.len(), 1);
    assert_eq!(created[0].0, "l1", "task went to the matched list");
}

#[tokio::test]
async fn create_task_unknown_project_reports_for_offer() {
    let fake = FakeClickUp::default();
    let err = clickup_create_task(&fake, &serde_json::json!({
        "project": "Klien Baru", "title": "x"
    })).await.unwrap_err();
    assert!(err.contains("Klien Baru"), "{err}");
    assert!(err.contains("belum ada"), "{err}");
    assert!(fake.created_tasks.lock().unwrap().is_empty(), "no task created");
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_task 2>&1 | tail -15`
Expected: FAIL (`clickup_create_task` not found).

- [ ] **Step 4: Implement handler + dispatch arm**

```rust
async fn clickup_create_task(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let project = str_arg(input, "project").ok_or("missing required argument 'project'")?;
    let title = str_arg(input, "title").ok_or("missing required argument 'title'")?;
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    let matched = projects
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(project))
        .ok_or_else(|| format!("project '{project}' belum ada — tawarkan buat project baru dulu"))?;
    let due_date_ms = match str_arg(input, "due") {
        Some(raw) => {
            let dt = parse_tool_datetime(raw)
                .ok_or_else(|| format!("due '{raw}' tidak terbaca — pakai RFC3339 +07:00"))?;
            Some(dt.timestamp_millis())
        }
        None => None,
    };
    let task = crate::clickup::NewTask { name: title.to_string(), due_date_ms };
    api.create_task(&matched.id, &task).await.map_err(|e| format!("{e}"))?;
    Ok(format!("task '{title}' ditambahkan ke project '{}'", matched.name))
}
```
Note: `parse_tool_datetime` is already imported at the top of dispatcher.rs
(`use super::time::{parse_tool_datetime, ...}`). Verify with
`grep -n "parse_tool_datetime" src/assistant/dispatcher.rs`.

Dispatch arm after `"create_project" => ...`:

```rust
"create_task" => match crate::clickup::ClickUpClient::from_env() {
    Ok(api) => clickup_create_task(&api, input).await,
    Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
},
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_task 2>&1 | tail -15`
Expected: PASS (both).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat(clickup): add create_task assistant tool with project resolution"
```

---

### Task 5: System-prompt disambiguation + schema-names test

**Files:**
- Modify: `backend/src/assistant/agent.rs`

- [ ] **Step 1: Write the failing prompt test**

In `backend/src/assistant/agent.rs` test module:

```rust
#[test]
fn system_prompt_mentions_the_project_tools() {
    let prompt = system_prompt("2026-06-13T10:00:00+07:00");
    assert!(prompt.contains("list_projects"), "{prompt}");
    assert!(prompt.contains("create_project"), "{prompt}");
    assert!(prompt.contains("create_task"), "{prompt}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_the_project_tools 2>&1 | tail -15`
Expected: FAIL (prompt missing `list_projects`).

- [ ] **Step 3: Extend the `SYSTEM` const**

In `backend/src/assistant/agent.rs`, append to the END of the `SYSTEM` string
literal (keep existing text; add before the closing `";`, using the same
backslash-newline continuation style):

```
 You also manage freelance projects in ClickUp. When the user wants to add a \
task (e.g. 'tambahin task bikin kontrak'), call list_projects: if the user \
named no project and more than one exists, ask which project; then call \
create_task with that project name and the title. If create_task reports the \
project 'belum ada', ask the user whether to create it, and only after they \
agree call create_project, then retry create_task. ALWAYS ask the user to \
confirm before create_project — it creates data in ClickUp. Creating a task \
itself is immediate, like a todo.
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_the_project_tools 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 5: Run the full suite + build**

Run: `cargo test --bin portfolio-tracker 2>&1 | tail -8` → report counts, expect 0 failed.
Run: `cargo build --bin portfolio-tracker 2>&1 | tail -10` → report any new warnings (the `ClickUpClient` HTTP methods are now reachable via dispatch, so unused-method warnings from Task 1 should be gone).

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/agent.rs
git commit -m "feat(clickup): teach assistant the project task + disambiguation flow"
```

---

## Self-Review Notes

- **Spec coverage (Phase 1 slice):** `clickup` module + trait seam → Task 1;
  `list_projects` → Task 2; `create_project` (confirm-before-create via prompt)
  → Tasks 3 + 5; `create_task` with project resolution + due parsing → Task 4;
  disambiguation + offer-to-create → Task 5; graceful no-token behavior →
  dispatch arms in Tasks 2–4. Phases 2 (list/complete/due-scopes), 3 (billable),
  4 (briefing) are explicitly out of this plan.
- **Type consistency:** `ClickUpApi` methods (`list_projects`,
  `create_project`, `create_task`), `Project { id, name }`, `NewTask { name,
  due_date_ms }`, and handler names (`clickup_list_projects`,
  `clickup_create_project`, `clickup_create_task`) are used identically across
  tasks. Dispatch arm names match tool schema `name`s and the names-vec order
  (`list_projects`, `create_project`, `create_task` appended last).
- **No-placeholder check:** every code step shows complete code; ClickUp v2
  endpoints and auth header (`Authorization: <pk_token>`, no Bearer) are
  concrete.
- **Known limitation (called out, not hidden):** the reqwest impl in Task 1 is
  not unit-tested (needs a live server); it is exercised manually against the
  real API and via the fake in Tasks 2–4. The dispatch arms' env-construction
  glue is likewise thin and not unit-tested.
