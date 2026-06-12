# Personal Assistant — Phase 4: Proactive Briefings, Alerts & Weekly Recap

**Date:** 2026-06-12
**Status:** Approved
**Note:** Phase 3 (internal agenda) is deliberately skipped for now; this phase
was prioritized because all of its inputs (todos, reminders, portfolio,
memory graph) already exist. The briefing gains an agenda section if/when
Phase 3 lands.

## Context

Phases 1-2 gave the Telegram assistant todos, reminders, a tool-use agent,
and Graphiti long-term memory. This phase makes it proactive: a daily morning
briefing, event-driven financial alerts, and a weekly recap — all delivered
to the linked Telegram chat without being asked.

Decisions made during brainstorming:

- **Morning briefing** at 07:00 WIB daily: today's todos/reminders, portfolio
  snapshot (net worth + overnight movers + pending reviews), and relevant
  facts from the memory graph.
- **Financial alerts**, event-driven: big daily movers (default ±5%), review
  items pending > 24h, stale prices, net-worth milestones (default steps of
  Rp 50.000.000).
- **Weekly recap** Sunday 17:00 WIB: productivity (todos done vs created,
  reminders delivered), weekly finance (net worth delta, top movers,
  cashflow), and a look at next week.
- **Composition:** deterministic data gathering + one Claude call to write
  the briefing/recap as natural Indonesian prose. Alerts are short templates,
  no LLM.
- **Scheduling (Approach A):** a 5-minute tick loop (same pattern as the
  proven `reminder_tick`) + a `proactive_log` table for restart-safe dedup
  and explicit catch-up semantics. No new dependencies; cron crates rejected
  (no built-in catch-up, dedup table needed anyway). Reusing the reminders
  table was rejected (briefings need content generated at send time).
- **LLM-failure fallback:** send the plain data block rather than skipping —
  an ugly briefing beats a missing one.

## Architecture

All in the Rust backend — no new services or dependencies:

```
assistant/proactive/
├── mod.rs        — module root
├── tick.rs       — 5-minute loop: check due schedules → run job → log
├── briefing.rs   — gather morning data → LLM compose → send
├── recap.rs      — gather weekly data → LLM compose → send
├── alerts.rs     — evaluate 4 triggers from stored data → send new ones
└── compose.rs    — shared LLM call + plain-list fallback
```

- `tick.rs` is spawned from `main.rs` (like `reminder_tick`). Each tick:
  briefing due? recap due? then evaluate alerts.
- **Separation:** data gathering is 100% deterministic and separate from
  composition (testable without an LLM); composition is separate from
  delivery.
- **Reused dependencies:** `service::portfolio::build_summary`,
  `service::movers`, `repo::{todos, reminders, review_items, snapshots,
  cashflow}`, `assistant::memory` (degrading `search`), `llm::claude`
  (`complete`, one call per briefing/recap), `TelegramClient` +
  `telegram_link`.
- **Gate:** proactive features run only when `TELEGRAM_BOT_TOKEN` is set AND
  a chat is linked — same as reminders.

## Data Model & Scheduling Semantics

Migration `0012_proactive.sql`:

```sql
CREATE TABLE proactive_log (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  kind TEXT NOT NULL,             -- 'briefing' | 'recap' | 'alert'
  dedup_key TEXT NOT NULL UNIQUE, -- idempotency key
  sent_at TEXT NOT NULL           -- UTC, Z format
);
```

Dedup keys (idempotent across restarts — INSERT first, send after; a
conflict on insert means already handled):

| Job | dedup_key | Meaning |
|---|---|---|
| Briefing | `briefing:2026-06-13` | Once per WIB date |
| Recap | `recap:2026-W24` | Once per ISO week |
| Mover | `mover:BBCA:2026-06-13` | One alert per instrument per day |
| Review backlog | `review-backlog:2026-06-13` | Once per day while a backlog exists |
| Stale price | `stale:BTC:2026-06-13` | Per instrument per day |
| Milestone | `milestone:1550000000` | Once per milestone value, ever |

Timing & catch-up:

