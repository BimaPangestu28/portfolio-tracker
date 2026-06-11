# Personal Assistant — Phase 1: Tool-Use Agent Foundation + Todos & Reminders

**Date:** 2026-06-11
**Status:** Approved

## Context

The portfolio tracker already has assistant-shaped infrastructure: an LLM chat
service with conversation history, a Telegram bot with inline confirm buttons
and photo ingestion, a WhatsApp gateway, and a background scheduler. This
project expands it from a finance assistant into a general personal assistant.

Agreed capability roadmap (each phase gets its own spec → plan → implementation
cycle):

1. **Phase 1 (this spec):** tool-use agent foundation + todos & reminders,
   including on-time reminder delivery via Telegram.
2. **Phase 2:** notes & knowledge (save and recall via keyword/filter; schema
   designed so embeddings can be added later).
3. **Phase 3:** internal agenda/calendar (events stored in own DB, no Google
   Calendar integration for now).
4. **Phase 4:** proactive messaging — morning briefing, on-the-spot financial
   alerts, weekly recap.

Decisions made during brainstorming:

- **Primary channel:** Telegram (most mature integration). Web and WhatsApp
  may become entry points later.
- **Architecture:** a single tool-use agent (Approach A) rather than
  intent-classification routing. Each new capability adds tools instead of
  prompt branches, and multi-intent messages are handled naturally.
- **Calendar:** internal-first; no Google OAuth.
- **Notes recall (Phase 2):** LLM + keyword/filter, no vector search.

## Phase 1 Goals

- Upgrade the Claude client to support the tool-use API with an execution loop.
- Create, list, and complete todos via natural-language Telegram chat.
- Create, list, and cancel reminders (one-shot and recurring) via chat.
- Deliver reminders to the linked Telegram chat at the right time.
- Keep all existing flows (portfolio Q&A, photo ingest, fund-entry confirm
  buttons) working.

## Architecture

### Tool-use agent loop (`llm/`)

`llm/claude.rs` gains `complete_with_tools()`: it sends the conversation plus
tool definitions to the Claude API. While the response contains `tool_use`
blocks, the backend executes each tool via the dispatcher, appends a
`tool_result` block, and calls the API again — until the model produces a
final text answer. The loop is capped at **5 iterations** as a cost/runaway
guard; on hitting the cap the bot replies with an apology message instead of
hanging.

### Tool dispatcher (`assistant/` module)

A single `match` on tool name that calls the appropriate service. Phase 1
tools:

| Tool | Action |
|---|---|
| `create_todo` | Create a todo (title, optional notes, optional due date) |
| `list_todos` | List open (and optionally done) todos |
| `complete_todo` | Mark a todo done |
| `create_reminder` | Create a reminder at a given time, optionally recurring |
| `list_reminders` | List pending reminders |
| `cancel_reminder` | Cancel a pending reminder |
| `get_portfolio_summary` | Wraps the existing portfolio context builder so finance Q&A keeps working inside the same agent |

### Confirmation policy

Unlike the fund-entry flow (which keeps its ✅/❌ inline-button confirmation
because it touches financial data), todo/reminder actions **execute
immediately without confirmation** — they are low-stakes and easily reversed
("batalin reminder tadi"). The model's reply summarizes what was created, so
mistakes are immediately visible.

### Time and language handling

The system prompt includes the current datetime in `Asia/Jakarta`. The model
must emit ISO datetimes in tool arguments — natural-language phrases like
"besok jam 9" are translated by the model, not by a hand-written date parser.
Timestamps are stored in UTC and rendered in `Asia/Jakarta` for display.

## Data Model

New migration `0010_assistant.sql` with two tables.

**`todos`**

| Column | Notes |
|---|---|
| `id` | PK autoincrement |
| `title` | Todo title |
| `notes` | Extra detail, nullable |
| `due_at` | ISO deadline, nullable |
| `status` | `open` / `done` |
| `created_at` | Timestamp |
| `completed_at` | Timestamp, nullable |

**`reminders`**

| Column | Notes |
|---|---|
| `id` | PK autoincrement |
| `todo_id` | FK to `todos`, nullable — a reminder can stand alone or attach to a todo |
| `message` | Text sent when the reminder fires |
| `remind_at` | Next delivery time (UTC in DB) |
| `recurrence` | `none` / `daily` / `weekly` / `monthly` — three patterns only, no full cron (YAGNI) |
| `status` | `pending` / `sent` / `cancelled` |
| `sent_at` | Last delivery timestamp, nullable |

Recurring reminders: after delivery, `remind_at` advances to the next
occurrence and status stays `pending`; one-shot reminders are marked `sent`.

New repos `repo/todos.rs` and `repo/reminders.rs` follow the existing repo
pattern.

## Scheduler & Reminder Delivery

The existing scheduler loop (price refresh, long interval) is untouched. A
**second tick loop** runs every 60 seconds:

1. Query `reminders` where `status = 'pending'` and `remind_at <= now`.
2. Send `message` to the linked Telegram chat (via the existing
   `telegram_link`). If the reminder is attached to a todo, the message
   includes an inline **"✅ Selesai"** button that marks the todo done
   directly from the notification.
3. Mark `sent` and set `sent_at`; for recurring reminders, advance
   `remind_at` to the next occurrence.

**Resilience:** if the Telegram send fails, the reminder stays `pending` and
is retried on the next tick — slightly late beats lost. If the backend was
down when a reminder came due, it is delivered on startup because the query
uses `remind_at <= now`, not an exact match.

## Telegram Integration

Free-text messages currently routed to the portfolio Q&A `answer()` are
redirected to the new tool-use agent. Since the agent has
`get_portfolio_summary`, the old capability is preserved. Existing special
flows (photo ingest, fund entry, ✅/❌ review callbacks) are untouched. The
"✅ Selesai" reminder button adds a new callback variant alongside the
existing `confirm:`/`reject:` parsing.

## Error Handling

- Tool arguments are validated in the dispatcher (valid ISO datetime,
  `remind_at` not in the past, referenced todo/reminder exists). Invalid
  arguments produce an error `tool_result` so the model can self-correct or
  ask the user.
- Agent loop capped at 5 iterations; cap hit → apologetic reply.
- Telegram delivery failures leave reminders `pending` for automatic retry.

## Testing

Following the existing unit-test patterns:

- Agent loop with mocked API responses (`tool_use` → `tool_result` → final
  text), including the iteration cap.
- Dispatcher argument validation (bad datetimes, past `remind_at`, missing
  ids).
- Due-reminder query behavior (due now, not yet due, already sent/cancelled).
- Next-`remind_at` computation for each recurrence pattern.
- Callback parsing for the new "Selesai" button alongside existing
  confirm/reject.

## Out of Scope (Phase 1)

- Web UI for todos/reminders (Telegram-first; REST endpoints can come with a
  later frontend phase).
- Notes & knowledge, agenda/calendar, proactive briefings/recaps — Phases 2-4.
- WhatsApp as an agent entry point.
- Full cron-style recurrence rules.
