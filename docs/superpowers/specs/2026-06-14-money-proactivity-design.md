# Money Proactivity — Design

**Date:** 2026-06-14
**Status:** Approved (design), pending implementation plan
**Phase:** Productivity roadmap — Fase 6 (final)

## Overview

Surface money intelligence the backend already computes but the assistant can't
reach, and add user-defined price alerts. Four pieces: a combined cashflow
summary in chat, portfolio insights in chat, a monthly recap, and per-instrument
price alerts.

## Goals

- **Cashflow gabungan (chat):** "bulan ini masuk berapa, kepake berapa, net
  berapa?" — month income/expense/net + top categories, plus freelance invoiced
  this month (shown separately).
- **Portfolio insights (chat):** savings rate, top-position concentration, and
  dividend yield (best-effort) on demand.
- **Monthly recap:** a proactive month-in-review push on the 1st.
- **Price alerts:** "kabarin kalau BBCA turun 5%" → fire once when the price
  crosses the target.

## Non-Goals (YAGNI for v1)

- No dividend tracking/reminders (only best-effort dividend yield in insights).
- No recurring/repeating price alerts (fire once, then done).
- Runway is included only if a liquid-category convention is cleanly available;
  otherwise deferred (noted, not blocking).

## Constraints

- **Migration `0020_price_alerts`** — on `main` the latest is `0019_inbox`, so
  `0020` is next free.
  > **Cross-branch coordination:** open PR #54 (Fase 2) still carries the stale
  > `0017` (collides with `main`'s `0017_invoices`) and must renumber to `0021`
  > before merge. PR #56 (Fase 3) has no migration.
- Reuses existing services: `service/cashflow::month_summary`, `service/insights`,
  `service/portfolio::build_summary`, `repo/prices::latest`, `repo/invoices`.

## Components

### 1. Cashflow summary tool (`tools.rs` + `dispatcher.rs`)

`cashflow_summary { month?: "YYYY-MM" }` (default current WIB month):
- Load `cashflow::list_for_month(month)` + categories; run
  `service/cashflow::month_summary` → income, expense, net, top category lines.
- Sum `invoices` with `issue_date` in `month` → "freelance diinvoice" total,
  rendered as a **separate** line (not added to cashflow income — invoices may or
  may not also be recorded as cashflow; keep them distinct to avoid double-count).
- Render: "Bulan {month}: masuk Rp X, kepake Rp Y, net Rp Z" + top categories +
  "Freelance diinvoice: Rp W".

### 2. Portfolio insights tool (`tools.rs` + `dispatcher.rs`)

`portfolio_insights {}`:
- `build_summary` → net worth, positions, allocation.
- Savings rate: from the current month's `month_summary` income/expense via
  `insights::savings_rate`.
- Concentration: `insights::concentration(positions→(symbol, value), net_worth)`.
- Dividend yield (best-effort): `insights::dividend_ttm` over cashflow rows in a
  dividend-named category (skip the line if none found).
- Runway: included via `insights::runway_months` only if liquid can be computed
  from a known cash-category convention; otherwise omitted in v1.
- Render a few plain lines; omit any line whose inputs are unavailable.

### 3. Monthly recap (`proactive/monthly_recap.rs` + `tick.rs` + `compose.rs`)

Mirrors `recap.rs` (weekly) but monthly:
- `gather(db, now_utc)` → prior-month: todos done, finances (net-worth change
  month-over-month from snapshots), spending (month_summary), freelance invoiced.
- `MONTHLY_RECAP_SYSTEM` prompt in `compose.rs` (extend the prompt-invariant test).
- `monthly_recap_due(now_wib, hour)` in `tick.rs`: due on day 1 from the hour for
  `GRACE_HOURS`; dedup key `monthly_recap:YYYY-MM` (the month that just ended).
  New env `MONTHLY_RECAP_HOUR_WIB` (default 8). `ProactiveConfig` +
  `run_once` claim-then-send block.

### 4. Price alerts

Migration `0020_price_alerts.sql`:
```sql
CREATE TABLE price_alerts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  instrument_id INTEGER NOT NULL REFERENCES instruments(id),
  target_price TEXT NOT NULL,                 -- decimal string
  direction TEXT NOT NULL CHECK (direction IN ('above', 'below')),
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'triggered', 'cancelled')),
  created_at TEXT NOT NULL,
  triggered_at TEXT
);
CREATE INDEX idx_price_alerts_active ON price_alerts (status, instrument_id);
```

Repo `repo/price_alerts.rs`: `create`, `list_active`, `list_active_for_chat`
(joined with instrument symbol), `mark_triggered(id)`, `cancel(id)`.

**Semantics:** storage is an **absolute target price + direction**. The agent
converts "% turun/naik" to a level at set-time from the current price.

Tools (`tools.rs` + `dispatcher.rs`):
- `set_price_alert { instrument: string, target?: number, percent?: number, direction?: "above"|"below" }`
  — resolve instrument (by symbol/name via `instruments`); if `target` given use
  it; if `percent` given, read current price (`prices::latest`) and compute
  `target = current × (1 ± percent/100)` with direction inferred from the sign /
  the `direction` arg; require a current price when using percent. Store.
- `list_price_alerts {}` — active alerts (symbol, direction, target, current).
- `cancel_price_alert { id }`.

Tick evaluation (`proactive/alerts.rs` or `tick.rs`): for each active alert, read
`prices::latest(instrument_id)`; if `below` and price ≤ target, or `above` and
price ≥ target → emit an `Alert` (dedup `price_alert:{id}`), `mark_triggered`.
Wire into the existing `evaluate`/`run_once` alert loop.

### Prompt (`agent.rs`)

Guidance: "bulan ini masuk/kepake/net" → `cashflow_summary`; insights questions →
`portfolio_insights`; "kabarin kalau <instrumen> turun/naik X% / di harga Y" →
`set_price_alert` (convert % from current price), `list_price_alerts`,
`cancel_price_alert`.

## Error Handling

- `cashflow_summary`/`portfolio_insights` degrade per-source (a missing finance
  source omits its line, like the briefing does) and never panic.
- `set_price_alert` with no resolvable instrument → ask the user; percent without
  a current price → explain price unavailable.
- Monthly recap uses the existing compose-with-fallback path.
- Price-alert eval failures are logged per-alert and skipped (one bad alert never
  blocks the loop), matching the existing alert loop.

## Testing

- `service`/pure: month_summary already tested; add cashflow tool formatting,
  insights tool rendering (omits unavailable lines).
- `price_alerts` repo: create → list_active; mark_triggered/cancel drop from
  active; the cross-direction trigger predicate (pure fn) tested for above/below.
- Dispatcher: `cashflow_summary`, `portfolio_insights`, `set_price_alert`
  (target + percent paths, instrument resolution), `list`/`cancel`.
- `tick.rs`: `monthly_recap_due` window (day-1 only, grace, off); price-alert
  trigger evaluation.
- `compose.rs`: `MONTHLY_RECAP_SYSTEM` invariants.
- Tool registration test updated with the new names.

## Open Coordination Item

Migration `0020`. Fase 2 PR #54 → `0021` before merge. `agent.rs`/`tools.rs`/
`tick.rs`/`compose.rs` are shared append points across roadmap branches — expect
trivial conflicts if merged out of order.
