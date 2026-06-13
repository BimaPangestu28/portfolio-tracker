# ClickUp Project Assistant — Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When creating a task, let the user flag it billable and set an amount, written to the ClickUp `Billable` (checkbox) and `Amount` (money) custom fields, so the future invoice generator can read them.

**Architecture:** Extend `NewTask` with `billable`/`amount`; the real `ClickUpClient::create_task` resolves the two custom-field ids per list (GET list fields, match by configurable name) and attaches them to the create-task payload, silently skipping a field that doesn't exist. The `create_task` tool gains optional `billable`/`amount` inputs; the trait seam carries them so the fake captures them in tests.

**Tech Stack:** Rust, reqwest, async-trait, serde_json. Binary crate `portfolio-tracker` — `cargo test --bin portfolio-tracker <filter>` from `backend/`.

**Scope note:** Phase 3 of `docs/superpowers/specs/2026-06-13-clickup-project-assistant-design.md`. Builds on Phases 1-2. Phase 4 (briefing) follows. Prerequisite: the `Billable` + `Amount` custom fields must be created in the ClickUp Space UI; the backend reads their ids at task-create time and skips them if absent.

---

## File Structure

- `backend/src/clickup/client.rs` — `NewTask` gains `billable: Option<bool>`, `amount: Option<f64>`; `ClickUpClient` gains `billable_field`/`amount_field` name config (env, defaults "Billable"/"Amount"); `create_task` resolves+attaches custom fields via a new private `list_fields` helper.
- `backend/src/assistant/tools.rs` — `create_task` schema gains `billable`/`amount`.
- `backend/src/assistant/dispatcher.rs` — `clickup_create_task` reads `billable`/`amount`; `FakeClickUp` captures them; tests.
- `backend/src/assistant/agent.rs` — `SYSTEM` prompt mentions billable; prompt test.

All commands run from `backend/`.

---

### Task 1: Carry billable/amount through to ClickUp

**Files:**
- Modify: `backend/src/clickup/client.rs`
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Extend `NewTask`**

In `client.rs`, change the `NewTask` struct to:

```rust
/// Fields for creating a task.
#[derive(Debug, Clone, Default)]
pub struct NewTask {
    pub name: String,
    pub due_date_ms: Option<i64>,
    /// Sets the ClickUp `Billable` checkbox custom field when present.
    pub billable: Option<bool>,
    /// Sets the ClickUp `Amount` money custom field (IDR) when present.
    pub amount: Option<f64>,
}
```

- [ ] **Step 2: Add field-name config to the client + from_env**

Add `billable_field: String` and `amount_field: String` to the `ClickUpClient` struct. In `from_env`, before the `Ok(Self {...})`:

```rust
        let billable_field = std::env::var("CLICKUP_BILLABLE_FIELD").unwrap_or_else(|_| "Billable".into());
        let amount_field = std::env::var("CLICKUP_AMOUNT_FIELD").unwrap_or_else(|_| "Amount".into());
```
and include both in the `Ok(Self { ... })` constructor.

- [ ] **Step 3: Add a private `list_fields` helper + attach custom fields in `create_task`**

In `impl ClickUpClient` (the inherent impl with `from_env`/`classify`), add:

```rust
    /// (id, name) of every custom field visible on a list.
    async fn list_fields(&self, list_id: &str) -> Result<Vec<(String, String)>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/list/{list_id}/field");
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
        let fields = parsed["fields"].as_array().map(|arr| {
            arr.iter().filter_map(|f| {
                Some((f["id"].as_str()?.to_string(), f["name"].as_str()?.to_string()))
            }).collect()
        }).unwrap_or_default();
        Ok(fields)
    }
```

Then in `ClickUpClient::create_task` (the trait-impl method), after building the base `payload` with `name`/`due_date` and BEFORE sending, add the custom-fields resolution:

```rust
        if task.billable.is_some() || task.amount.is_some() {
            let fields = self.list_fields(list_id).await?;
            let mut custom = Vec::new();
            if let Some(b) = task.billable {
                if let Some((id, _)) = fields.iter().find(|(_, n)| n.eq_ignore_ascii_case(&self.billable_field)) {
                    custom.push(serde_json::json!({ "id": id, "value": b }));
                }
            }
            if let Some(a) = task.amount {
                if let Some((id, _)) = fields.iter().find(|(_, n)| n.eq_ignore_ascii_case(&self.amount_field)) {
                    custom.push(serde_json::json!({ "id": id, "value": a }));
                }
            }
            if !custom.is_empty() {
                payload["custom_fields"] = serde_json::json!(custom);
            }
        }
```
(Place this between the `due_date` insertion and the `self.http.post(...)` call. `payload` must be declared `let mut payload`.)

- [ ] **Step 4: Add `billable`/`amount` to the `create_task` schema**

In `backend/src/assistant/tools.rs`, in the `create_task` object's `properties`, add (after `due`):

