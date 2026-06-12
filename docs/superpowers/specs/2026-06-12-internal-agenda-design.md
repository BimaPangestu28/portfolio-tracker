# Personal Assistant — Phase 3: Internal Agenda

**Date:** 2026-06-12
**Status:** Approved
**Note:** Built after Phase 4 (the roadmap order was deliberately swapped).
This completes the four-phase assistant roadmap from
`2026-06-11-assistant-phase1-todos-reminders-design.md`.

## Context

The assistant manages todos, reminders, long-term memory, and proactive
briefings. This phase adds the last roadmap item: an internal agenda —
events created and queried via Telegram chat ("meeting vendor besok jam 2 di
kantor", "besok ada apa?"), with automatic pre-event reminders and agenda
sections in the morning briefing and weekly recap.

Decisions made during brainstorming:

- **Simple event model:** title + start time + optional location/notes. No
  recurrence (recurring reminders already exist), no end time, no conflict
  detection — YAGNI.
- **Automatic pre-event reminder, default 30 minutes**, overridable per
  event ("ingetin 1 jam sebelum"), suppressible with 0.
- **Approach A — materialized reminders:** `create_event` also creates a row
  in the existing `reminders` table, linked via a new nullable
  `reminders.event_id` column. Cancelling the event cancels its pending
  reminder. Chosen because the proven 60-second reminder loop delivers
  **at-least-once** (a missed meeting reminder is unacceptable — note the
  contrast with briefings' at-most-once). Rejected: events-as-reminders
  (conflates calendar entries with notifications); pre-reminders computed by
  the proactive tick (claim-before-send is at-most-once, and 5-minute
  granularity is coarser than the reminder loop).
- No Google Calendar (decided in Phase 1; internal only).

## Data Model

Migration `0013_events.sql`:

```sql
CREATE TABLE events (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  title TEXT NOT NULL,
  location TEXT,
  notes TEXT,
  start_at TEXT NOT NULL,        -- UTC, Z format (same as reminders.remind_at)
  status TEXT NOT NULL DEFAULT 'scheduled'
    CHECK (status IN ('scheduled', 'cancelled')),
  created_at TEXT NOT NULL
);
ALTER TABLE reminders ADD COLUMN event_id INTEGER REFERENCES events(id);
```

New repo `repo/events.rs` (same pattern as todos): `create`, `get`, `cancel`
(scheduled only, returns bool), `list_between(db, from_z, to_z)` — the range
query behind "besok ada apa?", briefing, and recap. `repo/reminders.rs`
gains an optional `event_id` on `create` and `cancel_by_event(db, event_id)`.

## Tools (agent tool count: 12)

| Tool | Input | Behavior |
|---|---|---|
| `create_event` | `{ title, start_at (RFC3339 +07:00), location?, notes?, remind_minutes_before? }` | Default reminder 30 minutes before; creates the event plus a linked reminder ("📅 {title}{ at location} — {n} menit lagi"); the reminder is skipped when its time is already past; `remind_minutes_before: 0` means no reminder. `start_at` must be in the future. |
| `list_events` | `{ from?, to? }` | Default: the next 7 days. Renders WIB times. The model derives ranges ("besok") from the current datetime in its system prompt. |
| `cancel_event` | `{ id }` | Cancels the event AND its pending linked reminder. |

Dispatcher validation follows the existing pattern (parse via
`parse_tool_datetime`, errors become model-visible feedback). The agent
SYSTEM prompt gains one sentence describing the agenda capability.

## Integration

- **Morning briefing:** `BriefingData` gains `events_today` (via
  `list_between` over today's WIB range), rendered as an "Agenda hari ini:"
  section with `(tidak ada)` when empty.
- **Weekly recap:** `RecapData`'s next-week section gains
  `events_next_week` (next 7 days), rendered alongside todos and reminders.
- **Delivery:** no new send path — event reminders flow through the existing
  60-second reminder loop (at-least-once, automatic retry). Their `todo_id`
  is NULL, so the notification correctly has no "✅ Selesai" button.

## Failure Modes

No new ones. Events are a local SQLite table (failures propagate like
todos); briefing/recap already degrade per-source.

## Testing

- Repo: event round-trip, idempotent cancel, `list_between` range bounds.
- Dispatcher: past `start_at` rejected, default 30-minute reminder created
  and linked, `remind_minutes_before: 0` creates no reminder, event whose
  start is sooner than the offset creates no reminder, `cancel_event`
  cancels the linked reminder.
- Rendering: briefing/recap blocks with and without events; event reminder
  message format.

## Out of Scope (this phase)

- Recurring events, end times / conflict detection, event editing (cancel +
  recreate instead), Google Calendar sync, web calendar UI.
