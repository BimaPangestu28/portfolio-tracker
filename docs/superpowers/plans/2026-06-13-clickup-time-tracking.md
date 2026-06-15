# ClickUp Time Tracking Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Chat-driven ClickUp time tracking — start/stop/current timer, an hours report, and manual entry — as a thin passthrough with no local state.

**Architecture:** New pure helpers (`clickup/report.rs`) for duration parsing, hours aggregation, and period windows. The `ClickUpApi` trait + `ClickUpClient` gain five time-tracking methods hitting `/team/{team_id}/time_entries/*` (new `CLICKUP_TEAM_ID` env). Five agent tools wire them through the dispatcher, resolving task names via existing `list_tasks`. No DB, no migration.

**Tech Stack:** Rust, reqwest, serde_json, chrono. Tests: `cargo test <filter>` from `backend/` (BIN crate — never `cargo test --lib`, never `cargo fmt`).

---

## Conventions

- All paths relative to `backend/`. Run cargo from `backend/`. Commit from repo root.
- ClickUp durations/timestamps are epoch **milliseconds**; the API often returns them as **strings** — parse defensively (`as_i64().or_else(|| as_str().and_then(|s| s.parse().ok()))`).
- End every commit body with:
  `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`

---

## Task 1: Pure helpers — structs + report module

**Files:**
- Modify: `src/clickup/client.rs` (add `TimeEntry`, `RunningEntry` structs)
- Create: `src/clickup/report.rs`
- Modify: `src/clickup/mod.rs` (declare `pub mod report;`)

- [ ] **Step 1: Add the structs to `client.rs`**

After the `Task` struct in `src/clickup/client.rs`, add:

```rust
/// A completed/closed ClickUp time entry, used for reporting.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeEntry {
    pub task_id: String,
    pub task_name: String,
    pub project_name: String,
    pub duration_ms: i64,
    pub start_ms: i64,
    pub billable: bool,
}

/// The currently running timer, if any.
#[derive(Debug, Clone, PartialEq)]
pub struct RunningEntry {
    pub task_name: String,
    pub started_ms: i64,
}
```

- [ ] **Step 2: Declare the module in `mod.rs`**

In `src/clickup/mod.rs`, add after `pub mod client;`:

```rust
pub mod report;
```

- [ ] **Step 3: Write `report.rs` with failing tests**

Create `src/clickup/report.rs`:

