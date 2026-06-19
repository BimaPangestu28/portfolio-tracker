# DCA Allocation Planner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a rebalancing-aware, buy-only DCA calculator that splits a recurring contribution budget across allocation categories toward their diversification targets, exposed as a `/dca` page.

**Architecture:** A pure Rust domain function (`compute_dca_plan`) does all the money math over the existing `CategoryInput` type; a single-row `dca_setting` table persists budget/frequency/anchor-day/rounding; two API endpoints (`GET/PATCH /dca/settings`, `GET /dca/plan`) reuse `build_summary`'s category aggregation; a React page renders the per-category breakdown. Stateless — nothing about executed buys is stored.

**Tech Stack:** Rust (Axum, SQLx/SQLite, `rust_decimal`), React + TypeScript (TanStack Query, Zod), raw-CSS UI convention (`.card`, `.lay-*`, `.field`, `.btn`).

## Global Constraints

- **Backend money math uses `rust_decimal::Decimal`** — never `f64`. Parse TEXT columns with `crate::repo::dec(s)`. Serialize with `#[serde(with = "rust_decimal::serde::str")]` (required) / `str_option` (optional).
- **No `unwrap()` / `panic!()` / `expect()` in non-test code.** Handlers return `Result<Json<T>, AppError>`; map repo errors with `AppError::Other`, validation with `AppError::BadRequest`.
- **No rustfmt.** Verify with `cargo clippy` + `cargo test` (run from `backend/`). CI has no formatting gate.
- **TEXT decimals in SQLite.** Money/percent columns are `TEXT NOT NULL`; timestamps are ISO-8601 `TEXT`.
- **Frontend money/percent fields are `z.string()`** — coerce with `parseNum()` / `Number()`, format with `formatIDR` / `formatPct`.
- **Conventional commits** (`feat:`, `test:`, `docs:`). Frequent commits, one per task.
- Work on branch `feat/dca-allocation-planner` (already created).

---

### Task 1: `dca_setting` table + repo

**Files:**
- Create: `backend/migrations/0028_dca_setting.sql`
- Create: `backend/src/repo/dca_settings.rs`
- Modify: `backend/src/repo/mod.rs` (add `pub mod dca_settings;`)

**Interfaces:**
- Produces: `dca_settings::DcaSettingRow { id: i64, monthly_budget: String, frequency: String, anchor_day: i64, rounding_step: String, updated_at: String }`; `dca_settings::SaveDcaSetting { monthly_budget: String, frequency: String, anchor_day: i64, rounding_step: String }`; `async fn get(db: &Db) -> anyhow::Result<DcaSettingRow>` (returns defaults if no row); `async fn upsert(db: &Db, s: &SaveDcaSetting) -> anyhow::Result<DcaSettingRow>`.

- [ ] **Step 1: Write the migration**

Create `backend/migrations/0028_dca_setting.sql`:

```sql
-- DCA planner settings: a single persisted row (id = 1).
CREATE TABLE IF NOT EXISTS dca_setting (
    id             INTEGER PRIMARY KEY CHECK (id = 1),
    monthly_budget TEXT    NOT NULL DEFAULT '0',
    frequency      TEXT    NOT NULL DEFAULT 'monthly' CHECK (frequency IN ('monthly', 'weekly')),
    anchor_day     INTEGER NOT NULL DEFAULT 1,
    rounding_step  TEXT    NOT NULL DEFAULT '10000',
    updated_at     TEXT    NOT NULL
);
```

- [ ] **Step 2: Register the repo module**

In `backend/src/repo/mod.rs`, add alongside the other `pub mod` lines:

```rust
pub mod dca_settings;
```

- [ ] **Step 3: Write the failing repo test**

Create `backend/src/repo/dca_settings.rs` with only the test module first (so it fails to compile/run):

```rust
use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DcaSettingRow {
    pub id: i64,
    pub monthly_budget: String,
    pub frequency: String,
    pub anchor_day: i64,
    pub rounding_step: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SaveDcaSetting {
    pub monthly_budget: String,
    pub frequency: String,
    pub anchor_day: i64,
    pub rounding_step: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn get_returns_defaults_when_empty() {
        let db = mem_db().await;
        let row = get(&db).await.unwrap();
        assert_eq!(row.monthly_budget, "0");
        assert_eq!(row.frequency, "monthly");
        assert_eq!(row.anchor_day, 1);
        assert_eq!(row.rounding_step, "10000");
    }

    #[tokio::test]
    async fn upsert_then_get_roundtrips_and_is_singleton() {
        let db = mem_db().await;
        upsert(&db, &SaveDcaSetting {
            monthly_budget: "55000000".into(),
            frequency: "weekly".into(),
            anchor_day: 12,
            rounding_step: "10000".into(),
        }).await.unwrap();
        // second upsert must update the same row, not insert a new one
        let row = upsert(&db, &SaveDcaSetting {
            monthly_budget: "60000000".into(),
            frequency: "monthly".into(),
            anchor_day: 1,
            rounding_step: "100000".into(),
        }).await.unwrap();
        assert_eq!(row.id, 1);
        assert_eq!(row.monthly_budget, "60000000");
        let again = get(&db).await.unwrap();
        assert_eq!(again.frequency, "monthly");
        assert_eq!(again.rounding_step, "100000");
    }
}
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `cd backend && cargo test repo::dca_settings`
Expected: FAIL — `get`/`upsert` not found (and `connect` runs migrations including 0028).

- [ ] **Step 5: Implement `get` and `upsert`**

Add to `backend/src/repo/dca_settings.rs` (above the test module):

```rust
pub async fn get(db: &Db) -> anyhow::Result<DcaSettingRow> {
    if let Some(row) = sqlx::query_as::<_, DcaSettingRow>("SELECT * FROM dca_setting WHERE id = 1")
        .fetch_optional(db)
        .await?
    {
        return Ok(row);
    }
    Ok(DcaSettingRow {
        id: 1,
        monthly_budget: "0".to_string(),
        frequency: "monthly".to_string(),
        anchor_day: 1,
        rounding_step: "10000".to_string(),
        updated_at: String::new(),
    })
}

