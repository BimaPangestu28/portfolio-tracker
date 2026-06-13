# Daily Planning & Evening Review — Design

**Date:** 2026-06-13
**Status:** Approved (design), pending implementation plan
**Phase:** Productivity roadmap — Fase 2

## Overview

The assistant already pushes a morning briefing (07:00 WIB) listing today's todos,
reminders, and a portfolio summary, plus a weekly recap on Sunday evening. This
feature turns the morning briefing into an actual **day plan** (a time-block built
from events and prioritised todos), adds an on-demand version reachable from chat,
and introduces a **daily evening review** that surfaces unfinished work and offers
to roll it over to tomorrow.

A single deterministic assembler (`plan.rs`) is the one source of truth for "what
does my day look like", consumed by the morning briefing, the on-demand `plan_day`
tool, and the evening review.

## Goals

- Morning briefing reads as a plan (events at their times, todos time-blocked by
  priority and estimate), not a flat list.
- On-demand planning from chat: "rencanain hariku" / "sisa hari ini apa aja".
- Evening review: completed vs unfinished todos, then offer to roll unfinished
  todos to tomorrow — **only after the user confirms**.

## Non-Goals (YAGNI for v1)

- ClickUp freelance tasks are **not** included in the day plan (keeps Fase 2
  decoupled from ClickUp config; can be folded in later).
- No minute-accurate calendar scheduling — todos are time-blocked by ordering and
  estimate, not pinned to exact clock slots.
- No recurring todos.

## Data Model

Migration `0016_todo_priority_estimate.sql`:

```sql
ALTER TABLE todos ADD COLUMN priority TEXT;          -- 'high' | 'normal' | 'low'; NULL = normal
ALTER TABLE todos ADD COLUMN estimate_minutes INTEGER; -- optional duration estimate
```

> **Parallel-work note:** migrations currently end at `0015`. If Fase 3–6 are
> developed concurrently in separate worktrees, each must take a distinct, agreed
> migration number range to avoid the collision that has bitten this project
> before. Fase 2 owns `0016`.

`TodoRow` (`src/repo/todos.rs`) gains:

- `priority: Option<String>`
- `estimate_minutes: Option<i64>`

`NULL` priority is treated as `normal` by the application; no enforced DB CHECK
constraint (the tool schema constrains input instead).

## Components

### `proactive/plan.rs` (new) — day-plan assembler

The single source of truth for the day's shape.

- `build_plan_block(db, day_wib) -> String` (deterministic data block).
- Gathers:
  - **Events** for the day via `events::list_between(day_start, day_end)`, sorted
    by `start_at`.
  - **Open todos** via `todos::list_open`, ordered by: priority (high → normal →
    low), then `due_at` ascending (NULL last), then `estimate_minutes`.
- Emits a plain-text data block: events with their WIB times; todos grouped into
  loose buckets (pagi / siang / sore) with priority + estimate annotations.
- Output is fed to `compose()` with a new `PLAN_SYSTEM` prompt that renders a
  natural, plain-text time-blocked plan (no Markdown, WIB times, copy numbers
  exactly — same constraints as the existing briefing prompt).

### `proactive/briefing.rs` (modified) — morning briefing

- Builds the day-plan portion via `plan.rs`, then appends the existing portfolio
  one-liner, pending reviews, and clearly-relevant remembered facts.
- `BRIEFING_SYSTEM` (in `compose.rs`) is upgraded to plan-style phrasing while
  keeping the portfolio/closing structure.

### `proactive/evening_review.rs` (new) — daily evening review

- `run(db, client, chat_id)`:
  - Completed todos today: `todos::completed_since(today_start_rfc3339)`.
  - Unfinished candidates: open todos with `due_at <= end of today` (overdue or
    due today).
  - Composes a short review with `REVIEW_SYSTEM` (new prompt) ending with a
    rollover question: "Mau geser yang belum kelar ke besok? Balas iya."
  - Pushed via Telegram using the existing claim-before-send path.

### Agent tools (`tools.rs` + `dispatcher.rs`)

- **`plan_day`** — no/optional args; calls `plan::build_plan_block` for today and
  returns the data block to the agent, which phrases it in its normal voice.
  Triggered by SYSTEM-prompt guidance on "rencanain hariku" / "sisa hari ini".
- **`rollover_todos`** — optional `ids: [i64]`. If omitted, rolls **all** open
  todos with `due_at <= end of today`. Shifts each `due_at` forward by one day,
  preserving the time-of-day. Todos with no `due_at` or a future `due_at` are
  left untouched. Returns the count and titles moved.

### Repo (`src/repo/todos.rs`)

- `rollover(db, ids: Option<&[i64]>) -> anyhow::Result<Vec<TodoRow>>` — applies the
  +1-day shift to the selected/eligible todos and returns the moved rows.
- `create`/insert updated to accept `priority` and `estimate_minutes`.

## Data Flow

- **Morning (07:00 WIB):** `tick` → `briefing::run` → `plan::build_plan_block` +
  portfolio block → `compose(BRIEFING_SYSTEM)` → Telegram push.
- **On-demand:** user "rencanain hariku" → agent → `plan_day` tool →
  `plan::build_plan_block` → returned block → agent replies naturally.
- **Evening (21:00 WIB):** `tick` → `evening_review::run` → review block →
  `compose(REVIEW_SYSTEM)` → push ending with rollover question. User replies
  "iya" → agent → `rollover_todos` → "X todo digeser ke besok".

## Scheduling & Config (`proactive/tick.rs`)

- New env `EVENING_REVIEW_HOUR_WIB` (default `21`; `off` disables), parsed by the
  existing `parse_hour`.
- `ProactiveConfig` gains `evening_review_hour: Option<u32>`.
- `evening_review_due(now_wib, hour) -> Option<String>` analogous to
  `briefing_due`: due from the configured hour for `GRACE_HOURS`, dedup key
  `evening_review:YYYY-MM-DD`.
- `run_once` adds a claim-then-send block for the evening review, mirroring the
  briefing block.

## Confirmation Semantics

Rollover **writes** data, so it never fires automatically. The evening review only
*offers* it; the actual `rollover_todos` call happens after the user confirms in
chat. The SYSTEM prompt instructs the agent to call `rollover_todos` when the user
agrees to the review's offer (or asks to move todos), and to confirm what moved.

## Error Handling

- `compose` already degrades to sending the raw data block on any LLM failure —
  reused unchanged for plan/review.
- `rollover_todos` with no eligible candidates returns "nggak ada yang perlu
  digeser" rather than erroring.
- All proactive sends use the existing claim-before-send dedup (at-most-once).

## Testing

- `plan.rs`: ordering — high priority before normal/low; within a priority, earlier
  `due_at` first, NULL `due_at` last; estimate as final tiebreak.
- `tick.rs`: `evening_review_due` window (inside hour..hour+grace only; `off`
  disables; past grace forfeits) — mirroring the existing `briefing_due` tests.
- `todos::rollover`: shifts `due_at` by +1 day preserving time; leaves
  no-due/future todos untouched; returns the moved rows.
- Dispatcher: `plan_day` returns a non-empty block; `rollover_todos` honours
  explicit `ids` and the default-all-overdue path.
- Migration `0016` applies cleanly on top of `0015`.

## Open Coordination Item

When Fase 3–6 begin in parallel worktrees, assign each a migration-number range up
front (Fase 2 = `0016`) and re-check against `origin/main` before merging.