```rust
//! Pure helpers for time-tracking input/output: duration parsing, hours
//! aggregation, and period windows. No I/O — fully unit-tested.

use crate::clickup::client::TimeEntry;
use chrono::{DateTime, Datelike, Utc};

/// Parse a human duration into milliseconds. Accepts forms like "2 jam",
/// "90 menit", "1j30m", "1.5 jam", "45m", "2h", "30 min". Returns None when no
/// number+unit pair is found or a unit is unrecognised.
pub fn parse_duration(raw: &str) -> Option<i64> {
    let s = raw.trim().to_lowercase();
    let bytes = s.as_bytes();
    let mut total_minutes: f64 = 0.0;
    let mut matched = false;
    let mut i = 0;
    while i < bytes.len() {
        while i < bytes.len() && !bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let num_start = i;
        while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
            i += 1;
        }
        let num: f64 = s[num_start..i].parse().ok()?;
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        let unit_start = i;
        while i < bytes.len() && bytes[i].is_ascii_alphabetic() {
            i += 1;
        }
        let unit = &s[unit_start..i];
        let minutes = if unit.starts_with('j') || unit.starts_with('h') {
            num * 60.0
        } else if unit.starts_with('m') {
            num
        } else {
            return None;
        };
        total_minutes += minutes;
        matched = true;
    }
    if !matched {
        return None;
    }
    Some((total_minutes * 60_000.0).round() as i64)
}

/// Render a millisecond duration as "6j 30m" / "1j" / "30m".
pub fn format_duration(ms: i64) -> String {
    let total_minutes = ms / 60_000;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    if hours > 0 && minutes > 0 {
        format!("{hours}j {minutes}m")
    } else if hours > 0 {
        format!("{hours}j")
    } else {
        format!("{minutes}m")
    }
}

/// Hours for one project, broken down by task. First-seen order is preserved.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectHours {
    pub project: String,
    pub total_ms: i64,
    pub tasks: Vec<(String, i64)>,
}

/// Group entries by project → task, summing durations. Returns the per-project
/// breakdown and the grand total in ms.
pub fn aggregate_hours(entries: &[TimeEntry]) -> (Vec<ProjectHours>, i64) {
    let mut projects: Vec<ProjectHours> = Vec::new();
    let mut grand_total = 0i64;
    for entry in entries {
        grand_total += entry.duration_ms;
        let project = match projects.iter_mut().find(|p| p.project == entry.project_name) {
            Some(existing) => existing,
            None => {
                projects.push(ProjectHours {
                    project: entry.project_name.clone(),
                    total_ms: 0,
                    tasks: Vec::new(),
                });
                projects.last_mut().expect("just pushed")
            }
        };
        project.total_ms += entry.duration_ms;
        match project.tasks.iter_mut().find(|(name, _)| *name == entry.task_name) {
            Some((_, ms)) => *ms += entry.duration_ms,
            None => project.tasks.push((entry.task_name.clone(), entry.duration_ms)),
        }
    }
    (projects, grand_total)
}

/// UTC [start_ms, end_ms] for a reporting scope over the WIB calendar.
/// "today" = start of today; "week" = Monday this week; "month" = the 1st.
/// Anything else falls back to "week". end is `now`.
pub fn period_window(scope: &str, now_utc: DateTime<Utc>) -> (i64, i64) {
    let wib = crate::assistant::time::wib();
    let now_wib = now_utc.with_timezone(&wib);
    let today = now_wib.date_naive();
    let start_date = match scope {
        "today" => today,
        "month" => today.with_day(1).expect("day 1 is valid"),
        _ => today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64),
    };
    let start_ms = start_date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_local_timezone(wib)
        .single()
        .expect("WIB has no DST gaps")
        .with_timezone(&Utc)
        .timestamp_millis();
    (start_ms, now_utc.timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(project: &str, task: &str, ms: i64) -> TimeEntry {
        TimeEntry {
            task_id: format!("id_{task}"),
            task_name: task.into(),
            project_name: project.into(),
            duration_ms: ms,
            start_ms: 0,
            billable: false,
        }
    }

    #[test]
    fn parse_duration_handles_common_forms() {
        assert_eq!(parse_duration("2 jam"), Some(7_200_000));
        assert_eq!(parse_duration("90 menit"), Some(5_400_000));
        assert_eq!(parse_duration("1j30m"), Some(5_400_000));
        assert_eq!(parse_duration("1.5 jam"), Some(5_400_000));
        assert_eq!(parse_duration("45m"), Some(2_700_000));
        assert_eq!(parse_duration("2h"), Some(7_200_000));
        assert_eq!(parse_duration("banana"), None);
        assert_eq!(parse_duration(""), None);
    }

    #[test]
    fn format_duration_renders_hours_and_minutes() {
        assert_eq!(format_duration(9_000_000), "2j 30m");
        assert_eq!(format_duration(3_600_000), "1j");
        assert_eq!(format_duration(1_800_000), "30m");
    }

    #[test]
    fn aggregate_hours_groups_by_project_and_task() {
        let entries = vec![
            entry("PT AIS", "landing", 4 * 3_600_000),
            entry("PT AIS", "kontrak", 2 * 3_600_000 + 1_800_000),
            entry("PT AIS", "landing", 3_600_000), // same task again
            entry("Klien B", "revisi", 2 * 3_600_000),
        ];
        let (projects, grand) = aggregate_hours(&entries);
        assert_eq!(grand, 4 * 3_600_000 + 2 * 3_600_000 + 1_800_000 + 3_600_000 + 2 * 3_600_000);
        assert_eq!(projects.len(), 2);
        assert_eq!(projects[0].project, "PT AIS");
        assert_eq!(projects[0].tasks.len(), 2); // landing merged
        assert_eq!(projects[0].tasks[0], ("landing".to_string(), 5 * 3_600_000));
        assert_eq!(projects[1].project, "Klien B");
    }

    #[test]
    fn period_window_today_starts_at_wib_midnight() {
        // 2026-06-12T05:00:00Z == 12:00 WIB Friday.
        let now = DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (start, end) = period_window("today", now);
        // Start of WIB day = 2026-06-11T17:00:00Z.
        let expected_start = DateTime::parse_from_rfc3339("2026-06-11T17:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected_start);
        assert_eq!(end, now.timestamp_millis());
    }

    #[test]
    fn period_window_week_starts_monday_wib() {
        // Friday 2026-06-12 → Monday is 2026-06-08, 00:00 WIB = 2026-06-07T17:00:00Z.
        let now = DateTime::parse_from_rfc3339("2026-06-12T05:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let (start, _) = period_window("week", now);
        let expected = DateTime::parse_from_rfc3339("2026-06-07T17:00:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected);
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test clickup::report::tests`
Expected: PASS (5 tests). `cargo build` clean (the new structs/fns are dead until later tasks — warnings OK).