pub async fn upsert(db: &Db, s: &SaveDcaSetting) -> anyhow::Result<DcaSettingRow> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO dca_setting (id, monthly_budget, frequency, anchor_day, rounding_step, updated_at) \
         VALUES (1, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
           monthly_budget = excluded.monthly_budget, \
           frequency = excluded.frequency, \
           anchor_day = excluded.anchor_day, \
           rounding_step = excluded.rounding_step, \
           updated_at = excluded.updated_at",
    )
    .bind(&s.monthly_budget)
    .bind(&s.frequency)
    .bind(s.anchor_day)
    .bind(&s.rounding_step)
    .bind(&now)
    .execute(db)
    .await?;
    get(db).await
}
```

- [ ] **Step 6: Run the test to verify it passes**

Run: `cd backend && cargo test repo::dca_settings`
Expected: PASS (2 tests).

- [ ] **Step 7: Commit**

```bash
git add backend/migrations/0028_dca_setting.sql backend/src/repo/dca_settings.rs backend/src/repo/mod.rs
git commit -m "feat(dca): dca_setting table + settings repo with singleton upsert"
```

---

### Task 2: pure `compute_dca_plan` domain function

**Files:**
- Create: `backend/src/domain/dca.rs`
- Modify: `backend/src/domain/mod.rs` (add `pub mod dca;`)

**Interfaces:**
- Consumes: `crate::domain::allocation::CategoryInput { category_id: i64, name: String, target_pct: Decimal, tolerance_band_pct: Option<Decimal>, value_idr: Decimal }`.
- Produces:
  - `pub fn compute_dca_plan(categories: &[CategoryInput], budget: Decimal, rounding_step: Decimal) -> DcaPlan`
  - `pub struct DcaPlan { budget_idr: Decimal, total_value_idr: Decimal, mode: DcaMode, lines: Vec<DcaCategoryLine>, note: Option<String> }`
  - `pub struct DcaCategoryLine { category_id: i64, name: String, target_pct: Decimal, actual_pct: Decimal, current_value_idr: Decimal, drift_pct: Decimal, allocate_idr: Decimal, phase: DcaPhase }`
  - `pub enum DcaMode { Rebalance, Mixed, Proportional, Empty }`
  - `pub enum DcaPhase { Rebalance, Proportional, Mixed, None }`

- [ ] **Step 1: Register the module and write types + stubs**

In `backend/src/domain/mod.rs`, add: `pub mod dca;`

Create `backend/src/domain/dca.rs`:

```rust
use crate::domain::allocation::CategoryInput;
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DcaMode {
    Rebalance,
    Mixed,
    Proportional,
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DcaPhase {
    Rebalance,
    Proportional,
    Mixed,
    None,
}

#[derive(Debug, Clone, Serialize)]
pub struct DcaCategoryLine {
    pub category_id: i64,
    pub name: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub current_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub drift_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub allocate_idr: Decimal,
    pub phase: DcaPhase,
}

#[derive(Debug, Clone, Serialize)]
pub struct DcaPlan {
    #[serde(with = "rust_decimal::serde::str")]
    pub budget_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_value_idr: Decimal,
    pub mode: DcaMode,
    pub lines: Vec<DcaCategoryLine>,
    pub note: Option<String>,
}

/// Per-category raw (unrounded) split into its two phase contributions.
#[derive(Debug, Clone, Copy)]
struct RawAlloc {
    rebalance: Decimal,
    proportional: Decimal,
}
```

- [ ] **Step 2: Write failing tests for the two-phase raw split**

Append to `backend/src/domain/dca.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn cat(id: i64, name: &str, target: Decimal, band: Option<Decimal>, value: Decimal) -> CategoryInput {
        CategoryInput {
            category_id: id,
            name: name.into(),
            target_pct: target,
            tolerance_band_pct: band,
            value_idr: value,
        }
    }

    // Helper: total allocated across all lines.
    fn allocated(plan: &DcaPlan) -> Decimal {
        plan.lines.iter().map(|l| l.allocate_idr).sum()
    }

    #[test]
    fn starves_over_allocated_category() {
        // V=200M; Crypto 30% (target 40), Saham 30% (target 35) -> both under;
        // Reksa 40% (target 25) -> over, must get 0. Budget 55M, no rounding (step 1).
        let cats = vec![
            cat(1, "Crypto", dec!(40), None, dec!(60000000)),
            cat(2, "Saham", dec!(35), None, dec!(60000000)),
            cat(3, "Reksa", dec!(25), None, dec!(80000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(55000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Rebalance);
        let reksa = plan.lines.iter().find(|l| l.category_id == 3).unwrap();
        assert_eq!(reksa.allocate_idr, dec!(0));
        assert_eq!(reksa.phase, DcaPhase::None);
        // budget fully consumed, tilted toward larger gap (Crypto gap 42M > Saham gap 29.25M)
        let crypto = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        let saham = plan.lines.iter().find(|l| l.category_id == 2).unwrap();
        assert!(crypto.allocate_idr > saham.allocate_idr);
        assert_eq!(allocated(&plan), dec!(55000000));
    }

    #[test]
    fn fills_gaps_then_proportional_when_budget_exceeds_gaps() {
        // Two categories slightly under; small total gap, big budget -> Phase 2 kicks in.
        // V=100M: A 45M (target 50), B 55M (target 50). Gap only on A.
        let cats = vec![
            cat(1, "A", dec!(50), None, dec!(45000000)),
            cat(2, "B", dec!(50), None, dec!(55000000)),
        ];
        // T = 150M. A target@T = 75M, gap 30M. B is over (55% > 50%) -> starved in phase 1.
        // budget 50M > gap 30M -> remainder 20M spread by target (50/50) = 10M each.
        let plan = compute_dca_plan(&cats, dec!(50000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Mixed);
        let a = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        let b = plan.lines.iter().find(|l| l.category_id == 2).unwrap();
        assert_eq!(a.allocate_idr, dec!(40000000)); // 30M gap + 10M proportional
        assert_eq!(a.phase, DcaPhase::Mixed);
        assert_eq!(b.allocate_idr, dec!(10000000)); // proportional only
        assert_eq!(b.phase, DcaPhase::Proportional);
        assert_eq!(allocated(&plan), dec!(50000000));
    }

    #[test]
    fn balanced_portfolio_is_pure_proportional() {
        // Already at target -> no gaps -> all budget proportional by target.
        let cats = vec![
            cat(1, "A", dec!(60), None, dec!(60000000)),
            cat(2, "B", dec!(40), None, dec!(40000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Proportional);
        let a = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        let b = plan.lines.iter().find(|l| l.category_id == 2).unwrap();
        assert_eq!(a.allocate_idr, dec!(6000000));
        assert_eq!(b.allocate_idr, dec!(4000000));
    }

    #[test]
    fn tolerance_band_is_a_deadzone() {
        // A is 48% vs target 50% (drift -2) within band 5 -> NOT rebalanced.
        // B is 52% vs target 50% -> over -> starved. All budget goes proportional.
        let cats = vec![
            cat(1, "A", dec!(50), Some(dec!(5)), dec!(48000000)),
            cat(2, "B", dec!(50), Some(dec!(5)), dec!(52000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Proportional);
        let a = plan.lines.iter().find(|l| l.category_id == 1).unwrap();
        assert_eq!(a.phase, DcaPhase::Proportional);
        assert_eq!(a.allocate_idr, dec!(5000000));
    }

    #[test]
    fn uncategorized_zero_target_gets_nothing() {
        let cats = vec![
            cat(1, "Crypto", dec!(100), None, dec!(50000000)),
            cat(-1, "Lainnya", dec!(0), None, dec!(50000000)),
        ];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        let lainnya = plan.lines.iter().find(|l| l.category_id == -1).unwrap();
        assert_eq!(lainnya.allocate_idr, dec!(0));
    }

    #[test]
    fn target_under_100_leaves_cash() {
        // Single category, balanced, target 80 -> proportional gives 80% of budget, 20% stays cash.
        let cats = vec![cat(1, "A", dec!(80), None, dec!(80000000))];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        let a = &plan.lines[0];
        assert_eq!(a.allocate_idr, dec!(8000000));
        assert_eq!(allocated(&plan), dec!(8000000));
        assert!(plan.note.is_some()); // cash leftover reported
    }

    #[test]
    fn no_targets_is_empty_mode() {
        let cats = vec![cat(-1, "Lainnya", dec!(0), None, dec!(100000000))];
        let plan = compute_dca_plan(&cats, dec!(10000000), dec!(1));
        assert_eq!(plan.mode, DcaMode::Empty);
        assert_eq!(allocated(&plan), dec!(0));
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test domain::dca`
Expected: FAIL — `compute_dca_plan` not found.

- [ ] **Step 4: Implement `raw_allocations` and `compute_dca_plan` (no rounding yet — `apply_rounding` is identity)**

Add to `backend/src/domain/dca.rs` (above the test module):

```rust
fn raw_allocations(categories: &[CategoryInput], budget: Decimal) -> Vec<RawAlloc> {
    let hundred = Decimal::from(100);
    let total: Decimal = categories.iter().map(|c| c.value_idr).sum();
    let projected = total + budget; // T = V + B
    let n = categories.len();
    let mut out = vec![RawAlloc { rebalance: Decimal::ZERO, proportional: Decimal::ZERO }; n];

    // Phase 1: shortfalls for categories under target (by CURRENT %) beyond their band.
    let mut shortfalls = vec![Decimal::ZERO; n];
    for (i, c) in categories.iter().enumerate() {
        if c.target_pct <= Decimal::ZERO {
            continue;
        }
        let actual_pct = if total.is_zero() {
            Decimal::ZERO
        } else {
            c.value_idr / total * hundred
        };
        let band = c.tolerance_band_pct.unwrap_or(Decimal::ZERO);
        if actual_pct >= c.target_pct - band {
            continue; // within band or over target -> not a rebalance target
        }
        let target_value = projected * c.target_pct / hundred;
        let short = target_value - c.value_idr;
        if short > Decimal::ZERO {
            shortfalls[i] = short;
        }
    }
    let total_short: Decimal = shortfalls.iter().sum();

    if total_short >= budget && total_short > Decimal::ZERO {
        // Can't close every gap: split the whole budget proportional to gaps.
        for i in 0..n {
            if shortfalls[i] > Decimal::ZERO {
                out[i].rebalance = budget * shortfalls[i] / total_short;
            }
        }
        return out;
    }

    // Budget covers all gaps: fill each gap, then spread the remainder by target weight.
    for i in 0..n {
        out[i].rebalance = shortfalls[i];
    }
    let remainder = budget - total_short;
    if remainder > Decimal::ZERO {
        for (i, c) in categories.iter().enumerate() {
            if c.target_pct > Decimal::ZERO {
                // divide by 100 (not by sum of targets): if targets sum < 100, the slack stays cash.
                out[i].proportional = remainder * c.target_pct / hundred;
            }
        }
    }
    out
}

/// Largest-remainder rounding so each line is a multiple of `step` and the
/// total never exceeds the intended sum. `step <= 0` disables rounding.
fn apply_rounding(raws: &[Decimal], step: Decimal) -> Vec<Decimal> {
    if step <= Decimal::ZERO {
        return raws.to_vec();
    }
    let n = raws.len();
    let mut base = vec![Decimal::ZERO; n];
    let mut rem = vec![Decimal::ZERO; n];
    let mut base_units = Decimal::ZERO;
    let sum_raw: Decimal = raws.iter().sum();
    for i in 0..n {
        let q = (raws[i] / step).floor();
        base[i] = q * step;
        rem[i] = raws[i] - base[i];
        base_units += q;
    }
    let target_units = (sum_raw / step).floor();
    let mut extra = target_units - base_units; // whole steps still to hand out
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| rem[b].cmp(&rem[a]));
    let mut result = base;
    let mut k = 0;
    while extra > Decimal::ZERO && k < n {
        result[order[k]] += step;
        extra -= Decimal::ONE;
        k += 1;
    }
    result
}

pub fn compute_dca_plan(
    categories: &[CategoryInput],
    budget: Decimal,
    rounding_step: Decimal,
) -> DcaPlan {
    let hundred = Decimal::from(100);
    let total: Decimal = categories.iter().map(|c| c.value_idr).sum();
    let raws = raw_allocations(categories, budget);
    let raw_totals: Vec<Decimal> = raws.iter().map(|r| r.rebalance + r.proportional).collect();
    let rounded = apply_rounding(&raw_totals, rounding_step);

    let mut lines = Vec::with_capacity(categories.len());
    let mut any_rebalance = false;
    let mut any_proportional = false;
    for (i, c) in categories.iter().enumerate() {
        let actual_pct = if total.is_zero() {
            Decimal::ZERO
        } else {
            c.value_idr / total * hundred
        };
        let phase = match (raws[i].rebalance > Decimal::ZERO, raws[i].proportional > Decimal::ZERO) {
            (true, true) => DcaPhase::Mixed,
            (true, false) => DcaPhase::Rebalance,
            (false, true) => DcaPhase::Proportional,
            (false, false) => DcaPhase::None,
        };
        if raws[i].rebalance > Decimal::ZERO {
            any_rebalance = true;
        }
        if raws[i].proportional > Decimal::ZERO {
            any_proportional = true;
        }
        lines.push(DcaCategoryLine {
            category_id: c.category_id,
            name: c.name.clone(),
            target_pct: c.target_pct,
            actual_pct,
            current_value_idr: c.value_idr,
            drift_pct: actual_pct - c.target_pct,
            allocate_idr: rounded[i],
            phase,
        });
    }

    let allocated: Decimal = lines.iter().map(|l| l.allocate_idr).sum();
    let mode = if allocated.is_zero() {
        DcaMode::Empty
    } else if any_rebalance && any_proportional {
        DcaMode::Mixed
    } else if any_rebalance {
        DcaMode::Rebalance
    } else {
        DcaMode::Proportional
    };

    let cash_leftover = budget - allocated;
    let note = match mode {
        DcaMode::Empty => {
            Some("Belum ada kategori target. Atur alokasi di halaman Rencana dulu.".to_string())
        }
        DcaMode::Proportional => Some(
            "Portfolio sudah dalam target — alokasi mengikuti proporsi target (mode proporsional)."
                .to_string(),
        ),
        _ if cash_leftover > Decimal::ZERO => Some(format!(
            "Sisa Rp {} tidak teralokasi (target di bawah 100% atau pembulatan).",
            cash_leftover
        )),
        _ => None,
    };

    DcaPlan {
        budget_idr: budget,
        total_value_idr: total,
        mode,
        lines,
        note,
    }
}
```

- [ ] **Step 5: Run the two-phase tests to verify they pass**

Run: `cd backend && cargo test domain::dca`
Expected: PASS (7 tests). (All test budgets/values are multiples of any plausible step and use `step = dec!(1)`, so rounding is a no-op here.)

- [ ] **Step 6: Add a rounding test**

Append inside the `tests` module in `backend/src/domain/dca.rs`:

```rust
    #[test]
    fn rounds_to_step_and_total_stays_within_budget() {
        // Three under-target categories with awkward gaps; step 10k.
        let cats = vec![
            cat(1, "A", dec!(40), None, dec!(10000000)),
            cat(2, "B", dec!(35), None, dec!(10000000)),
            cat(3, "C", dec!(25), None, dec!(10000000)),
        ];
        let budget = dec!(55000000);
        let step = dec!(10000);
        let plan = compute_dca_plan(&cats, budget, step);
        // every line is a whole multiple of the step
        for l in &plan.lines {
            assert_eq!(l.allocate_idr % step, dec!(0), "{} not a multiple of step", l.name);
        }
        // total never exceeds budget, and with targets summing to 100 it equals budget
        let total: Decimal = plan.lines.iter().map(|l| l.allocate_idr).sum();
        assert!(total <= budget);
        assert_eq!(total, budget);
    }
```

- [ ] **Step 7: Run the rounding test to verify it passes**

Run: `cd backend && cargo test domain::dca`
Expected: PASS (8 tests). If it fails, the `apply_rounding` largest-remainder loop needs review — do NOT loosen the assertions.

- [ ] **Step 8: Clippy**

Run: `cd backend && cargo clippy --all-targets 2>&1 | grep -A3 "dca.rs" || echo "no dca clippy warnings"`
Expected: no warnings referencing `dca.rs`. Fix any that appear.

- [ ] **Step 9: Commit**

```bash
git add backend/src/domain/dca.rs backend/src/domain/mod.rs
git commit -m "feat(dca): pure rebalancing-aware two-phase DCA planner with rounding"
```

---

### Task 3: `/dca/settings` + `/dca/plan` API endpoints

**Files:**
- Create: `backend/src/api/dca.rs`
- Modify: `backend/src/api/mod.rs` (declare `mod dca;` near the other `mod` lines; add 2 routes to the `protected` block)

**Interfaces:**
- Consumes: `crate::repo::dca_settings::{get, upsert, DcaSettingRow, SaveDcaSetting}`; `crate::repo::dec`; `crate::service::portfolio::build_summary`; `crate::domain::allocation::CategoryInput`; `crate::domain::dca::{compute_dca_plan, DcaPlan}`; `crate::error::AppError`; `crate::AppState`.
- Produces: handlers `get_settings`, `update_settings`, `plan`; routes `GET/PATCH /dca/settings`, `GET /dca/plan`.

- [ ] **Step 1: Write the handlers**

Create `backend/src/api/dca.rs`:

```rust
use crate::domain::allocation::CategoryInput;
use crate::domain::dca::{compute_dca_plan, DcaPlan};
use crate::error::AppError;
use crate::repo::dca_settings::{self, DcaSettingRow, SaveDcaSetting};
use crate::repo::dec;
use crate::service::portfolio::build_summary;
use crate::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Deserialize;

pub async fn get_settings(
    State(s): State<AppState>,
) -> Result<Json<DcaSettingRow>, AppError> {
    Ok(Json(dca_settings::get(&s.db).await.map_err(AppError::Other)?))
}

pub async fn update_settings(
    State(s): State<AppState>,
    Json(body): Json<SaveDcaSetting>,
) -> Result<Json<DcaSettingRow>, AppError> {
    // Validate before persisting.
    dec(&body.monthly_budget).map_err(|e| AppError::BadRequest(e.to_string()))?;
    dec(&body.rounding_step).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if body.frequency != "monthly" && body.frequency != "weekly" {
        return Err(AppError::BadRequest("frequency must be 'monthly' or 'weekly'".into()));
    }
    if !(1..=28).contains(&body.anchor_day) {
        return Err(AppError::BadRequest("anchor_day must be between 1 and 28".into()));
    }
    Ok(Json(dca_settings::upsert(&s.db, &body).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct PlanQuery {
    /// Optional what-if budget override (decimal string). Defaults to saved settings.
    pub budget: Option<String>,
    /// Optional what-if frequency override ('monthly' | 'weekly').
    pub frequency: Option<String>,
}

pub async fn plan(
    State(s): State<AppState>,
    Query(q): Query<PlanQuery>,
) -> Result<Json<DcaPlan>, AppError> {
    let settings = dca_settings::get(&s.db).await.map_err(AppError::Other)?;

    let monthly = match q.budget.as_deref() {
        Some(b) => dec(b).map_err(|e| AppError::BadRequest(e.to_string()))?,
        None => dec(&settings.monthly_budget).map_err(AppError::Other)?,
    };
    let frequency = q.frequency.as_deref().unwrap_or(&settings.frequency);
    // Weekly slices the monthly budget into 4 (v1 simplification).
    let period_budget = if frequency == "weekly" {
        monthly / Decimal::from(4)
    } else {
        monthly
    };
    let rounding_step = dec(&settings.rounding_step).map_err(AppError::Other)?;

    // Reuse the portfolio summary's category aggregation (includes the "Lainnya" bucket).
    let summary = build_summary(&s.db).await.map_err(AppError::Other)?;
    let categories: Vec<CategoryInput> = summary
        .allocation
        .iter()
        .map(|a| CategoryInput {
            category_id: a.category_id,
            name: a.name.clone(),
            target_pct: a.target_pct,
            tolerance_band_pct: a.tolerance_band_pct,
            value_idr: a.actual_value_idr,
        })
        .collect();

    Ok(Json(compute_dca_plan(&categories, period_budget, rounding_step)))
}
```

- [ ] **Step 2: Declare the module**

In `backend/src/api/mod.rs`, add `mod dca;` next to the other `mod` declarations (e.g. near `mod cashflow;`).

- [ ] **Step 3: Register the routes**

In `backend/src/api/mod.rs`, inside the `let protected = Router::new()` chain (before `.route_layer(middleware::from_fn(auth::require_auth))`), add:

```rust
        .route(
            "/dca/settings",
            get(dca::get_settings).patch(dca::update_settings),
        )
        .route("/dca/plan", get(dca::plan))
```

- [ ] **Step 4: Build to verify it compiles**

Run: `cd backend && cargo build`
Expected: compiles clean. Fix any type mismatch (e.g. confirm `summary.allocation` items expose `category_id`, `name`, `target_pct`, `tolerance_band_pct`, `actual_value_idr` — they do, per `CategoryAllocation`).

- [ ] **Step 5: Write a handler-level validation test**

Create `backend/src/api/dca.rs` test module at the bottom of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_state() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::AppState::for_test(db)
    }

    #[tokio::test]
    async fn rejects_bad_frequency() {
        let st = mem_state().await;
        let err = update_settings(
            axum::extract::State(st),
            axum::Json(SaveDcaSetting {
                monthly_budget: "55000000".into(),
                frequency: "daily".into(),
                anchor_day: 12,
                rounding_step: "10000".into(),
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}
```

NOTE: confirm a test constructor for `AppState` exists. If `AppState::for_test` does not exist, instead inspect `backend/src/main.rs` / existing api tests for how `AppState` is built in tests and mirror that; if no pattern exists, SKIP this step's test and rely on the Task 2 domain tests + manual curl in Step 7 (document the skip in the commit body).

- [ ] **Step 6: Run the API test**

Run: `cd backend && cargo test api::dca`
Expected: PASS (or skipped per the note above).

- [ ] **Step 7: Manual smoke test (optional but recommended)**

Run the backend, then:
```bash
curl -s -H "authorization: Bearer $TOKEN" -X PATCH localhost:8080/api/dca/settings \
  -H 'content-type: application/json' \
  -d '{"monthly_budget":"55000000","frequency":"monthly","anchor_day":12,"rounding_step":"10000"}'
curl -s -H "authorization: Bearer $TOKEN" localhost:8080/api/dca/plan
```
Expected: settings echoed; plan returns `lines`, `mode`, `note`, `budget_idr`.

- [ ] **Step 8: Commit**

```bash
git add backend/src/api/dca.rs backend/src/api/mod.rs
git commit -m "feat(dca): /dca/settings and /dca/plan endpoints"
```

---

### Task 4: Frontend schemas + hooks

**Files:**
- Modify: `frontend/src/api/schemas.ts` (add DCA schemas)
- Modify: `frontend/src/api/hooks.ts` (add 3 hooks)

**Interfaces:**
- Produces: `DcaSettingsSchema`/`DcaSettings`, `DcaPlanSchema`/`DcaPlan`, `DcaCategoryLineSchema`; hooks `useDcaSettings()`, `useUpdateDcaSettings()`, `useDcaPlan()`.

- [ ] **Step 1: Add the Zod schemas**

In `frontend/src/api/schemas.ts`, append:

```ts
export const DcaSettingsSchema = z.object({
  id: z.number(),
  monthly_budget: z.string(),
  frequency: z.enum(["monthly", "weekly"]),
  anchor_day: z.number(),
  rounding_step: z.string(),
  updated_at: z.string(),
});
export type DcaSettings = z.infer<typeof DcaSettingsSchema>;

export const DcaCategoryLineSchema = z.object({
  category_id: z.number(),
  name: z.string(),
  target_pct: z.string(),
  actual_pct: z.string(),
  current_value_idr: z.string(),
  drift_pct: z.string(),
  allocate_idr: z.string(),
  phase: z.enum(["rebalance", "proportional", "mixed", "none"]),
});
export type DcaCategoryLine = z.infer<typeof DcaCategoryLineSchema>;

export const DcaPlanSchema = z.object({
  budget_idr: z.string(),
  total_value_idr: z.string(),
  mode: z.enum(["rebalance", "mixed", "proportional", "empty"]),
  lines: z.array(DcaCategoryLineSchema),
  note: z.string().nullable(),
});
export type DcaPlan = z.infer<typeof DcaPlanSchema>;
```

- [ ] **Step 2: Add the hooks**

In `frontend/src/api/hooks.ts`, add the schema imports to the existing `./schemas` import block:

```ts
  DcaSettingsSchema, DcaPlanSchema,
  type DcaSettings,
```

Then append the hooks (the GET hooks near the other queries, the mutation near the others — placement is cosmetic):

```ts
export const useDcaSettings = () =>
  useQuery({ queryKey: ["dca-settings"], queryFn: () => api.get("/dca/settings", DcaSettingsSchema) });

export const useDcaPlan = () =>
  useQuery({ queryKey: ["dca-plan"], queryFn: () => api.get("/dca/plan", DcaPlanSchema) });

export const useUpdateDcaSettings = () =>
  useInvalidatingMutation(
    (b: Omit<DcaSettings, "id" | "updated_at">) => api.patch("/dca/settings", DcaSettingsSchema, b),
    ["dca-settings", "dca-plan"],
  );
```

- [ ] **Step 3: Type-check**

Run: `cd frontend && npx tsc --noEmit`
Expected: no errors. (If `useInvalidatingMutation` is not exported, it's a module-private helper used the same way the other mutations in `hooks.ts` use it — define the hook in the same file, which already has access.)

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts
git commit -m "feat(dca): frontend schemas and query/mutation hooks"
```

---

### Task 5: `/dca` page + route + nav

**Files:**
- Create: `frontend/src/pages/DcaPage.tsx`
- Modify: `frontend/src/App.tsx` (import + `<Route path="dca" .../>`)
- Modify: `frontend/src/components/AppShell.tsx` (nav item + icon import)

**Interfaces:**
- Consumes: `useDcaSettings`, `useUpdateDcaSettings`, `useDcaPlan` from `../api/hooks`; `formatIDR`, `formatPct`, `parseNum` from `../lib/format`; `QueryState` from `../components/QueryState`; `toast` from `sonner`.

- [ ] **Step 1: Write the page**

Create `frontend/src/pages/DcaPage.tsx`:

```tsx
import { useState, useEffect } from "react";
import { Repeat, Save } from "lucide-react";
import { toast } from "sonner";
import { useDcaSettings, useUpdateDcaSettings, useDcaPlan } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, formatPct, parseNum } from "../lib/format";

const MODE_LABEL: Record<string, string> = {
  rebalance: "Rebalancing",
  mixed: "Rebalancing + Proporsional",
  proportional: "Proporsional",
  empty: "Belum ada target",
};

export default function DcaPage() {
  const settings = useDcaSettings();
  const plan = useDcaPlan();
  const save = useUpdateDcaSettings();

  const [form, setForm] = useState({
    monthly_budget: "",
    frequency: "monthly",
    anchor_day: "1",
    rounding_step: "10000",
  });

  // Seed the form once settings load.
  useEffect(() => {
    if (settings.data) {
      setForm({
        monthly_budget: settings.data.monthly_budget,
        frequency: settings.data.frequency,
        anchor_day: String(settings.data.anchor_day),
        rounding_step: settings.data.rounding_step,
      });
    }
  }, [settings.data]);

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    save.mutate(
      {
        monthly_budget: form.monthly_budget || "0",
        frequency: form.frequency as "monthly" | "weekly",
        anchor_day: Number(form.anchor_day),
        rounding_step: form.rounding_step || "10000",
      },
      {
        onSuccess: () => toast.success("Setelan DCA disimpan"),
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  return (
    <div>
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">DCA Planner</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>
            Alokasi kontribusi rutin menuju target diversifikasi
          </div>
        </div>
      </div>

      <div className="lay-2-15col" style={{ gap: 16 }}>
        {/* Settings */}
        <div className="card">
          <div className="card-head">
            <div>
              <div className="card-title">Setelan</div>
              <div className="card-sub">budget &amp; frekuensi</div>
            </div>
          </div>
          <form className="card-pad" style={{ paddingTop: 16 }} onSubmit={submit}>
            <label className="field">
              <span className="field-label">Budget bulanan (IDR)</span>
              <input type="number" className="input" placeholder="55000000"
                     value={form.monthly_budget} onChange={set("monthly_budget")} />
            </label>
            <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              <label className="field">
                <span className="field-label">Frekuensi</span>
                <select className="select" value={form.frequency} onChange={set("frequency")}>
                  <option value="monthly">Bulanan</option>
                  <option value="weekly">Mingguan</option>
                </select>
              </label>
              <label className="field">
                <span className="field-label">Tanggal anchor</span>
                <input type="number" min={1} max={28} className="input"
                       value={form.anchor_day} onChange={set("anchor_day")} />
              </label>
            </div>
            <label className="field">
              <span className="field-label">Pembulatan (IDR)</span>
              <input type="number" className="input" placeholder="10000"
                     value={form.rounding_step} onChange={set("rounding_step")} />
            </label>
            <button type="submit" className="btn btn-primary" disabled={save.isPending}
                    style={{ marginTop: 8 }}>
              <Save size={16} /> Simpan
            </button>
          </form>
        </div>

        {/* Plan */}
        <div className="card">
          <div className="card-head">
            <div>
              <div className="card-title">Rencana periode ini</div>
              <div className="card-sub">
                <Repeat size={13} style={{ display: "inline", verticalAlign: "-2px" }} />{" "}
                {plan.data ? `${MODE_LABEL[plan.data.mode] ?? plan.data.mode} · budget ${formatIDR(plan.data.budget_idr)}` : "—"}
              </div>
            </div>
          </div>
          <div className="card-pad" style={{ paddingTop: 16 }}>
            <QueryState isLoading={plan.isLoading} error={plan.error}>
              {plan.data && plan.data.note && (
                <div className="t-sm t-muted" style={{ marginBottom: 12 }}>{plan.data.note}</div>
              )}
              <div style={{ display: "flex", flexDirection: "column", gap: 14 }}>
                {(plan.data?.lines ?? []).map((l) => {
                  const alloc = parseNum(l.allocate_idr);
                  const budget = parseNum(plan.data?.budget_idr ?? "0");
                  const ratio = budget > 0 ? Math.min((alloc / budget) * 100, 100) : 0;
                  const muted = alloc <= 0;
                  return (
                    <div key={l.category_id} style={{ display: "flex", flexDirection: "column", gap: 6, opacity: muted ? 0.55 : 1 }}>
                      <div className="flex items-center justify-between">
                        <span className="t-sm" style={{ fontWeight: 500 }}>
                          {l.name}
                          <span className="t-muted" style={{ fontWeight: 400 }}>
                            {" "}· {formatPct(l.actual_pct)} / target {formatPct(l.target_pct)}
                          </span>
                        </span>
                        <span className="t-sm num" style={{ fontWeight: 600 }}>{formatIDR(l.allocate_idr)}</span>
                      </div>
                      <div className="progress">
                        <span style={{ width: `${ratio}%`, background: muted ? "hsl(var(--muted-foreground))" : "hsl(var(--primary))" }} />
                      </div>
                    </div>
                  );
                })}
              </div>
            </QueryState>
          </div>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Add the route**

In `frontend/src/App.tsx`, add the import near the other page imports:

```tsx
import DcaPage from "./pages/DcaPage";
```

And add the route after the `budget` route inside `<Route element={<AppShell />}>`:

```tsx
        <Route path="dca" element={<DcaPage />} />
```

- [ ] **Step 3: Add the nav item**

In `frontend/src/components/AppShell.tsx`, add `Repeat` to the existing `lucide-react` import, then add a nav item to the `"Asisten"` group's `items` array (after the Budget entry):

```tsx
      { to: "/dca", label: "DCA", icon: Repeat },
```

- [ ] **Step 4: Type-check and build**

Run: `cd frontend && npx tsc --noEmit && npm run build`
Expected: no type errors; build succeeds.

- [ ] **Step 5: Manual check**

Start the frontend dev server, log in, navigate to `/dca`. Verify: settings form loads saved values, saving shows a toast, the plan table renders per-category allocations, over-allocated categories show Rp 0 (muted), and the mode badge + note display.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/DcaPage.tsx frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(dca): DCA planner page, route, and nav entry"
```

---

## Self-Review

**Spec coverage:**
- §3 decision 1 (rebalancing-aware) → Task 2 `raw_allocations` Phase 1/2. ✓
- §3 decision 2 (buy-only starve) → Task 2 current-% gate; `starves_over_allocated_category` test. ✓
- §3 decision 3 (two-phase overflow) → Task 2 `fills_gaps_then_proportional...` test. ✓
- §3 decision 4 (monthly budget, weekly = ÷N recompute) → Task 3 `plan` handler weekly slice; recompute is inherent (stateless GET). ✓
- §3 decision 5 (stateless, settings persisted) → Task 1 table; no plan/execution tables. ✓
- §3 decision 6 (category-level) → output is per-category lines. ✓
- §3 decision 7 (Lainnya = 0) → `uncategorized_zero_target_gets_nothing` test. ✓
- §3 decision 8 (band deadzone) → `tolerance_band_is_a_deadzone` test. ✓
- §3 decision 9 (rounding Rp 10k, largest-remainder) → Task 2 `apply_rounding` + `rounds_to_step...` test. ✓
- §4 algorithm (T = V+B, current-% gate) → Task 2. ✓
- §5 frequency → Task 3. ✓
- §6 data model / backend / frontend → Tasks 1,3,4,5. ✓
- §7 error handling (validation 400, no panics, Decimal) → Task 3 `update_settings` validation; Global Constraints. ✓
- §8 testing (9 cases) → Task 2 has all 9 (starve, gaps-then-proportional, balanced, uncategorized, band, target<100, no-targets, rounding) + budget<gaps covered by `starves_over_allocated_category` (S>B). ✓
- §9 future phases → explicitly out of scope; no tasks. ✓

**Placeholder scan:** Task 3 Step 5 intentionally documents a conditional skip for `AppState::for_test` (real fallback, not a placeholder). No TBD/TODO elsewhere.

**Type consistency:** `CategoryInput` fields, `compute_dca_plan(categories, budget, rounding_step)` signature, `DcaSettingRow`/`SaveDcaSetting` fields, and the snake_case enum values (`rebalance`/`mixed`/`proportional`/`empty`, `none`) match between backend serialize (Task 2/3) and frontend Zod enums (Task 4). `useInvalidatingMutation` and `api.patch` match the verbatim hooks reference.

## Execution Handoff

After saving the plan, choose an execution approach (subagent-driven recommended, or inline).
