# DCA Allocation Planner — Design Spec

**Date:** 2026-06-19
**Status:** Approved (brainstorming) → ready for implementation plan
**Author:** Bima + Claude

## 1. Summary

A **rebalancing-aware Dollar-Cost-Averaging (DCA) planner**. Given a recurring
contribution budget (e.g. Rp 55,000,000/month, anchored on the 12th) and a
frequency (`monthly` or `weekly`), the feature computes how much of that budget
to direct into each allocation **category** so the portfolio drifts toward its
diversification targets — using **new money only** (buy-only, never sell).

This first version is a **stateless calculator**: it always recomputes the
recommendation from the live portfolio state. Only the user's DCA *settings*
(budget, frequency, anchor day, rounding step) are persisted. Execution
tracking and reminders are explicitly deferred to a later phase, but the data
model is kept clean so they can be added without rework.

## 2. Goals & non-goals

### Goals
- Turn the existing per-category allocation targets into an actionable
  "what to buy this period" breakdown in IDR.
- Be rebalancing-aware: bias the budget toward under-allocated categories so the
  portfolio self-corrects over time without selling.
- Adapt to live state: weekly mode recomputes each week from the current actual
  allocation.
- Reuse existing allocation math (`compute_allocation()`), formatting, and UI
  primitives.

### Non-goals (this phase)
- No execution/realization tracking, no "mark as done", no link to transactions.
- No instrument-level drill-down (output stops at category level).
- No sell suggestions for over-allocated categories.
- No automated reminders/notifications.

## 3. Design decisions (locked during brainstorming)

| # | Decision | Choice |
|---|----------|--------|
| 1 | Output type | Rebalancing-aware plan (not plain proportional, not just a reminder) |
| 2 | Over-allocated handling | **Buy-only, starve** — over-target categories get Rp 0 in the rebalance phase |
| 3 | Budget overflow (budget > total gap) | **Two-phase** — fill gaps first, then distribute the remainder proportionally by target |
| 4 | Frequency model | One **monthly** budget input; `weekly` = budget ÷ N with **per-week recompute** (adaptive) |
| 5 | Architecture | **Stateless calculator**, settings persisted, designed to grow into a tracked plan later |
| 6 | Granularity | **Category-level** output only |
| 7 | Uncategorized ("Lainnya", target 0%) | Always Rp 0 (falls out of buy-only-starve automatically) |
| 8 | Tolerance band | Used as a **deadzone**: a category under target but inside its band is "good enough" and is not prioritized in the rebalance phase |
| 9 | Rounding | Default step **Rp 10,000**, residual assigned to the largest bucket so the total stays exactly equal to the budget; step is configurable in settings |

## 4. Algorithm

All amounts in IDR. Inputs per recompute:

- `categories` — current per-category allocation, from the existing
  `compute_allocation()`: each has `target_pct`, `tolerance_band_pct` (optional),
  `actual_value_idr`, `actual_pct`, `drift_pct`.
- `V` — current total portfolio value (`net_idr`).
- `B` — budget for **this period** (monthly budget, or the weekly slice).

Projected total after this contribution: **`T = V + B`**.

### Phase 1 — rebalance fill

For each category `i` with `target_pct_i > 0`:

- **Deadzone gate:** the category is a Phase-1 target only if it is under target
  beyond its tolerance band, i.e. `actual_pct_i < target_pct_i − band_i`.
  If `tolerance_band_pct` is null, treat `band_i = 0` (any under-target qualifies).
  Categories inside the band (or over target) have `shortfall_i = 0`.
- For qualifying categories:
  `shortfall_i = max(0, (target_pct_i / 100) × T − actual_value_idr_i)`.

Let `S = Σ shortfall_i`.

- If `S ≥ B`: distribute the whole budget proportional to the gaps —
  `alloc_i = B × shortfall_i / S`. Budget consumed; **stop** (mode = `rebalance`).
- If `S < B`: `alloc_i = shortfall_i` for each, and carry remainder
  `R = B − S` into Phase 2 (mode = `proportional` once R > 0; `mixed` if some
  gaps existed).

### Phase 2 — proportional top-up (only when budget exceeds all gaps)

Distribute `R` across all `target_pct > 0` categories by target weight, using a
fixed divisor of **100** (NOT `Σ target_pct`):