- [ ] **Step 5: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/clickup/client.rs backend/src/clickup/report.rs backend/src/clickup/mod.rs
git commit -m "feat(clickup): time-tracking structs + report helpers"
```

---

## Task 2: ClickUp client — team_id + five time-tracking methods

**Files:**
- Modify: `src/clickup/client.rs` (env, trait, impl)
- Modify: `src/assistant/dispatcher.rs` (extend the `FakeClickUp` test fake so the crate still compiles)

- [ ] **Step 1: Add `team_id` to the client + env**

In `ClickUpClient` struct add a field:

```rust
    team_id: Option<String>,
```

In `from_env`, before the final `Ok(Self { ... })`, add:

```rust
        let team_id = std::env::var("CLICKUP_TEAM_ID").ok().filter(|v| !v.trim().is_empty());
```

and include `team_id` in the struct literal. Update the `from_env` doc comment to mention `CLICKUP_TEAM_ID` (optional; required only for time tracking).

Add a small helper method on `impl ClickUpClient` (near `classify`):

```rust
    fn team(&self) -> Result<&str, ClickUpError> {
        self.team_id.as_deref().ok_or(ClickUpError::Api {
            status: 0,
            body: "CLICKUP_TEAM_ID tidak diset".into(),
        })
    }

    /// Parse ClickUp's string-or-number millisecond fields.
    fn parse_ms(value: &serde_json::Value) -> Option<i64> {
        value.as_i64().or_else(|| value.as_str().and_then(|s| s.parse().ok()))
    }

    /// Build a TimeEntry from a ClickUp time-entry JSON object.
    fn parse_time_entry(value: &serde_json::Value) -> crate::clickup::client::TimeEntry {
        crate::clickup::client::TimeEntry {
            task_id: value["task"]["id"].as_str().unwrap_or_default().to_string(),
            task_name: value["task"]["name"].as_str().unwrap_or("(tanpa task)").to_string(),
            project_name: value["task"]["list"]["name"].as_str().unwrap_or("(tanpa project)").to_string(),
            duration_ms: Self::parse_ms(&value["duration"]).unwrap_or(0),
            start_ms: Self::parse_ms(&value["start"]).unwrap_or(0),
            billable: value["billable"].as_bool().unwrap_or(false),
        }
    }
