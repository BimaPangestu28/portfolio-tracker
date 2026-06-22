# Planner Tree + Goals Integration — Design

**Date:** 2026-06-22
**Status:** Approved (design), pending implementation plan
**Branch:** `feat/planner-tree-goals`

## Problem

The current planner is a **flat** allocation model: each `category` carries one `target_pct`,
and every instrument links 1:1 to a category. Users cannot express:

1. **Hierarchical (sub-planner) targets** — e.g. "Saham 30% of total, and within Saham, BBCA
   40%, BBRI 30%". Targets are stuck at a single asset-class level.
2. **Goals tied to actual holdings** — a `goal` table exists, but progress can only come from
   `cash` (total liquid), `networth` (total net worth), or a `manual` number. There is no way to
   say "Reksadana 200jt for kids' education" and have it track the specific money put toward it.

This design adds a **free-depth allocation tree** and **transaction-tagged goals**, integrated on
the Planner page.

## Goals / Non-goals

**In scope**
- Arbitrary-depth allocation tree (`plan_node`) overlaid on the existing `category` catalog.
- Per-transaction goal tagging; one transaction → at most one goal; one instrument can feed many
  goals across its transactions.
- Goal progress shown both as **current market value** (primary) and **invested capital + P&L**
  (secondary), with an optional **target date**.

**Out of scope (future)**
- Hard enforcement of sibling target percentages summing to 100% (indicator only for now).
- FIFO/lot-accurate cost basis per goal (net-cash approximation used; see Trade-offs).
- Auto-suggesting a goal tag during statement ingestion (manual + assistant tagging only for now).
- Splitting a single transaction across multiple goals.

## Key decisions (from brainstorming)

| Decision | Choice |
|---|---|
| Scope | Hierarchy **and** goals, integrated |
| Goal ↔ holdings link | **Tag per transaction** (`txn.goal_id`); 1 instrument can span many goals |
| Goal progress basis | **Both** — market value (primary) + invested capital & P&L (secondary) |
| Allocation depth | **Free tree**, unbounded depth (adjacency list) |
| Node → instrument mapping | Node binds a **category OR instrument**; each branch has an auto **"Lainnya"** |
| Architecture | **Approach A — overlay**: keep `category`, add `plan_node` referencing it |
| Sibling % validation | **Indicator only**, no hard block |
| Goal target date | **Optional** field |

## Architecture — Approach A (overlay)

`plan_node` is a new adjacency-list tree that **references** the existing `category` / `instrument`
tables rather than replacing them. The `category` table stays as the asset-class catalog (still
used by ingestion auto-categorization, donut color mapping, and the DCA planner). Allocation
**targets** move from `category.target_pct` to `plan_node.target_pct`; `category.target_pct`
becomes deprecated (column retained, no longer read for targets).

Rationale: gives the free tree the user wants with a small blast radius and a safe migration, vs.
Approach B (replace `category` entirely) which would touch `instrument.category_id`, color mapping,
DCA, ingestion auto-assign, and all fixtures.

## Data model

### Migration `0030_plan_tree.sql`

```sql
CREATE TABLE plan_node (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_id          INTEGER REFERENCES plan_node(id) ON DELETE CASCADE,  -- NULL = root level
  name               TEXT NOT NULL,
  target_pct         TEXT NOT NULL,        -- % relative to parent (root: % of total portfolio)
  tolerance_band_pct TEXT,                 -- nullable, same semantics as category.tolerance_band_pct
  bind_kind          TEXT NOT NULL,        -- 'group' | 'category' | 'instrument'
  category_id        INTEGER REFERENCES category(id),    -- set iff bind_kind='category'
  instrument_id      INTEGER REFERENCES instrument(id),  -- set iff bind_kind='instrument'
  sort_order         INTEGER NOT NULL DEFAULT 0,
  color              TEXT
);
CREATE INDEX idx_plan_node_parent ON plan_node(parent_id);
```

**`bind_kind` semantics**
- `group` — pure aggregation node; value = sum of children; binds nothing.
- `category` — absorbs all instruments in `category_id` **not already claimed** by descendant
  instrument leaves; the unclaimed remainder surfaces as a synthetic **"Lainnya"** child
  (computed, not stored).
- `instrument` — leaf bound to one `instrument_id`; value = that instrument's IDR value.

**Data migration:** each existing `category` row → one root `plan_node`
(`bind_kind='category'`, `parent_id=NULL`, `target_pct`/`tolerance_band_pct`/`color`/`sort_order`
copied from the category). Zero data loss. `category.target_pct` retained but no longer read.

**Validation rules (repo layer)**
- `bind_kind='instrument'` requires `instrument_id` non-null and `category_id` null.
- `bind_kind='category'` requires `category_id` non-null and `instrument_id` null.
- `bind_kind='group'` requires both null.
- `target_pct` must parse as a valid decimal (reuse `repo::dec`).
- Reparenting must reject cycles (a node cannot become its own descendant).
- Sibling target sum is **not** enforced (indicator only in UI).

### Migration `0031_goal_tagging.sql`

```sql
ALTER TABLE txn ADD COLUMN goal_id INTEGER REFERENCES goal(id);
CREATE INDEX idx_txn_goal ON txn(goal_id);

ALTER TABLE goal ADD COLUMN target_date TEXT;  -- optional ISO date, e.g. '2035-06-01'
-- current_kind gains a new valid value: 'tagged' (progress computed from tagged txns)
```

