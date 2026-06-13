# ClickUp Time Tracking — Design

**Date:** 2026-06-13
**Status:** Approved (design), pending implementation plan
**Phase:** Productivity roadmap — Fase 3

## Overview

Let the owner track time on freelance work straight from chat: start/stop a timer
on a ClickUp task, see what's currently running, get an hours report for a period,
and log time manually when they forgot to start a timer. ClickUp is the source of
truth for all timing data — the backend is a thin passthrough with no local state.

## Goals

- "mulai ngerjain landing page PT AIS" → start a ClickUp timer on that task.
- "udahan" / "stop" → stop the running timer, report task + elapsed.
- "lagi ngerjain apa?" → show the currently running timer (task + elapsed).
- "minggu ini berapa jam?" / "jam di PT AIS bulan ini" → hours report grouped by
  project/task for a period.
- "tambahin 2 jam ke task kontrak kemarin" → manual time entry.

## Non-Goals (YAGNI for v1)

- No local mirror of time entries (no DB table, **no migration**). Reports are
  computed on demand from ClickUp.
- No "your timer has been running for N hours" proactive reminder.
- No task-less timers — every timer attaches to a ClickUp task.
- No hourly rate / money math. Reports show **hours only**; billing stays the
  existing fixed `Amount`-per-task model, separate from time tracking.

## Constraints / Dependencies

- ClickUp time tracking endpoints are keyed by **`team_id` (workspace)**, not the
  `space_id` the client already holds. New env var `CLICKUP_TEAM_ID` (workspace
  `90182781247`). When unset, the time-tracking tools return a "time tracking
  belum dikonfigurasi" error — the same degradation pattern the existing ClickUp
  tools use when `CLICKUP_API_TOKEN` is missing.
- ClickUp time tracking is a paid-plan feature; it must be enabled on the account.
- ClickUp durations are milliseconds; start times are epoch-ms.

## Architecture

### ClickUp client (`backend/src/clickup/client.rs`)

Extend the `ClickUpApi` trait + `ClickUpClient` impl with time-tracking methods
(all under `https://api.clickup.com/api/v2/team/{team_id}/time_entries`):

- `start_timer(task_id: &str) -> Result<(), ClickUpError>` — POST `/start`, body `{ "tid": task_id }`.
- `stop_timer() -> Result<Option<TimeEntry>, ClickUpError>` — POST `/stop`. Returns
  the stopped entry, or `None` when no timer was running (ClickUp returns an error
  in that case — map "no running timer" to `Ok(None)`).
- `current_timer() -> Result<Option<RunningEntry>, ClickUpError>` — GET `/current`.
  `None` when nothing is running.
- `time_entries(start_ms: i64, end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError>`
  — GET with `start_date`/`end_date` query params.
- `add_time_entry(task_id: &str, duration_ms: i64, start_ms: i64) -> Result<(), ClickUpError>`
  — POST, body `{ "tid", "duration", "start" }`.

`CLICKUP_TEAM_ID` is read in `from_env` and stored on the client (`Option<String>`;
the time-tracking methods error cleanly when it is `None`).

New structs:

```
pub struct TimeEntry {
    pub task_id: String,
    pub task_name: String,
    pub project_name: String,   // ClickUp "list" name, for report grouping
    pub duration_ms: i64,
    pub start_ms: i64,
    pub billable: bool,
}

pub struct RunningEntry {
    pub task_name: String,
    pub started_ms: i64,
}
```

(Project/list name comes from the entry's `task.list.name` in the ClickUp response;
fall back to a placeholder if absent.)

### Task resolution

`start_timer` and `add_time_entry` take a **task name** from the user. Resolve it to
a `task_id` by searching `list_tasks` across projects (the same approach
`create_task` uses for project lookup). On no match or ambiguous match, return an
error so the model asks the user to clarify — never guess.

### Duration parsing (`backend/src/clickup/` or a small assistant util)

`parse_duration(raw: &str) -> Option<i64>` (milliseconds) handling Indonesian forms:
`"2 jam"`, `"90 menit"`, `"1j30m"`, `"45m"`, `"1.5 jam"`. Pure function, unit-tested.

### Report aggregation (pure function)

`aggregate_hours(entries: &[TimeEntry]) -> Vec<ProjectHours>` grouping by project →
task, summing `duration_ms`, plus a grand total. A `format_duration(ms)` helper
renders `"6j 30m"`. Pure, unit-tested. Example output the report tool returns:

```
Minggu ini: 8j 30m
- PT AIS: 6j 30m
  - landing page: 4j
  - kontrak: 2j 30m
- Klien B: 2j
  - revisi: 2j
```

### Agent tools (`backend/src/assistant/tools.rs` + `dispatcher.rs`)

Five tools, each gated on `ClickUpClient::from_env()` like the existing ClickUp
tools (and additionally requiring `CLICKUP_TEAM_ID`):

- `start_timer` — `{ task: string }` → resolve task, start timer, confirm.
- `stop_timer` — `{}` → stop, report task + elapsed (or "nggak ada timer yang jalan").
- `current_timer` — `{}` → running task + elapsed, or "lagi nggak ada timer jalan".
- `time_report` — `{ scope?: "today"|"week"|"month", project?: string }` → compute
  the WIB period window, fetch entries, aggregate, render. Default scope `week`.
- `add_time_entry` — `{ task: string, duration: string, day?: string }` → resolve
  task, parse duration, default `day` to today; `start_ms` = start of that WIB day.

### Period windows

`time_report` maps `today`/`week`/`month` to a `[start_ms, end_ms]` UTC range over
the WIB calendar (reuse `crate::assistant::time::start_of_today_wib` once Fase 2 is
merged; until then compute locally). Week = Monday-to-now WIB; month = 1st-to-now WIB.

### Prompt (`backend/src/assistant/agent.rs`)

Append guidance to `SYSTEM`: start/stop/current timer, hours report, and manual
entry — including that timers always attach to a ClickUp task, and to ask which
task when a name is ambiguous.

## Error Handling

- Missing `CLICKUP_TEAM_ID` → tool returns "time tracking belum dikonfigurasi".
- `stop_timer` with nothing running → friendly "nggak ada timer yang jalan", not an error.
- Unresolvable/ambiguous task name → error so the model asks the user.
- ClickUp API/network errors propagate through the existing `ClickUpError` mapping.

## Testing

- `parse_duration`: "2 jam"→7200000, "90 menit"→5400000, "1j30m"→5400000,
  "1.5 jam"→5400000, garbage→None.
- `aggregate_hours`: groups by project/task, sums durations, computes total;
  `format_duration`: 9000000→"2j 30m", 3600000→"1j", 1800000→"30m".
- Period-window math for today/week/month over the WIB boundary.
- Dispatcher tests with a fake `ClickUpApi` (extend the existing fake) for each of
  the five tools: start (resolves task), stop (running vs none), current, report
  (aggregated text), add (parses duration).
- Tool registration test updated with the five new names.

## Open Coordination Item

This phase adds **no migration** (pure ClickUp passthrough), so it is free of the
migration-number collision risk. It does touch `agent.rs` `SYSTEM` and `tools.rs`
(tool list) — both also touched by the Fase 2 branch; merge order will need a
trivial conflict resolution on the appended prompt text and the tool-name vector.