```rust
            "billable": { "type": "boolean", "description": "Mark the task billable (sets the ClickUp Billable field, if it exists)" },
            "amount": { "type": "number", "description": "Billable amount in IDR (sets the ClickUp Amount field, if it exists)" }
```
The `required` array stays `["project", "title"]`. The names vec is unchanged (no new tool).

- [ ] **Step 5: Capture billable/amount in `FakeClickUp` + write failing test**

In `backend/src/assistant/dispatcher.rs` test module, add two fields to the `FakeClickUp` struct:

```rust
        created_billables: Mutex<Vec<Option<bool>>>,
        created_amounts: Mutex<Vec<Option<f64>>>,
```
and in its `create_task` impl, after pushing `created_dues`, add:

```rust
            self.created_billables.lock().unwrap().push(task.billable);
            self.created_amounts.lock().unwrap().push(task.amount);
```

Add the test:

```rust
#[tokio::test]
async fn create_task_passes_billable_and_amount() {
    let fake = FakeClickUp::default();
    fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
    clickup_create_task(&fake, &serde_json::json!({
        "project": "PT AIS", "title": "landing page", "billable": true, "amount": 10000000
    })).await.unwrap();
    assert_eq!(fake.created_billables.lock().unwrap()[0], Some(true));
    assert_eq!(fake.created_amounts.lock().unwrap()[0], Some(10_000_000.0));
}

#[tokio::test]
async fn create_task_without_billable_is_none() {
    let fake = FakeClickUp::default();
    fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
    clickup_create_task(&fake, &serde_json::json!({ "project": "PT AIS", "title": "x" })).await.unwrap();
    assert_eq!(fake.created_billables.lock().unwrap()[0], None);
    assert_eq!(fake.created_amounts.lock().unwrap()[0], None);
}
```

- [ ] **Step 6: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_task_passes_billable 2>&1 | tail -15`
Expected: FAIL (assertion: billable not captured — the handler doesn't set it yet).

- [ ] **Step 7: Set billable/amount in the handler**

In `backend/src/assistant/dispatcher.rs`, update `clickup_create_task` where it builds `NewTask`. The current code is:

```rust
    let task = crate::clickup::NewTask { name: title.to_string(), due_date_ms };
```
Change to:

```rust
    let billable = input.get("billable").and_then(|v| v.as_bool());
    let amount = input.get("amount").and_then(|v| v.as_f64());
    let task = crate::clickup::NewTask { name: title.to_string(), due_date_ms, billable, amount };
```

- [ ] **Step 8: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_task 2>&1 | tail -15`
Expected: PASS (the two new tests plus the existing create_task tests).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.
`cargo build --bin portfolio-tracker 2>&1 | grep -c warning` → expect 0.

- [ ] **Step 9: Commit**

```bash
git add src/clickup/client.rs src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(clickup): set Billable/Amount custom fields on create_task"
```

---

### Task 2: Prompt guidance for billable tasks

**Files:**
- Modify: `backend/src/assistant/agent.rs`

- [ ] **Step 1: Write the failing test**

In `backend/src/assistant/agent.rs` test module:

```rust
#[test]
fn system_prompt_mentions_billable() {
    let prompt = system_prompt("2026-06-13T10:00:00+07:00");
    assert!(prompt.contains("billable"), "{prompt}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_billable 2>&1 | tail -12`
Expected: FAIL.

- [ ] **Step 3: Extend the `SYSTEM` const**

Append to the END of the `SYSTEM` literal (keep existing text; ` \` continuation style), after the task-reading sentence:

```
 When the user says a task is billable or gives a price (e.g. 'task landing page PT AIS, billable 10 juta'), pass billable=true and amount (in IDR) to create_task so it can be invoiced later.
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_billable 2>&1 | tail -12`
Expected: PASS.

- [ ] **Step 5: Full suite + build**

Run: `cargo test --bin portfolio-tracker 2>&1 | tail -8` → report counts, 0 failed.
Run: `cargo build --bin portfolio-tracker 2>&1 | grep -c warning` → expect 0.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/agent.rs
git commit -m "feat(clickup): teach assistant to flag billable tasks with an amount"
```

---

## Self-Review Notes

- **Spec coverage (Phase 3):** billable + amount on create_task → Task 1; custom-field resolution with graceful skip when a field is absent → Task 1 Step 3; prompt → Task 2. Invoice generator is out of scope (separate future project) — this only writes the data it will consume.
- **Type consistency:** `NewTask { name, due_date_ms, billable: Option<bool>, amount: Option<f64> }` (Default-derived so the Phase 1/2 struct-literal sites that don't set the new fields would break — Task 1 Step 7 updates the only literal site, in `clickup_create_task`). The fake captures via parallel `created_billables`/`created_amounts` vecs (mirrors `created_dues`).
- **Graceful degradation:** a missing `Billable`/`Amount` field in ClickUp means that custom field is skipped; the task is still created. Field names are env-overridable (`CLICKUP_BILLABLE_FIELD`/`CLICKUP_AMOUNT_FIELD`).
- **Untested-by-design:** the real client's `list_fields` GET + custom-field attach is HTTP — exercised manually; the trait contract (NewTask carries the values, fake captures them) is unit-tested.