```

- [ ] **Step 2: Extend the `ClickUpApi` trait**

Add to the `pub trait ClickUpApi` block:

```rust
    /// Start a timer on a task.
    async fn start_timer(&self, task_id: &str) -> Result<(), ClickUpError>;
    /// Stop the running timer; `Ok(None)` if nothing was running.
    async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError>;
    /// The currently running timer, if any.
    async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError>;
    /// Completed time entries overlapping [start_ms, end_ms].
    async fn time_entries(&self, start_ms: i64, end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError>;
    /// Log a manual time entry on a task.
    async fn add_time_entry(&self, task_id: &str, duration_ms: i64, start_ms: i64) -> Result<(), ClickUpError>;
```

Also extend the `pub use` in `mod.rs` so `TimeEntry`/`RunningEntry` are reachable:

```rust
pub use client::{ClickUpApi, ClickUpClient, NewTask, Project, RunningEntry, TimeEntry};
```

- [ ] **Step 3: Implement the five methods on `ClickUpClient`**

Add inside `impl ClickUpApi for ClickUpClient`:

```rust
    async fn start_timer(&self, task_id: &str) -> Result<(), ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries/start", self.team()?);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "tid": task_id }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        Ok(())
    }

    async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries/stop", self.team()?);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            // ClickUp returns an error when no timer is running — treat the
            // "no running timer" case as a clean None rather than an error.
            if body.to_lowercase().contains("running") {
                return Ok(None);
            }
            return Err(Self::classify(status, body));
        }
        let parsed: serde_json::Value =
            serde_json::from_str(&body).map_err(|e| ClickUpError::Http(e.to_string()))?;
        Ok(Some(Self::parse_time_entry(&parsed["data"])))
    }

    async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries/current", self.team()?);
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
        let data = &parsed["data"];
        if data.is_null() {
            return Ok(None);
        }
        Ok(Some(RunningEntry {
            task_name: data["task"]["name"].as_str().unwrap_or("(tanpa task)").to_string(),
            started_ms: Self::parse_ms(&data["start"]).unwrap_or(0),
        }))
    }

    async fn time_entries(&self, start_ms: i64, end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError> {
        let url = format!(
            "https://api.clickup.com/api/v2/team/{}/time_entries?start_date={start_ms}&end_date={end_ms}",
            self.team()?
        );
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
        let entries = parsed["data"].as_array().map(|arr| {
            arr.iter().map(Self::parse_time_entry).collect()
        }).unwrap_or_default();
        Ok(entries)
    }

    async fn add_time_entry(&self, task_id: &str, duration_ms: i64, start_ms: i64) -> Result<(), ClickUpError> {
        let url = format!("https://api.clickup.com/api/v2/team/{}/time_entries", self.team()?);
        let resp = self.http.post(&url)
            .header(reqwest::header::AUTHORIZATION, &self.token)
            .json(&serde_json::json!({ "tid": task_id, "duration": duration_ms, "start": start_ms }))
            .send().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        let status = resp.status();
        let body = resp.text().await.map_err(|e| ClickUpError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, body));
        }
        Ok(())
    }
```

- [ ] **Step 4: Extend the `FakeClickUp` test fake (in `dispatcher.rs`)**

The trait grew, so the fake must implement the new methods or the test build breaks. In `src/assistant/dispatcher.rs` tests, update the import line to include the new types:

```rust
    use crate::clickup::client::{ClickUpApi, ClickUpError, NewTask, Project, RunningEntry, Task, TimeEntry};
```

Add fields to `struct FakeClickUp`:

```rust
        running: Mutex<Option<RunningEntry>>,
        entries: Mutex<Vec<TimeEntry>>,
        started: Mutex<Vec<String>>,       // task_ids passed to start_timer
        stopped: Mutex<u32>,
        added: Mutex<Vec<(String, i64)>>,  // (task_id, duration_ms)