- **Briefing due:** WIB time ≥ 07:00 AND no `briefing:<today WIB>` row.
  Grace window until 12:00 WIB; past that the day is forfeited (the date is
  still logged so it can't fire in the afternoon).
- **Recap due:** Sunday, ≥ 17:00 WIB, no row for this ISO week; grace until
  Monday 09:00 WIB (a Monday-morning recap is still useful).
- **Write order:** insert log row first, then send. A crash between the two
  loses that day's briefing — **at-most-once on purpose**, the inverse of
  reminders (at-least-once): a duplicate briefing annoys more than a missing
  one, and nothing is lost (the data can always be asked for in chat).

Config via env (defaults in code): `BRIEFING_HOUR_WIB=7`,
`RECAP_HOUR_WIB=17`, `MOVER_ALERT_PCT=5`, `MILESTONE_STEP_IDR=50000000`.
Setting `BRIEFING_HOUR_WIB=off` (or `RECAP_HOUR_WIB=off`) disables that job.

## Message Content & Composition

### Morning briefing (`briefing.rs` → `BriefingData`)

| Source | Content |
|---|---|
| `repo::todos` | Open todos due today (WIB) + overdue ones |
| `repo::reminders` | Pending reminders firing today |
| `service::portfolio` + `service::movers` | Net worth + delta vs yesterday's snapshot; overnight top movers; pending review count |
| `assistant::memory` | `search("hal penting hari ini <weekday>, <WIB date>")`, limit 5 — catches facts like "gajian tanggal 25" |

### Weekly recap (`recap.rs` → `RecapData`)

Todos completed vs created in the last 7 days (via `completed_at` /
`created_at`), reminders delivered; net worth start-of-week vs now (from
`snapshots`), weekly top movers, this week's cashflow/spending; next week's
due todos and reminders.

### Alerts (`alerts.rs`, no LLM, short templates)

- **Mover:** position with |daily price Δ| ≥ `MOVER_ALERT_PCT`% →
  "📈 BBCA +6,2% hari ini (Rp ...)"
- **Review backlog:** pending `review_items` older than 24h → one concise
  message
- **Stale price:** instrument whose price hasn't updated in > 24h and whose
  source isn't `manual`
- **Milestone:** today's net worth crossed a multiple of
  `MILESTONE_STEP_IDR` never logged before → "🎉 Net worth melewati Rp 1,55 M"

### Composition (`compose.rs`, briefing & recap only)

- One `ClaudeClient::complete` call. System prompt: Indonesian, plain-text
  messenger rules (same as the agent), concise (~10-15 lines), numbers MUST
  be copied exactly from the provided data, memory facts woven in naturally
  when relevant, one short closing encouragement (not over the top).
- Input: a structured data block from a deterministic `render_data_block`
  function (unit-testable).
- **Fallback:** LLM failure/timeout → send the plain data block itself with
  the header "📋 Briefing (mode ringkas)".

## Failure Modes

| Failure | Behavior |
|---|---|
| LLM compose fails | Plain-list fallback — the briefing still arrives |
| Telegram send fails | Warn log; the log row already exists so that day is forfeited (at-most-once, deliberate) |
| Memory service down | Briefing runs without the facts section (degrading `search`) |
| Partial data (e.g. movers errors) | Failed section skipped with a log note; briefing sent with what's available |
| Backend down past the grace window | That day's briefing is absent — by design |

## Testing

- **Pure:** due-window logic (WIB hour vs grace, ISO-week recap), dedup-key
  construction, milestone-crossing detection (yesterday 1.49M → today 1.56M
  yields milestones 1.50 and 1.55), mover threshold, `render_data_block` for
  briefing and recap.
- **Repo:** `proactive_log` insert / dedup conflict.
- **Loop & alert evaluator with mem-db:** seed data → evaluate → assert
  produced messages + log rows (Telegram delivery stubbed by separating
  "evaluate → message list" from "send").
- LLM compose not tested live (the fallback path is tested).

## Out of Scope (this phase)

- Adjusting schedules via chat ("ubah briefing ke jam 8") — env only.
- Alerts via web/push notification.
- Agenda/calendar section in the briefing (awaits Phase 3).
- Granular per-alert on/off configuration.