`VALID_KINDS` in `repo/goals.rs` extends to `['cash','networth','manual','tagged']`.
Deleting a goal sets `txn.goal_id` back to NULL for its tagged transactions (handled in repo;
SQLite has no `ON DELETE SET NULL` here without FK pragma, so do it explicitly in the delete path).

## Computation logic

### Allocation tree rollup

Recursive generalization of the existing `compute_allocation` in `domain/allocation.rs`.

1. Compute each instrument's current IDR value (reuse existing position/valuation logic from
   `service/portfolio.rs`).
2. Compute node **actual value** bottom-up:
   - `instrument` leaf → that instrument's IDR value.
   - `category` node → total IDR of all instruments in the category. Descendant `instrument`
     leaves under it "claim" part of that total; the synthetic **"Lainnya"** child =
     `category_total − Σ(claimed instrument leaves)`.
   - `group` node → Σ children.
3. Compute **target value** top-down:
   - root: `target_value = target_pct% × total_portfolio_idr`.
   - child: `target_value = target_pct% × parent.target_value`.
4. Per node (relative to its parent): `actual_pct = node_value / parent_value`,
   `drift_pct = actual_pct − target_pct`, `out_of_band = |drift_pct| > tolerance_band_pct`,
   `rebalance_idr = target_value − actual_value`. Same fields as today's `CategoryAllocation`,
   emitted recursively as `PlanNodeAllocation`.

Instruments not represented anywhere in the tree roll into a root-level synthetic "Lainnya"
(consistent with today's `category_id = -1` bucket), which never flags `out_of_band`.

### Goal progress (`current_kind='tagged'`)

For a goal, gather its tagged transactions grouped by instrument:
- `net_unit(instrument) = Σ(buy qty) − Σ(sell qty)` over tagged txns.
- `market_value = Σ(net_unit × current_price_idr)` → **primary progress**.
- `invested_idr = Σ(buy cost incl. fee, in IDR) − Σ(sell proceeds in IDR)` over tagged txns
  → **secondary**.
- `gain_loss_idr = market_value − invested_idr`.
- `progress_pct = market_value / target_idr`.
- If `target_date` set: `months_left`, and `required_monthly_idr = max(0, target_idr − market_value) / months_left`.

Only **tagged** transactions affect a goal. An untagged sell of a tagged instrument does **not**
reduce the goal (the user must tag the sell to the goal to draw it down).

`current_kind` values `cash` / `networth` / `manual` keep their existing behavior unchanged.

## API

**Plan tree**
- `GET /plan/tree` — nested nodes with computed `PlanNodeAllocation` (actual value/pct, drift,
  rebalance, synthetic "Lainnya" children).
- `POST /plan/nodes` — create node (`name`, `parent_id`, `target_pct`, `tolerance_band_pct`,
  `bind_kind`, `category_id?`, `instrument_id?`, `sort_order?`, `color?`).
- `PATCH /plan/nodes/{id}` — update editable fields.
- `DELETE /plan/nodes/{id}` — cascade delete subtree.
- `POST /plan/nodes/{id}/move` — reparent and/or reorder (`parent_id`, `sort_order`); rejects cycles.

**Goals**
- `PATCH /goals/{id}` — **new** (update currently missing): label, note, target_idr, current_kind,
  current_manual_idr, target_date, sort_order.
- Extend `GoalResponse` with: `current_market_idr`, `invested_idr`, `gain_loss_idr`,
  `progress_pct`, `target_date`, `required_monthly_idr`, and a per-instrument `breakdown` array.
  Keep existing `current_idr` (equals `current_market_idr` for `tagged`).

**Transaction tagging**
- Extend the existing transaction edit path to set/clear `goal_id`.
- Add an assistant tool to tag a transaction to a goal via chat (consistent with the existing
  assistant instrument-management tools, PR #79).

## UI (PlannerPage)

Two sections / tabs:

**Alokasi (tree)**
- Expandable tree rows; inline-edit `target_pct`; "+ child" action (choose bind: category /
  instrument / group); per-node drift badge and rebalance hint; auto "Lainnya" rows; per-level
  "sibling target sum = X%" indicator (no block).
- Existing donut + drift bars stay, driven by the top level of the tree.

**Goals**
- Cards with a progress bar: market value primary; invested capital + gain/loss secondary.
- Optional target-date countdown and required monthly contribution.
- Breakdown of contributing instruments; create / edit (incl. `target_date`).

**Transaction tagging**
- Goal selector in the transaction edit form; also taggable via the assistant chat.

## Trade-offs / risks

- **Cost basis is net-cash, not FIFO.** `invested_idr` = tagged buy cost − tagged sell proceeds;
  `gain_loss_idr` is therefore an approximation when there are partial sells. Acceptable for a
  goal-progress view; documented so it isn't mistaken for realized-P&L accounting.
- **Two grouping concepts coexist** (`category` + `plan_node`). `plan_node` is the source of truth
  for targets; `category` remains for ingestion/color/DCA. Slight conceptual overlap, accepted to
  keep the migration safe.
- **Manual tagging burden.** Every contributing transaction must be tagged. Assistant tagging
  mitigates; ingestion auto-suggest is deferred.

## Phasing (for the implementation plan)

1. **Backend tree** — migration `0030`, category→root-node data migration, `plan_node` repo,
   recursive rollup compute, tree API.
2. **Tree UI** — Planner allocation tree (drill-down, inline edit, add/move/delete).
3. **Backend goal tagging** — migration `0031`, `txn.goal_id`, goal compute, `PATCH /goals`,
   extended `GoalResponse`.
4. **Goals UI + tagging** — goal cards, transaction goal selector, assistant tagging tool.