```

Add the implementations inside `impl ClickUpApi for FakeClickUp`:

```rust
        async fn start_timer(&self, task_id: &str) -> Result<(), ClickUpError> {
            self.started.lock().unwrap().push(task_id.to_string());
            *self.running.lock().unwrap() = Some(RunningEntry {
                task_name: task_id.to_string(),
                started_ms: 0,
            });
            Ok(())
        }
        async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError> {
            *self.stopped.lock().unwrap() += 1;
            let running = self.running.lock().unwrap().take();
            Ok(running.map(|r| TimeEntry {
                task_id: r.task_name.clone(),
                task_name: r.task_name,
                project_name: "(test)".into(),
                duration_ms: 3_600_000,
                start_ms: 0,
                billable: false,
            }))
        }
        async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError> {
            Ok(self.running.lock().unwrap().clone())
        }
        async fn time_entries(&self, _start_ms: i64, _end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError> {
            Ok(self.entries.lock().unwrap().clone())
        }
        async fn add_time_entry(&self, task_id: &str, duration_ms: i64, _start_ms: i64) -> Result<(), ClickUpError> {
            self.added.lock().unwrap().push((task_id.to_string(), duration_ms));
            Ok(())
        }
```

- [ ] **Step 5: Build + run existing tests**

Run: `cargo build` (clean) and `cargo test clickup` and `cargo test dispatcher::tests` — all existing tests still pass (the fake compiles; no behavior changed yet).

- [ ] **Step 6: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/clickup/client.rs backend/src/clickup/mod.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(clickup): team_id env + time-tracking API methods"
```

---

## Task 3: start / stop / current timer tools

**Files:**
- Modify: `src/assistant/dispatcher.rs` (task-resolve helper, 3 handlers, 3 match arms, tests)

- [ ] **Step 1: Write failing tests**

Add to `src/assistant/dispatcher.rs` tests (the `FakeClickUp` is available there):

```rust
    #[tokio::test]
    async fn start_timer_resolves_task_and_starts() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![Task {
            id: "t9".into(), name: "landing page".into(), status: "open".into(), due_date_ms: None,
        }]);
        let out = clickup_start_timer(&fake, &serde_json::json!({ "task": "landing page" })).await.unwrap();
        assert!(out.to_lowercase().contains("landing page"), "{out}");
        assert_eq!(fake.started.lock().unwrap().as_slice(), &["t9".to_string()]);
    }

    #[tokio::test]
    async fn start_timer_unknown_task_errors() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        let err = clickup_start_timer(&fake, &serde_json::json!({ "task": "ghost" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("ghost") || err.to_lowercase().contains("ketemu"), "{err}");
    }

    #[tokio::test]
    async fn stop_timer_running_then_none() {
        let fake = FakeClickUp::default();
        *fake.running.lock().unwrap() = Some(RunningEntry { task_name: "landing".into(), started_ms: 0 });
        let out = clickup_stop_timer(&fake).await.unwrap();
        assert!(out.to_lowercase().contains("landing"), "{out}");
        let out2 = clickup_stop_timer(&fake).await.unwrap();
        assert!(out2.to_lowercase().contains("nggak ada"), "{out2}");
    }

    #[tokio::test]
    async fn current_timer_reports_running_or_idle() {
        let fake = FakeClickUp::default();
        assert!(clickup_current_timer(&fake).await.unwrap().to_lowercase().contains("nggak ada"));
        *fake.running.lock().unwrap() = Some(RunningEntry { task_name: "kontrak".into(), started_ms: 0 });
        assert!(clickup_current_timer(&fake).await.unwrap().to_lowercase().contains("kontrak"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test start_timer_resolves_task_and_starts`
Expected: FAIL (`clickup_start_timer` not defined).

- [ ] **Step 3: Add the task-resolve helper + three handlers**

In `src/assistant/dispatcher.rs` (near the other `clickup_*` handlers), add:

```rust
/// Resolve a task name to (task_id, task_name, project_name) by scanning open
/// tasks across all projects. Exact (case-insensitive) matches win; otherwise
/// fall back to substring matches. Errors on no match or ambiguity so the model
/// asks the user instead of guessing.
async fn resolve_clickup_task(
    api: &dyn crate::clickup::ClickUpApi,
    name: &str,
) -> Result<(String, String, String), String> {
    let needle = name.to_lowercase();
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    let mut exact = Vec::new();
    let mut partial = Vec::new();
    for project in &projects {
        for task in api.list_tasks(&project.id).await.map_err(|e| format!("{e}"))? {
            let hay = task.name.to_lowercase();
            if hay == needle {
                exact.push((task.id.clone(), task.name.clone(), project.name.clone()));
            } else if hay.contains(&needle) {
                partial.push((task.id.clone(), task.name.clone(), project.name.clone()));
            }
        }
    }
    let mut hits = if !exact.is_empty() { exact } else { partial };
    match hits.len() {
        0 => Err(format!("task '{name}' nggak ketemu — sebutin nama task yang ada ya")),
        1 => Ok(hits.remove(0)),
        _ => Err(format!("ada beberapa task yang cocok '{name}' — sebutin lebih spesifik")),
    }
}

async fn clickup_start_timer(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let name = str_arg(input, "task").ok_or("missing required argument 'task'")?;
    let (task_id, task_name, project) = resolve_clickup_task(api, name).await?;
    api.start_timer(&task_id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("timer jalan buat '{task_name}' ({project})"))
}

async fn clickup_stop_timer(api: &dyn crate::clickup::ClickUpApi) -> Result<String, String> {
    match api.stop_timer().await.map_err(|e| format!("{e}"))? {
        Some(entry) => Ok(format!(
            "timer '{}' distop — {}",
            entry.task_name,
            crate::clickup::report::format_duration(entry.duration_ms)
        )),
        None => Ok("nggak ada timer yang jalan".into()),
    }
}

async fn clickup_current_timer(api: &dyn crate::clickup::ClickUpApi) -> Result<String, String> {
    match api.current_timer().await.map_err(|e| format!("{e}"))? {
        Some(running) => Ok(format!("lagi ngerjain '{}'", running.task_name)),
        None => Ok("lagi nggak ada timer yang jalan".into()),
    }
}
```

- [ ] **Step 4: Add the dispatch arms**

In the `match name` block in `dispatch`, add (after the existing ClickUp arms, following the same `from_env` gating pattern):

```rust
        "start_timer" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_start_timer(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "stop_timer" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_stop_timer(&api).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "current_timer" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_current_timer(&api).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
```

- [ ] **Step 5: Run tests**

Run: `cargo test _timer` (matches start/stop/current timer tests).
Expected: PASS. `cargo build` clean.

- [ ] **Step 6: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): start/stop/current timer tools"
```

---

## Task 4: time_report + add_time_entry tools

**Files:**
- Modify: `src/assistant/dispatcher.rs` (2 handlers, 2 match arms, tests)

- [ ] **Step 1: Write failing tests**

Add to `src/assistant/dispatcher.rs` tests:

```rust
    #[tokio::test]
    async fn time_report_aggregates_entries() {
        let fake = FakeClickUp::default();
        fake.entries.lock().unwrap().extend([
            TimeEntry { task_id: "t1".into(), task_name: "landing".into(), project_name: "PT AIS".into(), duration_ms: 4 * 3_600_000, start_ms: 0, billable: false },
            TimeEntry { task_id: "t2".into(), task_name: "kontrak".into(), project_name: "PT AIS".into(), duration_ms: 2 * 3_600_000, start_ms: 0, billable: false },
        ]);
        let out = clickup_time_report(&fake, &serde_json::json!({ "scope": "week" })).await.unwrap();
        assert!(out.contains("PT AIS"), "{out}");
        assert!(out.contains("landing"), "{out}");
        assert!(out.contains("6j"), "{out}"); // project total 6j
    }

    #[tokio::test]
    async fn time_report_empty_is_explicit() {
        let fake = FakeClickUp::default();
        let out = clickup_time_report(&fake, &serde_json::json!({ "scope": "week" })).await.unwrap();
        assert!(out.to_lowercase().contains("belum ada"), "{out}");
    }

    #[tokio::test]
    async fn add_time_entry_parses_duration_and_records() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![Task {
            id: "t9".into(), name: "kontrak".into(), status: "open".into(), due_date_ms: None,
        }]);
        let out = clickup_add_time_entry(&fake, &serde_json::json!({ "task": "kontrak", "duration": "2 jam" })).await.unwrap();
        assert!(out.to_lowercase().contains("kontrak"), "{out}");
        let added = fake.added.lock().unwrap();
        assert_eq!(added.as_slice(), &[("t9".to_string(), 7_200_000i64)]);
    }

    #[tokio::test]
    async fn add_time_entry_bad_duration_errors() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![Task {
            id: "t9".into(), name: "kontrak".into(), status: "open".into(), due_date_ms: None,
        }]);
        let err = clickup_add_time_entry(&fake, &serde_json::json!({ "task": "kontrak", "duration": "kapan-kapan" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("durasi"), "{err}");
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test time_report_aggregates_entries`
Expected: FAIL (`clickup_time_report` not defined).

- [ ] **Step 3: Add the two handlers**

In `src/assistant/dispatcher.rs`:

```rust
async fn clickup_time_report(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let scope = match str_arg(input, "scope") {
        Some(s) if matches!(s, "today" | "week" | "month") => s,
        Some(s) => return Err(format!("scope '{s}' nggak dikenal — pakai today/week/month")),
        None => "week",
    };
    let (start_ms, end_ms) = crate::clickup::report::period_window(scope, chrono::Utc::now());
    let mut entries = api.time_entries(start_ms, end_ms).await.map_err(|e| format!("{e}"))?;
    if let Some(project) = str_arg(input, "project") {
        let needle = project.to_lowercase();
        entries.retain(|e| e.project_name.to_lowercase() == needle);
    }
    let (projects, grand_total) = crate::clickup::report::aggregate_hours(&entries);
    if projects.is_empty() {
        return Ok("belum ada jam tercatat untuk periode itu".into());
    }
    let label = match scope { "today" => "Hari ini", "month" => "Bulan ini", _ => "Minggu ini" };
    let mut out = format!("{label}: {}\n", crate::clickup::report::format_duration(grand_total));
    for project in projects {
        out.push_str(&format!("- {}: {}\n", project.project, crate::clickup::report::format_duration(project.total_ms)));
        for (task, ms) in project.tasks {
            out.push_str(&format!("  - {task}: {}\n", crate::clickup::report::format_duration(ms)));
        }
    }
    Ok(out)
}

async fn clickup_add_time_entry(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let name = str_arg(input, "task").ok_or("missing required argument 'task'")?;
    let raw_duration = str_arg(input, "duration").ok_or("missing required argument 'duration'")?;
    let duration_ms = crate::clickup::report::parse_duration(raw_duration)
        .ok_or_else(|| format!("durasi '{raw_duration}' nggak kebaca — coba '2 jam' atau '90 menit'"))?;
    let start_ms = match str_arg(input, "day") {
        Some(raw) => {
            let dt = parse_tool_datetime(raw)
                .ok_or_else(|| format!("day '{raw}' nggak terbaca — pakai RFC3339 +07:00"))?;
            dt.timestamp_millis()
        }
        None => {
            let (start, _) = crate::clickup::report::period_window("today", chrono::Utc::now());
            start
        }
    };
    let (task_id, task_name, _project) = resolve_clickup_task(api, name).await?;
    api.add_time_entry(&task_id, duration_ms, start_ms).await.map_err(|e| format!("{e}"))?;
    Ok(format!(
        "{} dicatat ke '{task_name}'",
        crate::clickup::report::format_duration(duration_ms)
    ))
}
```

- [ ] **Step 4: Add the dispatch arms**

In the `match name` block, after the timer arms:

```rust
        "time_report" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_time_report(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "add_time_entry" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_add_time_entry(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
```

- [ ] **Step 5: Run tests**

Run: `cargo test time_report` and `cargo test add_time_entry`
Expected: PASS. `cargo build` clean.

- [ ] **Step 6: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): time report + manual time entry tools"
```

---

## Task 5: Tool schemas + prompt guidance

**Files:**
- Modify: `src/assistant/tools.rs` (5 schemas + registration test)
- Modify: `src/assistant/agent.rs` (`SYSTEM` prompt)

- [ ] **Step 1: Register the five tool schemas**

In `src/assistant/tools.rs`, add these objects to the `definitions()` array immediately AFTER the `complete_task` object (the last current entry):

```rust
        ,
        {
            "name": "start_timer",
            "description": "Start a ClickUp time tracker on a task. Use for 'mulai ngerjain <task>'. Resolves the task by name; if ambiguous, ask which one.",
            "input_schema": {
                "type": "object",
                "properties": { "task": { "type": "string", "description": "Task name to start timing" } },
                "required": ["task"]
            }
        },
        {
            "name": "stop_timer",
            "description": "Stop the running ClickUp timer. Use for 'udahan' / 'stop'. Reports the task and elapsed time.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "current_timer",
            "description": "Show the currently running ClickUp timer. Use for 'lagi ngerjain apa?'.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "time_report",
            "description": "Report tracked hours grouped by project/task for a period. Use for 'minggu ini berapa jam?' or 'jam di <project> bulan ini'. Default scope is this week.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "scope": { "type": "string", "enum": ["today", "week", "month"], "description": "Reporting period; default week" },
                    "project": { "type": "string", "description": "Optional: limit the report to one project" }
                }
            }
        },
        {
            "name": "add_time_entry",
            "description": "Log time manually on a task when the user forgot to run a timer. Use for 'tambahin 2 jam ke task <name>'. Duration accepts forms like '2 jam', '90 menit', '1j30m'.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "task": { "type": "string", "description": "Task name" },
                    "duration": { "type": "string", "description": "Duration, e.g. '2 jam', '90 menit', '1j30m'" },
                    "day": { "type": "string", "description": "Optional day the work happened, RFC3339 +07:00; defaults to today" }
                },
                "required": ["task", "duration"]
            }
        }
```

> Note: the leading `,` before the first new object closes the previous `complete_task` object's array element. Verify the array stays valid JSON (the `serde_json::json!` macro will fail to compile if not).

Update the `defines_all_tools_with_schemas` expected name list — append after `"complete_task"`:

```rust
                "complete_task",
                "start_timer", "stop_timer", "current_timer", "time_report", "add_time_entry",
```

- [ ] **Step 2: Run the registration test**

Run: `cargo test tools::tests`
Expected: PASS (the order in `definitions()` matches the updated vector).

- [ ] **Step 3: Add prompt guidance to `agent.rs`**

In `src/assistant/agent.rs`, append to the end of the `SYSTEM` string (before the closing `";`), continuing the `\`-joined style:

```rust
 You can track time on ClickUp tasks: 'mulai ngerjain <task>' → start_timer; \
'udahan'/'stop' → stop_timer; 'lagi ngerjain apa?' → current_timer. Timers always attach to a \
ClickUp task — if the task name is ambiguous, ask which one. For 'minggu ini berapa jam?' or \
'jam di <project> bulan ini' call time_report (scope today/week/month). When the user logged time \
after the fact ('tambahin 2 jam ke task kontrak kemarin'), call add_time_entry with the task and \
duration.
```

- [ ] **Step 4: Build + full test run**

Run: `cargo build` (clean) and `cargo test` (all pass).

- [ ] **Step 5: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/tools.rs backend/src/assistant/agent.rs
git commit -m "feat(assistant): register time-tracking tools + prompt guidance"
```

---

## Final verification

- [ ] Run `cargo test` (all pass) and `cargo build` (0 warnings).

## Spec coverage check

- start/stop/current timer → Task 2 (client), Task 3 (tools).
- hours report (period + project filter) → Task 1 (aggregate/window), Task 2 (`time_entries`), Task 4 (tool).
- manual entry → Task 1 (`parse_duration`), Task 2 (`add_time_entry`), Task 4 (tool).
- `CLICKUP_TEAM_ID` config + clean error when unset → Task 2 (`team()`), Task 3/4 (gating).
- task resolution by name with ambiguity handling → Task 3 (`resolve_clickup_task`).
- hours-only (no billing math), ClickUp source of truth, no migration → honoured throughout.
- prompt guidance → Task 5.
- tests for parsing/aggregation/window/handlers/registration → Tasks 1, 3, 4, 5.