`alloc_i += R × target_pct_i / 100`

Reached only when the portfolio is essentially balanced (no under-target gaps
left), so this is ordinary proportional DCA. The divisor is literally `100`, not
`Σ target_pct`: when the user's targets sum to less than 100% (intentional
headroom), the unallocated slack `R × (100 − Σ target_pct) / 100` simply stays
as cash that period. (Normalizing by `Σ target_pct` would instead spread all of
`R` and leave nothing as cash — that would contradict the cash-leftover
behavior, which the `target_under_100_leaves_cash` test pins.)

**Important — the gate is on *current* percentage, the shortfall is on the
*projected* value.** A category qualifies for Phase 1 only if it is under target
on the **current** basis (`actual_pct_i < target_pct_i − band_i`, where
`actual_pct_i = value_i / V × 100`). Because `T = V + B > V`, every category's
target *value* @T exceeds its target value @V — so if we computed "shortfall"
purely on the T basis, even over-allocated categories would show a positive
shortfall. The current-% gate is what enforces "starve the over-allocated": an
over-% category is excluded before any shortfall is computed.

### Rounding (largest-remainder to `rounding_step`)

After computing raw `alloc_i` (which sum to the amount we intend to allocate —
equal to `B` when `Σ target_pct = 100`, less when the user leaves headroom):

1. For each category: `base_i = floor(alloc_i / step) × step`,
   `rem_i = alloc_i − base_i`.
2. Whole steps left over: `units = floor(Σ alloc_i / step) − Σ (base_i / step)`.
3. Hand out one `step` each to the `units` categories with the largest `rem_i`.
4. Final `alloc_i = base_i (+ step if it received one)`.

This guarantees every line is a multiple of `step`, the total never exceeds `B`,
and — when `Σ target_pct = 100` and `B` is a multiple of `step` — the lines sum
exactly to `B`. Any remainder (intentional headroom, or rounding crumbs) is
reported as unallocated cash in the plan note, never silently dropped.

### Emergent properties (no special-casing needed)

- **Over-allocated (by current %) → Rp 0** in Phase 1 (excluded by the gate).
- **"Lainnya" (target 0%) → Rp 0** always.
- **Within-band under-target → not rebalanced**, only ever receives Phase-2 money.
- **Perfectly balanced portfolio → pure proportional DCA.**

### Worked example

Targets: Crypto 40%, Saham ID 35%, Reksadana 25%. `V = 200,000,000`,
`B = 55,000,000` → `T = 255,000,000`.

| Category | target_pct | current | current % | gate | target value @T | shortfall |
|----------|-----------:|--------:|----------:|------|----------------:|----------:|
| Crypto    | 40% | 60,000,000 | 30% | under → in  | 102,000,000 | 42,000,000 |
| Saham ID  | 35% | 60,000,000 | 30% | under → in  |  89,250,000 | 29,250,000 |
| Reksadana | 25% | 80,000,000 | 40% | **over → starved** |  63,750,000 | 0 |

`S = 71,250,000 > B`, so the whole `B` is split proportional to the two
shortfalls (mode `rebalance`): Crypto `55M × 42 / 71.25 ≈ 32,420,000`,
Saham ID `55M × 29.25 / 71.25 ≈ 22,580,000`, Reksadana `0` (starved — it's
over-allocated). If `B` were larger than `S`, the gaps would be filled exactly
and the overflow spread by target weight (Phase 2).

## 5. Frequency model

- Settings hold one **monthly** budget, an **anchor day** (e.g. 12), and a
  **frequency**.
- `monthly`: period budget `B = monthly_budget`; computed once per cycle.
- `weekly`: period budget `B = monthly_budget / N` (N = 4 for v1, room to make it
  configurable later). The plan is **recomputed each week** from the current
  actual allocation, so price moves and any executed buys are reflected
  automatically.

The compute endpoint returns the period budget it used and the mode, so the UI
can label the slice ("Minggu ini") without owning the math.

## 6. Architecture

### Data model (new)

Single-row settings table (upsert):

