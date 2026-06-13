# ClickUp Project Assistant — Phase 4 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The morning briefing lists ClickUp tasks that are overdue or due today, grouped by project — so the daily planning view covers freelance work too.

**Architecture:** Add an optional `clickup_due` field to `BriefingData`. `gather` queries ClickUp (via `from_env`) for overdue/due-today tasks across projects, degrading to `None` (section omitted) when ClickUp isn't configured and to an empty list when an API call fails. A pure `clickup_due_line` function decides per-task whether/how a task appears, so the filtering is unit-tested without a fake; `render_data_block` renders the section.

**Tech Stack:** Rust, chrono, async-trait, reqwest. Binary crate `portfolio-tracker` — `cargo test --bin portfolio-tracker <filter>` from `backend/`.

**Scope note:** Phase 4 (final) of `docs/superpowers/specs/2026-06-13-clickup-project-assistant-design.md`. Builds on Phases 1-3 (client with `list_projects`/`list_tasks`, `Task` DTO, `end_of_today_wib_ms`).

---

## File Structure

- `backend/src/assistant/proactive/briefing.rs` — `BriefingData.clickup_due: Option<Vec<(String, Vec<String>)>>`; pure `clickup_due_line`; async `gather_clickup_due`; `gather` wiring; `render_data_block` section; tests.

All commands run from `backend/`.

---

### Task 1: ClickUp due/overdue section in the morning briefing

**Files:**
- Modify: `backend/src/assistant/proactive/briefing.rs`

- [ ] **Step 1: Failing test for the pure line function**

In `briefing.rs` test module, add:

```rust
#[test]
fn clickup_due_line_tags_overdue_and_today_only() {
    use crate::clickup::client::Task;
    let now = 100_000i64;
    let end = 200_000i64;
    let mk = |due: Option<i64>| Task { id: "t".into(), name: "kerjaan".into(), status: "to do".into(), due_date_ms: due };
    assert_eq!(clickup_due_line(&mk(Some(50_000)), now, end).as_deref(), Some("kerjaan (overdue)"));
    assert_eq!(clickup_due_line(&mk(Some(150_000)), now, end).as_deref(), Some("kerjaan (hari ini)"));
    assert_eq!(clickup_due_line(&mk(Some(300_000)), now, end), None); // future
    assert_eq!(clickup_due_line(&mk(None), now, end), None);          // no due
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::proactive::briefing::tests::clickup_due_line_tags 2>&1 | tail -12`
Expected: FAIL (`clickup_due_line` not found).

- [ ] **Step 3: Implement the pure function**

In `briefing.rs` (near the other free functions like `classify_todos`):

```rust
/// One briefing line for a task that is overdue or due today; None otherwise
/// (future due date, or no due date — those don't belong in a "due" view).
fn clickup_due_line(task: &crate::clickup::client::Task, now_ms: i64, end_today_ms: i64) -> Option<String> {
    let due = task.due_date_ms?;
    if due < now_ms {
        Some(format!("{} (overdue)", task.name))
    } else if due <= end_today_ms {
        Some(format!("{} (hari ini)", task.name))
    } else {
        None
    }
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::proactive::briefing::tests::clickup_due_line_tags 2>&1 | tail -12`
Expected: PASS.

- [ ] **Step 5: Add the `clickup_due` field + gather helper**

Add to the `BriefingData` struct (after `memory_facts`):

```rust
    /// Overdue/due-today ClickUp tasks grouped by project. `None` when ClickUp
    /// isn't configured (section omitted); `Some(empty)` when nothing is due.
    pub clickup_due: Option<Vec<(String, Vec<String>)>>,
```

Add the orchestration helper (uses the `ClickUpApi` trait so it could be faked; here it is driven by the real client from `gather`):

```rust
/// Collect overdue/due-today tasks grouped by project name.
async fn gather_clickup_due(
    api: &dyn crate::clickup::ClickUpApi,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let end_today = crate::assistant::time::end_of_today_wib_ms(chrono::Utc::now());
    let projects = api.list_projects().await?;
    let mut grouped = Vec::new();
    for project in projects {
        let tasks = api.list_tasks(&project.id).await?;
        let lines: Vec<String> = tasks
            .iter()
            .filter_map(|t| clickup_due_line(t, now_ms, end_today))
            .collect();
        if !lines.is_empty() {
            grouped.push((project.name, lines));
        }
    }
    Ok(grouped)
}
```