```sql
CREATE TABLE dca_setting (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    monthly_budget TEXT    NOT NULL,           -- decimal string, IDR
    frequency      TEXT    NOT NULL DEFAULT 'monthly', -- 'monthly' | 'weekly'
    anchor_day     INTEGER NOT NULL DEFAULT 1,  -- day-of-month 1..28
    rounding_step  TEXT    NOT NULL DEFAULT '10000',
    updated_at     TEXT    NOT NULL
);
```

No plan/execution tables in this phase.

### Backend (Rust / Axum / SQLx)

- `backend/src/domain/dca.rs` — pure, fully unit-tested:
  - Types `DcaPlan`, `DcaCategoryLine`, `DcaMode { Rebalance, Mixed, Proportional }`.
  - `fn compute_dca_plan(categories: &[CategoryAllocation], total_idr: Decimal,
    budget: Decimal, rounding_step: Decimal) -> DcaPlan`.
- `backend/src/repo/dca_settings.rs` — `get()` (returns defaults if empty),
  `upsert(settings)`.
- `backend/src/api/dca.rs` — handlers:
  - `GET  /dca/settings` → current settings.
  - `PUT  /dca/settings` → upsert settings (validated).
  - `GET  /dca/plan` → compute plan using saved settings; optional query
    overrides `?budget=&frequency=` for what-if exploration without saving.
- Wire routes into `backend/src/api/mod.rs` (protected/JWT, same as other routes).

The plan handler reuses the same category-aggregation path as
`service/portfolio.rs::build_summary()` to obtain `CategoryAllocation` + total,
then calls `compute_dca_plan`.

### Frontend (React / TS / TanStack Query / Zod)

- Page `frontend/src/pages/DcaPlanPage.tsx`, route `/dca`.
  - Settings form: monthly budget, frequency (`monthly`/`weekly`), anchor day,
    rounding step. Uses `.form-stack` for mobile.
  - Period header: period label, period budget, active mode badge, note
    (e.g. "Portfolio sudah balance → mode proporsional").
  - Breakdown table per category: name + color, target %, current actual %,
    drift, **allocate IDR this period**, phase tag. Visual bar in the style of
    `DriftBars.tsx`. Uses `.lay-*` helpers.
- API layer: `DcaSettingsSchema`, `DcaPlanSchema`, `DcaCategoryLineSchema` in
  `frontend/src/api/schemas.ts`; hooks `useDcaSettings`, `useUpdateDcaSettings`,
  `useDcaPlan` in `frontend/src/api/hooks.ts`.
- Add `/dca` to the nav/router alongside `/planner` and `/budget`.

## 7. Error handling & edge cases

- **No categories / all target 0%:** plan returns empty lines with a note;
  budget unallocated. UI nudges user to set targets in the Planner.
- **`Σ target_pct < 100`:** normalizer is `Σ target_pct`; Phase-2 leftover stays
  as cash (surface it in the note).
- **`B = 0` or invalid budget:** validate on `PUT /dca/settings` (positive
  decimal); reject with 400.
- **`anchor_day` out of range:** clamp/validate to 1..28.
- **Decimal precision:** use the same `Decimal` type as the rest of the backend;
  no floats in money math. No `unwrap()`/`panic!()` on parse — propagate errors.
- **Rounding residual:** always reassigned so displayed total == budget.

## 8. Testing

Rust unit tests for `compute_dca_plan` covering:

1. All categories under target → proportional-to-gap split.
2. Mixed over/under → over-allocated get Rp 0.
3. Perfectly balanced → pure proportional (Phase 2).
4. Uncategorized / target 0% → Rp 0.
5. `B` < total gap → proportional tilt toward the largest gaps.
6. `B` > total gap → gaps filled, remainder spread by target weight.
7. Tolerance deadzone → within-band under-target excluded from Phase 1.
8. `Σ target_pct < 100` → leftover cash, normalizer correct.
9. Rounding → rounded lines sum exactly to `B`.

Per backend convention (no rustfmt): run `cargo clippy` + `cargo test`.

## 9. Future phases (out of scope now)

- Tracked plans with per-slice `pending/done` status, linked to transactions for
  plan-vs-actual reporting (the stateful "B" option).
- Reminders on the anchor day (reuse the existing reminders subsystem).
- Instrument-level drill-down within a category.
- Sell suggestions for severely over-allocated (out-of-band) categories.
- Configurable weekly slice count / calendar-accurate week counting.