- [ ] **Step 6: Wire into `gather`**

In `gather`, before the final `Ok(BriefingData { ... })`, add:

```rust
    let clickup_due = match crate::clickup::ClickUpClient::from_env() {
        Ok(api) => Some(gather_clickup_due(&api).await.unwrap_or_else(|e| {
            tracing::warn!("briefing: clickup due tasks unavailable: {e:#}");
            Vec::new()
        })),
        Err(_) => None, // not configured → section omitted
    };
```
and add `clickup_due,` to the `BriefingData { ... }` constructor.

- [ ] **Step 7: Render the section + failing render tests**

Add render tests first:

```rust
#[test]
fn clickup_section_renders_grouped_due_tasks() {
    let mut d = data();
    d.clickup_due = Some(vec![("PT AIS".into(), vec!["landing page (overdue)".into()])]);
    let block = render_data_block(&d);
    assert!(block.contains("Task ClickUp jatuh tempo:"), "{block}");
    assert!(block.contains("PT AIS"), "{block}");
    assert!(block.contains("landing page (overdue)"), "{block}");
}

#[test]
fn clickup_section_omitted_when_unconfigured() {
    let d = data(); // data() leaves clickup_due = None
    let block = render_data_block(&d);
    assert!(!block.contains("Task ClickUp"), "{block}");
}
```
Update the test helper `data()` to set `clickup_due: None` in its `BriefingData { ... }` literal (otherwise it won't compile after Step 5 adds the field).

Run: `cargo test --bin portfolio-tracker assistant::proactive::briefing::tests::clickup_section 2>&1 | tail -12`
Expected: FAIL (section not rendered yet).

Then implement the render. In `render_data_block`, after the `Review pending` line and before the `memory_facts` block, add:

```rust
    if let Some(due) = &d.clickup_due {
        out.push_str("Task ClickUp jatuh tempo:\n");
        if due.is_empty() {
            out.push_str("(tidak ada)\n");
        } else {
            for (project, lines) in due {
                out.push_str(&format!("- {project}:\n"));
                for line in lines {
                    out.push_str(&format!("  - {line}\n"));
                }
            }
        }
    }
```

- [ ] **Step 8: Run to verify they pass + fix the other `BriefingData` literals**

The `data()` test helper and the `gather` constructor both build `BriefingData` — both must set `clickup_due`. Also search for any other `BriefingData {` literal: `grep -n "BriefingData {" src/assistant/proactive/briefing.rs` and set `clickup_due` in each (the `gather` one uses the computed value; `data()` uses `None`).

Run: `cargo test --bin portfolio-tracker assistant::proactive::briefing:: 2>&1 | tail -12`
Expected: PASS (all briefing tests including the 3 new ones).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.
`cargo build --bin portfolio-tracker 2>&1 | grep -c warning` → expect 0.

- [ ] **Step 9: Commit**

```bash
git add src/assistant/proactive/briefing.rs
git commit -m "feat(clickup): add overdue/due-today ClickUp tasks to the morning briefing"
```

---

## Self-Review Notes

- **Spec coverage (Phase 4):** morning-briefing section querying due/overdue ClickUp tasks grouped by project → Task 1; omitted when unconfigured (`None`), "(tidak ada)" when configured-but-empty.
- **Type consistency:** `clickup_due: Option<Vec<(String, Vec<String>)>>` set in both `gather` and the `data()` test helper; pure `clickup_due_line(&Task, now_ms, end_today_ms) -> Option<String>` reused by `gather_clickup_due`; `end_of_today_wib_ms` (Phase 2) reused.
- **Graceful degradation:** unconfigured → `None` → section absent; configured + API error → `Some(empty)` with a warn → "(tidak ada)". The briefing never fails because of ClickUp.
- **Testability:** the per-task decision is a pure function (unit-tested for overdue/today/future/no-due); rendering is tested with hand-built `BriefingData`; the async orchestration `gather_clickup_due` is thin and exercised via the real client at runtime (consistent with how other `gather` sources hit live services).
