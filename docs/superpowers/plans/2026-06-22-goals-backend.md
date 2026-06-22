# Goals Backend (Phase 3) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Track financial goals from transactions tagged to them — a `txn.goal_id` tag, a `goal.target_date`, a `current_kind='tagged'` mode whose progress is the market value of the tagged holdings (with invested capital + P&L + required-monthly), `PATCH /goals/:id`, an extended `GoalResponse`, an HTTP endpoint to tag a transaction, and assistant chat tools.

**Architecture:** Additive overlay on the existing `goal`/`txn` tables. Tagging lives at the repo row level (`set_txn_goal`/`list_by_goal`) — the domain `Transaction` model is intentionally NOT changed, so the wide set of `build_summary`/`cost_basis` test fixtures stay compiling. A pure domain function `compute_goal_progress` computes market value / invested / P&L from a goal's tagged transactions plus a per-instrument current-price map; a thin service assembles that map (reusing the same latest-price + native→IDR fx logic as `build_summary`). The API extends `GoalResponse` and adds `PATCH /goals/:id` + a tag endpoint; the assistant gets `list_goals` + `tag_transaction_to_goal` tools.

**Tech Stack:** Rust, axum, sqlx (SQLite), rust_decimal, chrono, anyhow/thiserror, tokio, serde_json (assistant tools).

**Spec:** `docs/superpowers/specs/2026-06-22-planner-tree-and-goals-design.md` (the "goal progress" and goal-tagging sections).

## Global Constraints

- **No rustfmt.** Never run `cargo fmt`. Match surrounding style by hand. Verify with `cargo test` + `cargo clippy` only.
- **Decimals are TEXT.** Money/percent columns are `TEXT`, parsed with `crate::repo::dec(&str) -> anyhow::Result<Decimal>`. Never use floats for money.
- **No `unwrap()`/`panic!()`/`expect()` in non-test code.** Propagate with `?` and `anyhow`/`AppError`. `unwrap()` is fine inside `#[cfg(test)]`.
- **Do NOT add `goal_id` to the domain `Transaction` struct** (`backend/src/domain/models.rs`) or to `transactions::NewTransaction`/`TxnPatch`. Tagging is a row-level concern handled by dedicated repo functions. This keeps every existing `Transaction { ... }` literal in tests compiling.
- **Migration versioning.** New file `backend/migrations/0031_goal_tracking.sql` (highest existing is `0030`). The `db::tests::migration_versions_are_unique` test enforces uniqueness. Migrations run automatically via `sqlx::migrate!("./migrations")` in `db::connect`.
- **Repo conventions:** module per table under `backend/src/repo/`. Row struct (`sqlx::FromRow`), `NewXxx`/`UpdateXxx` (Deserialize), free `create/get/list/update/delete` fns taking `&Db`, `#[cfg(test)] mod tests` with `crate::db::connect("sqlite::memory:")`.
- **Goal progress math (spec):** for `current_kind='tagged'`, over the goal's tagged txns — net units per instrument = Σ(buy qty) − Σ(sell qty); `market_value_idr` = Σ net_units × current_price_idr (PRIMARY progress); `invested_idr` = Σ buy cost incl. fee − Σ sell proceeds net of fee, each × the txn's `fx_to_idr` (SECONDARY); `gain_loss_idr` = market − invested. Only Buy/Sell contribute; other txn types tagged to a goal contribute nothing (documented). Cost basis is net-cash, not FIFO (a documented approximation).
- **Conventional commits** (`feat:`/`fix:`). Commit after every green test cycle.
- **Test command (from repo root):** `cargo test --manifest-path backend/Cargo.toml`. Scope with e.g. `cargo test --manifest-path backend/Cargo.toml goal_progress`.
- No new crate dependencies; do not touch `Cargo.toml`/`Cargo.lock`.

## File structure

- **Create** `backend/migrations/0031_goal_tracking.sql` — `txn.goal_id` + index, `goal.target_date`.
- **Modify** `backend/src/repo/transactions.rs` — add `set_txn_goal` + `list_by_goal` (no struct changes).
- **Modify** `backend/src/repo/goals.rs` — `target_date` on `GoalRow`/`NewGoal`, `'tagged'` kind, `UpdateGoal` + `update`.
- **Create** `backend/src/domain/goal_progress.rs` — pure `compute_goal_progress` + required-monthly helpers.
- **Modify** `backend/src/domain/mod.rs` — `pub mod goal_progress;`.
- **Create** `backend/src/service/goals.rs` — `build_goal_progress(db, goal_id)`.
- **Modify** `backend/src/service/mod.rs` — `pub mod goals;`.
- **Modify** `backend/src/api/goals.rs` — extend `GoalResponse`, per-kind compute, `update_goal`, tag endpoint.
- **Modify** `backend/src/api/mod.rs` — register `PATCH /goals/:id` and the tag route.
- **Modify** `backend/src/assistant/tools.rs` + `backend/src/assistant/dispatcher.rs` — `list_goals` + `tag_transaction_to_goal` tools.

---

## Task 1: Migration — `txn.goal_id` + `goal.target_date`

**Files:**
- Create: `backend/migrations/0031_goal_tracking.sql`

**Interfaces:**
- Produces: column `txn.goal_id INTEGER REFERENCES goal(id)` (+ index `idx_txn_goal`), column `goal.target_date TEXT` (nullable).

- [ ] **Step 1: Write the migration SQL**

Create `backend/migrations/0031_goal_tracking.sql`:

```sql
-- Tag a transaction to at most one goal; progress for current_kind='tagged'
-- goals is computed from the txns carrying their goal_id.
ALTER TABLE txn ADD COLUMN goal_id INTEGER REFERENCES goal(id);
CREATE INDEX idx_txn_goal ON txn(goal_id);

-- Optional target date for a goal (ISO 'YYYY-MM-DD'); drives required-monthly.
ALTER TABLE goal ADD COLUMN target_date TEXT;
```

- [ ] **Step 2: Verify boot + unique migration versions**

Run: `cargo test --manifest-path backend/Cargo.toml migration_versions_are_unique`
Expected: PASS (no duplicate `0031`).

Run: `cargo test --manifest-path backend/Cargo.toml --lib db::`
Expected: PASS — `connect("sqlite::memory:")` runs all migrations including `0031`.

- [ ] **Step 3: Commit**

```bash
git add backend/migrations/0031_goal_tracking.sql
git commit -m "feat(goals): add txn.goal_id tag + goal.target_date"
```

---

## Task 2: txn repo — tag / list-by-goal

**Files:**
- Modify: `backend/src/repo/transactions.rs`

**Interfaces:**
- Consumes: existing `TxnRowRaw` (FromRow, explicit-column SELECT) + `into_domain()`; `get(db, id)`.
- Produces:
  - `async fn set_txn_goal(db: &Db, id: i64, goal_id: Option<i64>) -> anyhow::Result<()>` — sets/clears the tag; errors if the txn id doesn't exist.
  - `async fn list_by_goal(db: &Db, goal_id: i64) -> anyhow::Result<Vec<Transaction>>` — all txns tagged to the goal, oldest first.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `backend/src/repo/transactions.rs`:

```rust
    #[tokio::test]
    async fn tag_and_list_by_goal_round_trip() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let goal = crate::repo::goals::create(&db, &crate::repo::goals::NewGoal { label:"Dana Pendidikan".into(), note:None, target_idr:"200000000".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();

        let buy = |q: &str| NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:q.into(), price_native:"9000".into(), fee_native:None,
            currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None };
        let t1 = create(&db, &buy("100")).await.unwrap();
        let t2 = create(&db, &buy("50")).await.unwrap();

        // Initially nothing is tagged.
        assert!(list_by_goal(&db, goal.id).await.unwrap().is_empty());

        set_txn_goal(&db, t1.id, Some(goal.id)).await.unwrap();
        set_txn_goal(&db, t2.id, Some(goal.id)).await.unwrap();
        let tagged = list_by_goal(&db, goal.id).await.unwrap();
        assert_eq!(tagged.len(), 2);

        // Untag t2.
        set_txn_goal(&db, t2.id, None).await.unwrap();
        let after = list_by_goal(&db, goal.id).await.unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].id, t1.id);
    }

    #[tokio::test]
    async fn set_txn_goal_rejects_unknown_txn() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(set_txn_goal(&db, 999, None).await.is_err());
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml tag_and_list_by_goal`
Expected: FAIL — `set_txn_goal`/`list_by_goal` not found.

- [ ] **Step 3: Implement the functions**

Add to `backend/src/repo/transactions.rs` (after `list_for_instrument`, before the `TxnPatch` block):

```rust
/// Tag (or with `None`, untag) a transaction to a goal. Errors if the txn id
/// doesn't exist so a bad id is a clear failure, not a silent no-op.
pub async fn set_txn_goal(db: &Db, id: i64, goal_id: Option<i64>) -> anyhow::Result<()> {
    get(db, id).await?; // 404s as RowNotFound -> caller maps appropriately
    sqlx::query("UPDATE txn SET goal_id = ? WHERE id = ?")
        .bind(goal_id).bind(id).execute(db).await?;
    Ok(())
}

/// All transactions tagged to a goal, oldest first. Returns plain domain
/// `Transaction`s (goal_id is a row-level tag, not part of the domain model).
pub async fn list_by_goal(db: &Db, goal_id: i64) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE goal_id = ? ORDER BY executed_at")
        .bind(goal_id).fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml --lib repo::transactions`
Expected: PASS (new tests + all existing txn tests).

> Note: the `tag_and_list_by_goal_round_trip` test constructs `NewGoal { ..., target_date: None }`. That field is added to `NewGoal` in Task 3. If you implement strictly task-by-task and Task 3 is not done yet, this test won't compile. Implement Task 3's `NewGoal`/`GoalRow`/`create` change FIRST if you hit that, or run this task's test after Task 3. (The subagent controller sequences Task 3 before re-confirming Task 2's suite at the final run.)

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/transactions.rs
git commit -m "feat(goals): tag/untag a txn to a goal + list_by_goal repo fns"
```

---

## Task 3: goals repo — `target_date`, `'tagged'` kind, `update`

**Files:**
- Modify: `backend/src/repo/goals.rs`

**Interfaces:**
- Consumes: existing `GoalRow` (FromRow over `SELECT *`), `NewGoal`, `create`, `get`, `dec`.
- Produces:
  - `GoalRow` gains `pub target_date: Option<String>`.
  - `NewGoal` gains `pub target_date: Option<String>`.
  - `VALID_KINDS` = `["cash","networth","manual","tagged"]`.
  - `pub struct UpdateGoal { label, note, target_idr, current_kind, current_manual_idr, target_date, sort_order }` (all `Option<...>`).
  - `async fn update(db: &Db, id: i64, u: &UpdateGoal) -> anyhow::Result<GoalRow>`.

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `backend/src/repo/goals.rs`:

```rust
    #[tokio::test]
    async fn create_tagged_goal_with_target_date() {
        let db = mem_db().await;
        let g = NewGoal {
            label: "Pendidikan Anak".into(), note: Some("SD 2035".into()),
            target_idr: "200000000".into(), current_kind: "tagged".into(),
            current_manual_idr: None, sort_order: None, target_date: Some("2035-06-01".into()),
        };
        let created = create(&db, &g).await.unwrap();
        assert_eq!(created.current_kind, "tagged");
        assert_eq!(created.target_date.as_deref(), Some("2035-06-01"));
    }

    #[tokio::test]
    async fn update_changes_fields_keeping_others() {
        let db = mem_db().await;
        let created = create(&db, &NewGoal {
            label: "Dana Darurat".into(), note: None, target_idr: "100000000".into(),
            current_kind: "cash".into(), current_manual_idr: None, sort_order: Some(1), target_date: None,
        }).await.unwrap();

        let updated = update(&db, created.id, &UpdateGoal {
            target_idr: Some("136000000".into()),
            target_date: Some("2027-01-01".into()),
            label: None, note: None, current_kind: None, current_manual_idr: None, sort_order: None,
        }).await.unwrap();

        assert_eq!(updated.target_idr, "136000000");
        assert_eq!(updated.target_date.as_deref(), Some("2027-01-01"));
        assert_eq!(updated.label, "Dana Darurat"); // preserved
        assert_eq!(updated.current_kind, "cash");   // preserved
    }

    #[tokio::test]
    async fn update_rejects_bad_kind() {
        let db = mem_db().await;
        let created = create(&db, &NewGoal {
            label: "X".into(), note: None, target_idr: "1".into(),
            current_kind: "cash".into(), current_manual_idr: None, sort_order: None, target_date: None,
        }).await.unwrap();
        assert!(update(&db, created.id, &UpdateGoal {
            current_kind: Some("bogus".into()),
            label: None, note: None, target_idr: None, current_manual_idr: None, target_date: None, sort_order: None,
        }).await.is_err());
    }
```

> If a `mem_db()` helper does not already exist in this test module, add it: `async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }` (the Phase-1 goals tests already use this pattern).

Also update EVERY existing `NewGoal { ... }` literal in this file's test module to add `target_date: None,` (the create tests from Phase 1). There are several — add the field to each so the module compiles.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml --lib repo::goals`
Expected: FAIL — `target_date` field / `UpdateGoal` / `update` not defined (compile error).

- [ ] **Step 3: Implement the changes**

In `backend/src/repo/goals.rs`:

1. Add `target_date` to `GoalRow` (after `created_at`):
```rust
    pub created_at: String,
    pub target_date: Option<String>,
```

2. Add `target_date` to `NewGoal`:
```rust
    pub sort_order: Option<i64>,
    pub target_date: Option<String>,
```

3. Extend `VALID_KINDS`:
```rust
const VALID_KINDS: &[&str] = &["cash", "networth", "manual", "tagged"];
```

4. In `create`, add `target_date` to the INSERT (column list + bind). Change the INSERT to:
```rust
    let id = sqlx::query(
        "INSERT INTO goal (label, note, target_idr, current_kind, current_manual_idr, sort_order, created_at, target_date)
         VALUES (?,?,?,?,?,?,?,?)",
    )
    .bind(&g.label)
    .bind(&g.note)
    .bind(&g.target_idr)
    .bind(&g.current_kind)
    .bind(&g.current_manual_idr)
    .bind(sort_order)
    .bind(&now)
    .bind(&g.target_date)
    .execute(db)
    .await?
    .last_insert_rowid();
```

5. Add the update struct + function (after `delete`):
```rust
/// Partial update; absent fields keep their current values.
#[derive(Debug, Deserialize)]
pub struct UpdateGoal {
    pub label: Option<String>,
    pub note: Option<String>,
    pub target_idr: Option<String>,
    pub current_kind: Option<String>,
    pub current_manual_idr: Option<String>,
    pub target_date: Option<String>,
    pub sort_order: Option<i64>,
}

pub async fn update(db: &Db, id: i64, u: &UpdateGoal) -> anyhow::Result<GoalRow> {
    let cur = get(db, id).await?;
    let current_kind = u.current_kind.clone().unwrap_or(cur.current_kind);
    if !VALID_KINDS.contains(&current_kind.as_str()) {
        anyhow::bail!("current_kind must be one of {VALID_KINDS:?}, got '{current_kind}'");
    }
    let target_idr = u.target_idr.clone().unwrap_or(cur.target_idr);
    dec(&target_idr)?;
    let label = u.label.clone().unwrap_or(cur.label);
    let note = u.note.clone().or(cur.note);
    let current_manual_idr = u.current_manual_idr.clone().or(cur.current_manual_idr);
    if current_kind == "manual" {
        match current_manual_idr.as_deref() {
            Some(v) => { dec(v)?; }
            None => anyhow::bail!("current_manual_idr is required when current_kind='manual'"),
        }
    }
    let target_date = u.target_date.clone().or(cur.target_date);
    let sort_order = u.sort_order.unwrap_or(cur.sort_order);

    sqlx::query(
        "UPDATE goal SET label=?, note=?, target_idr=?, current_kind=?, current_manual_idr=?, target_date=?, sort_order=? WHERE id=?",
    )
    .bind(&label).bind(&note).bind(&target_idr).bind(&current_kind)
    .bind(&current_manual_idr).bind(&target_date).bind(sort_order).bind(id)
    .execute(db).await?;
    get(db, id).await
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml --lib repo::goals`
Expected: PASS.

- [ ] **Step 5: Confirm Task 2's txn tests now compile/pass (NewGoal gained target_date)**

Run: `cargo test --manifest-path backend/Cargo.toml --lib repo::transactions`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/repo/goals.rs
git commit -m "feat(goals): target_date column, 'tagged' kind, goals::update"
```

---

## Task 4: Domain — pure `compute_goal_progress` + required-monthly helpers

**Files:**
- Create: `backend/src/domain/goal_progress.rs`
- Modify: `backend/src/domain/mod.rs` (add `pub mod goal_progress;`)

**Interfaces:**
- Consumes: `crate::domain::models::{Transaction, TxnType}`, `rust_decimal::Decimal`, `std::collections::HashMap`, `chrono::NaiveDate`.
- Produces:
  - `pub struct GoalProgress { pub market_value_idr: Decimal, pub invested_idr: Decimal, pub gain_loss_idr: Decimal }`
  - `pub fn compute_goal_progress(txns: &[Transaction], price_idr: &HashMap<i64, Decimal>) -> GoalProgress`
  - `pub fn months_until(now: NaiveDate, target: NaiveDate) -> i64` (clamped to ≥ 1)
  - `pub fn required_monthly(target_idr: Decimal, current_idr: Decimal, months_left: i64) -> Decimal`

- [ ] **Step 1: Register the module**

In `backend/src/domain/mod.rs` add (near `pub mod cost_basis;`):
```rust
pub mod goal_progress;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/domain/goal_progress.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{Transaction, TxnType};
    use chrono::{NaiveDate, TimeZone, Utc};
    use rust_decimal_macros::dec;
    use std::collections::HashMap;

    fn txn(id: i64, instrument_id: i64, kind: TxnType, qty: Decimal, price: Decimal, fee: Decimal, fx_to_idr: Decimal) -> Transaction {
        Transaction {
            id, account_id: 1, instrument_id, txn_type: kind,
            executed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            quantity: qty, price_native: price, fee_native: fee,
            currency: "IDR".into(), fx_to_idr, fx_to_usd: dec!(1), note: None,
        }
    }

    #[test]
    fn buy_only_market_invested_and_gain() {
        // Buy 100 @ 9000 (+10 fee), IDR. Current price 9500 IDR.
        let txns = vec![txn(1, 7, TxnType::Buy, dec!(100), dec!(9000), dec!(10), dec!(1))];
        let mut price = HashMap::new();
        price.insert(7, dec!(9500));
        let p = compute_goal_progress(&txns, &price);
        assert_eq!(p.market_value_idr, dec!(950000));        // 100 * 9500
        assert_eq!(p.invested_idr, dec!(900010));            // 100*9000 + 10
        assert_eq!(p.gain_loss_idr, dec!(49990));            // 950000 - 900010
    }

    #[test]
    fn buy_then_partial_sell_nets_units_and_invested() {
        // Buy 100 @ 9000; Sell 40 @ 9500 (−5 fee). Current price 9500.
        let txns = vec![
            txn(1, 7, TxnType::Buy, dec!(100), dec!(9000), dec!(0), dec!(1)),
            txn(2, 7, TxnType::Sell, dec!(40), dec!(9500), dec!(5), dec!(1)),
        ];
        let mut price = HashMap::new();
        price.insert(7, dec!(9500));
        let p = compute_goal_progress(&txns, &price);
        assert_eq!(p.market_value_idr, dec!(570000));        // (100-40)=60 * 9500
        // invested = buy 900000 - sell proceeds (40*9500 - 5 = 379995) = 520005
        assert_eq!(p.invested_idr, dec!(520005));
        assert_eq!(p.gain_loss_idr, dec!(49995));            // 570000 - 520005
    }

    #[test]
    fn multi_instrument_sums_and_missing_price_is_zero() {
        let txns = vec![
            txn(1, 7, TxnType::Buy, dec!(10), dec!(100), dec!(0), dec!(1)),
            txn(2, 8, TxnType::Buy, dec!(5),  dec!(200), dec!(0), dec!(1)),
        ];
        let mut price = HashMap::new();
        price.insert(7, dec!(150)); // instrument 8 has no price -> contributes 0 to market value
        let p = compute_goal_progress(&txns, &price);
        assert_eq!(p.market_value_idr, dec!(1500));          // 10*150 + 5*0
        assert_eq!(p.invested_idr, dec!(2000));              // 1000 + 1000
    }

    #[test]
    fn months_until_is_clamped_to_one() {
        let now = NaiveDate::from_ymd_opt(2026, 6, 22).unwrap();
        assert_eq!(months_until(now, NaiveDate::from_ymd_opt(2026, 6, 1).unwrap()), 1); // past -> 1
        assert_eq!(months_until(now, NaiveDate::from_ymd_opt(2026, 6, 30).unwrap()), 1); // same month -> 1
        assert_eq!(months_until(now, NaiveDate::from_ymd_opt(2027, 6, 22).unwrap()), 12);
    }

    #[test]
    fn required_monthly_is_remaining_over_months() {
        assert_eq!(required_monthly(dec!(200000000), dec!(80000000), 12), dec!(10000000));
        assert_eq!(required_monthly(dec!(100), dec!(150), 5), dec!(0)); // already met -> 0
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml goal_progress`
Expected: FAIL — types/functions not defined (compile error).

- [ ] **Step 4: Implement the pure logic**

Prepend to `backend/src/domain/goal_progress.rs`:

```rust
use crate::domain::models::{Transaction, TxnType};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct GoalProgress {
    pub market_value_idr: Decimal,
    pub invested_idr: Decimal,
    pub gain_loss_idr: Decimal,
}

/// Compute a goal's progress from its tagged transactions.
///
/// - net units per instrument = Σ(buy qty) − Σ(sell qty)
/// - market_value_idr = Σ net_units × current_price_idr (0 for instruments absent from `price_idr`)
/// - invested_idr = Σ buy cost incl. fee − Σ sell proceeds net of fee, each × the txn's fx_to_idr
/// - gain_loss_idr = market − invested
///
/// Only Buy/Sell contribute; other txn types tagged to a goal are ignored.
/// Cost basis is net-cash (not FIFO) — a documented approximation.
pub fn compute_goal_progress(txns: &[Transaction], price_idr: &HashMap<i64, Decimal>) -> GoalProgress {
    let mut net_units: HashMap<i64, Decimal> = HashMap::new();
    let mut invested = Decimal::ZERO;
    for t in txns {
        match t.txn_type {
            TxnType::Buy => {
                *net_units.entry(t.instrument_id).or_default() += t.quantity;
                invested += (t.quantity * t.price_native + t.fee_native) * t.fx_to_idr;
            }
            TxnType::Sell => {
                *net_units.entry(t.instrument_id).or_default() -= t.quantity;
                invested -= (t.quantity * t.price_native - t.fee_native) * t.fx_to_idr;
            }
            _ => {}
        }
    }
    let market_value_idr: Decimal = net_units
        .iter()
        .map(|(iid, units)| price_idr.get(iid).copied().unwrap_or(Decimal::ZERO) * *units)
        .sum();
    GoalProgress {
        market_value_idr,
        invested_idr: invested,
        gain_loss_idr: market_value_idr - invested,
    }
}

/// Whole months from `now` to `target`, clamped to at least 1 (a past or
/// same-month target still shows the full remaining amount as "this month").
pub fn months_until(now: NaiveDate, target: NaiveDate) -> i64 {
    use chrono::Datelike;
    let months = (target.year() - now.year()) * 12 + (target.month() as i32 - now.month() as i32);
    (months as i64).max(1)
}

/// Monthly contribution needed to reach the target from the current value over
/// `months_left` months; 0 once the target is already met.
pub fn required_monthly(target_idr: Decimal, current_idr: Decimal, months_left: i64) -> Decimal {
    let remaining = target_idr - current_idr;
    if remaining <= Decimal::ZERO || months_left <= 0 {
        return Decimal::ZERO;
    }
    remaining / Decimal::from(months_left)
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml goal_progress`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/domain/goal_progress.rs backend/src/domain/mod.rs
git commit -m "feat(goals): pure compute_goal_progress + required-monthly helpers"
```

---

## Task 5: Service — `build_goal_progress`

**Files:**
- Create: `backend/src/service/goals.rs`
- Modify: `backend/src/service/mod.rs` (add `pub mod goals;`)

**Interfaces:**
- Consumes: `crate::repo::{transactions, instruments, prices, dec}`, `crate::domain::models::TxnType`, `crate::domain::goal_progress::{compute_goal_progress, GoalProgress}`.
- Produces: `pub async fn build_goal_progress(db: &Db, goal_id: i64) -> anyhow::Result<GoalProgress>`.
- Price map: for each instrument with net units in the goal's txns — current price = latest quote, else the goal's average buy price for that instrument; native→IDR fx = 1 for IDR instruments, else latest USD/IDR (default 1). Mirrors `build_summary`'s currency handling.

- [ ] **Step 1: Register the module**

In `backend/src/service/mod.rs` add (alongside the other `pub mod` lines, near `pub mod insights;`):
```rust
pub mod goals;
```

- [ ] **Step 2: Write the failing test**

Create `backend/src/service/goals.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, goals, instruments, prices, transactions};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    async fn d(s: &str) -> Decimal { Decimal::from_str(s).unwrap() }

    #[tokio::test]
    async fn build_goal_progress_uses_latest_price_for_market_value() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let goal = goals::create(&db, &goals::NewGoal { label:"Pendidikan".into(), note:None, target_idr:"200000000".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();

        let t = transactions::create(&db, &transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"100".into(), price_native:"9000".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        transactions::set_txn_goal(&db, t.id, Some(goal.id)).await.unwrap();
        prices::upsert_latest(&db, ins.id, d("9500").await, "IDR", "test", "2099-01-01").await.unwrap();

        let p = build_goal_progress(&db, goal.id).await.unwrap();
        assert_eq!(p.market_value_idr, d("950000").await);  // 100 * 9500
        assert_eq!(p.invested_idr, d("900000").await);      // 100 * 9000
        assert_eq!(p.gain_loss_idr, d("50000").await);
    }

    #[tokio::test]
    async fn build_goal_progress_falls_back_to_avg_buy_when_no_price() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"RDX".into(), name:"Reksadana X".into(), instrument_type:"mutual_fund".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(4), note:None }).await.unwrap();
        let goal = goals::create(&db, &goals::NewGoal { label:"G".into(), note:None, target_idr:"1".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();
        let t = transactions::create(&db, &transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"10".into(), price_native:"1000".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        transactions::set_txn_goal(&db, t.id, Some(goal.id)).await.unwrap();
        // No price quote -> fall back to avg buy price (1000), so market ≈ invested.
        let p = build_goal_progress(&db, goal.id).await.unwrap();
        assert_eq!(p.market_value_idr, d("10000").await);
        assert_eq!(p.gain_loss_idr, d("0").await);
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml --lib service::goals`
Expected: FAIL — `build_goal_progress` not found.

- [ ] **Step 4: Implement the service**

Prepend to `backend/src/service/goals.rs`:

```rust
use crate::db::Db;
use crate::domain::goal_progress::{compute_goal_progress, GoalProgress};
use crate::domain::models::TxnType;
use crate::repo::{instruments, prices, transactions};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Compute a goal's progress (market value / invested / P&L) from its tagged
/// transactions. Per-instrument current price in IDR is the latest quote, or the
/// goal's average buy price for that instrument when no quote exists; native→IDR
/// fx is 1 for IDR instruments and the latest USD/IDR otherwise (mirrors build_summary).
pub async fn build_goal_progress(db: &Db, goal_id: i64) -> anyhow::Result<GoalProgress> {
    let txns = transactions::list_by_goal(db, goal_id).await?;

    // Average buy price per instrument (native), for the no-quote fallback.
    let mut buy_value: HashMap<i64, Decimal> = HashMap::new();
    let mut buy_qty: HashMap<i64, Decimal> = HashMap::new();
    for t in &txns {
        if t.txn_type == TxnType::Buy {
            *buy_value.entry(t.instrument_id).or_default() += t.quantity * t.price_native;
            *buy_qty.entry(t.instrument_id).or_default() += t.quantity;
        }
    }

    let usd_idr = prices::latest_fx(db, "USD", "IDR").await?.unwrap_or(Decimal::ONE);

    // Distinct instruments in the goal.
    let mut instrument_ids: Vec<i64> = txns.iter().map(|t| t.instrument_id).collect();
    instrument_ids.sort_unstable();
    instrument_ids.dedup();

    let mut price_idr: HashMap<i64, Decimal> = HashMap::new();
    for iid in instrument_ids {
        let ins = instruments::get(db, iid).await?;
        let price_native = match prices::latest(db, iid).await? {
            Some(lp) => lp.price,
            None => {
                let qty = buy_qty.get(&iid).copied().unwrap_or(Decimal::ZERO);
                if qty.is_zero() { Decimal::ZERO } else { buy_value.get(&iid).copied().unwrap_or(Decimal::ZERO) / qty }
            }
        };
        let fx = if ins.native_currency == "IDR" { Decimal::ONE } else { usd_idr };
        price_idr.insert(iid, price_native * fx);
    }

    Ok(compute_goal_progress(&txns, &price_idr))
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test --manifest-path backend/Cargo.toml --lib service::goals`
Expected: PASS (2 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/service/goals.rs backend/src/service/mod.rs
git commit -m "feat(goals): build_goal_progress service (market value from tagged txns)"
```

---

## Task 6: API — extended `GoalResponse`, `PATCH /goals/:id`, tag endpoint

**Files:**
- Modify: `backend/src/api/goals.rs`
- Modify: `backend/src/api/mod.rs`

**Interfaces:**
- Consumes: `repo::goals::{list, get, create, update, delete, NewGoal, UpdateGoal, GoalRow}`, `repo::transactions::set_txn_goal`, `service::insights::build_insights`, `service::goals::build_goal_progress`, `AppError`.
- Produces:
  - Extended `GoalResponse` with `invested_idr: Option<String>`, `gain_loss_idr: Option<String>`, `progress_pct: String`, `target_date: Option<String>`, `required_monthly_idr: Option<String>` (keeps existing `current_idr: String`).
  - `update_goal(State, Path<i64>, Json<UpdateGoal>) -> Json<GoalResponse>`.
  - `set_transaction_goal(State, Path<i64>, Json<TagBody>) -> Json<()>` where `TagBody { goal_id: Option<i64> }`.
  - Routes: `PATCH /goals/:id`, `POST /transactions/:id/goal`.

- [ ] **Step 1: Write the failing test (route protection)**

Add to `backend/src/api/mod.rs` `router_tests`:

```rust
#[serial]
#[tokio::test]
async fn goal_and_tag_routes_are_protected() {
    std::env::set_var("AUTH_PASSWORD", "pw");
    std::env::set_var("JWT_SECRET", "router-test-goals");
    let app = router(test_state().await);
    let cases = [("/goals/1", "PATCH"), ("/transactions/1/goal", "POST")];
    for (uri, method) in cases {
        let res = app.clone().oneshot(
            Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri} should be protected");
    }
    std::env::remove_var("AUTH_PASSWORD");
    std::env::remove_var("JWT_SECRET");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml goal_and_tag_routes_are_protected`
Expected: FAIL — routes 404 (not registered), not 401.

- [ ] **Step 3: Extend `GoalResponse` + per-kind compute in `api/goals.rs`**

Replace the `GoalResponse` struct and `compute_current` with a richer builder. Edit `backend/src/api/goals.rs`:

1. Add imports at the top:
```rust
use crate::repo::transactions;
use crate::service::goals::build_goal_progress;
use crate::domain::goal_progress::{months_until, required_monthly};
use chrono::NaiveDate;
```

2. Replace the `GoalResponse` struct with:
```rust
#[derive(Debug, Serialize)]
pub struct GoalResponse {
    pub id: i64,
    pub label: String,
    pub note: Option<String>,
    pub target_idr: String,
    pub current_kind: String,
    pub current_manual_idr: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    pub target_date: Option<String>,
    /// Computed current value in IDR (market value for 'tagged').
    pub current_idr: String,
    /// 'tagged' only: net invested capital in IDR.
    pub invested_idr: Option<String>,
    /// 'tagged' only: market value − invested.
    pub gain_loss_idr: Option<String>,
    /// current / target × 100 (0 when target is 0).
    pub progress_pct: String,
    /// Monthly contribution needed to hit target by target_date (None if no date).
    pub required_monthly_idr: Option<String>,
}
```

3. Replace `compute_current` with an async per-goal builder that does the kind dispatch:
```rust
async fn build_goal_response(
    s: &AppState,
    g: goals::GoalRow,
    liquid: Decimal,
    net_worth: Decimal,
) -> Result<GoalResponse, AppError> {
    // current_idr + invested/gain depend on the kind.
    let (current, invested, gain): (Decimal, Option<Decimal>, Option<Decimal>) = match g.current_kind.as_str() {
        "cash" => (liquid, None, None),
        "networth" => (net_worth, None, None),
        "manual" => (
            g.current_manual_idr.as_deref().map(crate::repo::dec).transpose().map_err(AppError::Other)?.unwrap_or(Decimal::ZERO),
            None, None,
        ),
        "tagged" => {
            let p = build_goal_progress(&s.db, g.id).await.map_err(AppError::Other)?;
            (p.market_value_idr, Some(p.invested_idr), Some(p.gain_loss_idr))
        }
        _ => (Decimal::ZERO, None, None),
    };

    let target = crate::repo::dec(&g.target_idr).map_err(AppError::Other)?;
    let progress_pct = if target.is_zero() { Decimal::ZERO } else { current / target * Decimal::from(100) };

    let required_monthly_idr = match g.target_date.as_deref() {
        Some(d) => match NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            Ok(td) => {
                let months = months_until(chrono::Utc::now().date_naive(), td);
                Some(required_monthly(target, current, months).to_string())
            }
            Err(_) => None, // unparseable date -> no projection rather than a 500
        },
        None => None,
    };

    Ok(GoalResponse {
        id: g.id,
        label: g.label,
        note: g.note,
        target_idr: g.target_idr,
        current_kind: g.current_kind,
        current_manual_idr: g.current_manual_idr,
        sort_order: g.sort_order,
        created_at: g.created_at,
        target_date: g.target_date,
        current_idr: current.to_string(),
        invested_idr: invested.map(|v| v.to_string()),
        gain_loss_idr: gain.map(|v| v.to_string()),
        progress_pct: progress_pct.to_string(),
        required_monthly_idr,
    })
}
```

4. Update `list_goals` to use the builder (it already fetches `insights`):
```rust
pub async fn list_goals(State(s): State<AppState>) -> Result<Json<Vec<GoalResponse>>, AppError> {
    let goal_rows = goals::list(&s.db).await.map_err(AppError::Other)?;
    if goal_rows.is_empty() {
        return Ok(Json(vec![]));
    }
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let (liquid, net_worth) = (insights.liquid_idr, insights.net_worth_idr);
    let mut responses = Vec::with_capacity(goal_rows.len());
    for g in goal_rows {
        responses.push(build_goal_response(&s, g, liquid, net_worth).await?);
    }
    Ok(Json(responses))
}
```

5. Update `create_goal` to use the builder:
```rust
pub async fn create_goal(
    State(s): State<AppState>,
    Json(body): Json<goals::NewGoal>,
) -> Result<Json<GoalResponse>, AppError> {
    let row = goals::create(&s.db, &body).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let resp = build_goal_response(&s, row, insights.liquid_idr, insights.net_worth_idr).await?;
    Ok(Json(resp))
}
```

6. Add the `update_goal` handler and the tag handler:
```rust
pub async fn update_goal(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<goals::UpdateGoal>,
) -> Result<Json<GoalResponse>, AppError> {
    goals::get(&s.db, id).await.map_err(|_| AppError::NotFound)?;
    let row = goals::update(&s.db, id, &body).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let resp = build_goal_response(&s, row, insights.liquid_idr, insights.net_worth_idr).await?;
    Ok(Json(resp))
}

#[derive(Debug, serde::Deserialize)]
pub struct TagBody {
    /// Goal to tag the transaction to; null clears the tag.
    pub goal_id: Option<i64>,
}

pub async fn set_transaction_goal(
    State(s): State<AppState>,
    Path(txn_id): Path<i64>,
    Json(body): Json<TagBody>,
) -> Result<Json<()>, AppError> {
    if let Some(gid) = body.goal_id {
        goals::get(&s.db, gid).await.map_err(|_| AppError::BadRequest(format!("unknown goal_id {gid}")))?;
    }
    transactions::set_txn_goal(&s.db, txn_id, body.goal_id)
        .await
        .map_err(|_| AppError::NotFound)?;
    Ok(Json(()))
}
```

> Keep `Decimal` and the existing `use` lines; remove the now-unused old `compute_current` fn.

- [ ] **Step 4: Register the routes in `backend/src/api/mod.rs`**

Find the existing goals routes and extend them. Replace:
```rust
        .route("/goals", get(goals::list_goals).post(goals::create_goal))
        .route("/goals/:id", delete(goals::delete_goal))
```
with:
```rust
        .route("/goals", get(goals::list_goals).post(goals::create_goal))
        .route(
            "/goals/:id",
            delete(goals::delete_goal).patch(goals::update_goal),
        )
        .route("/transactions/:id/goal", post(goals::set_transaction_goal))
```

- [ ] **Step 5: Run the protection test + a behavior check**

Run: `cargo test --manifest-path backend/Cargo.toml goal_and_tag_routes_are_protected`
Expected: PASS.

Run: `cargo test --manifest-path backend/Cargo.toml --lib api::goals`
Expected: PASS (compiles; any existing goal api tests stay green).

- [ ] **Step 6: Commit**

```bash
git add backend/src/api/goals.rs backend/src/api/mod.rs
git commit -m "feat(goals): PATCH /goals/:id, tag endpoint, progress-rich GoalResponse"
```

---

## Task 7: Assistant tools — `list_goals` + `tag_transaction_to_goal`

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

**Interfaces:**
- Consumes: existing dispatcher helpers `str_arg`, `id_arg`, `optional_id`; `crate::repo::{goals, transactions}`; the `dispatch` match and `definitions()` JSON array.
- Produces: two new tools wired into `definitions()` and `dispatch()`:
  - `list_goals` — lists goals (id, label, target, kind) so the model can pick one.
  - `tag_transaction_to_goal { transaction_id: int, goal_id?: int }` — tags (or with `goal_id` omitted/null, untags) a transaction.

- [ ] **Step 1: Write the failing tests (dispatcher handlers)**

Add a test module entry in `backend/src/assistant/dispatcher.rs` `#[cfg(test)] mod tests` (or create one mirroring existing dispatcher tests). Use the real in-memory db + `dispatch`:

```rust
    #[tokio::test]
    async fn tag_transaction_to_goal_tags_and_untags() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let goal = crate::repo::goals::create(&db, &crate::repo::goals::NewGoal { label:"Pendidikan".into(), note:None, target_idr:"200000000".into(), current_kind:"tagged".into(), current_manual_idr:None, sort_order:None, target_date:None }).await.unwrap();
        let t = crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"100".into(), price_native:"9000".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();

        let out = dispatch(&db, "tag_transaction_to_goal", &serde_json::json!({ "transaction_id": t.id, "goal_id": goal.id })).await.unwrap();
        assert!(out.to_lowercase().contains("pendidikan"));
        assert_eq!(crate::repo::transactions::list_by_goal(&db, goal.id).await.unwrap().len(), 1);

        // Untag (omit goal_id).
        dispatch(&db, "tag_transaction_to_goal", &serde_json::json!({ "transaction_id": t.id })).await.unwrap();
        assert!(crate::repo::transactions::list_by_goal(&db, goal.id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn tag_transaction_to_goal_rejects_unknown_goal() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument { symbol:"BBCA".into(), name:"BCA".into(), instrument_type:"stock_id".into(), native_currency:"IDR".into(), category_id:None, price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
        let t = crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction { account_id:acc.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"1".into(), price_native:"1".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        assert!(dispatch(&db, "tag_transaction_to_goal", &serde_json::json!({ "transaction_id": t.id, "goal_id": 999 })).await.is_err());
    }
```

> If the dispatcher test module needs imports, mirror the existing dispatcher tests' `use super::*;` plus `crate::repo::*` references shown above.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml tag_transaction_to_goal`
Expected: FAIL — `unknown tool: tag_transaction_to_goal` (dispatch returns Err) / test assertions fail.

- [ ] **Step 3: Add the tool definitions**

In `backend/src/assistant/tools.rs` `definitions()`, add these two objects to the JSON array (next to the instrument tools):

```rust
        {
            "name": "list_goals",
            "description": "List the owner's financial goals (id, label, target in IDR, and how progress is tracked). Use this to find a goal's id before tagging a transaction to it.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "tag_transaction_to_goal",
            "description": "Tag a transaction so its holdings count toward a goal's progress (for goals tracked as 'tagged'). Get transaction_id from list_transactions and goal_id from list_goals. Omit goal_id (or pass null) to UNTAG the transaction. One transaction belongs to at most one goal. Echo the change and confirm before calling — this writes data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "transaction_id": { "type": "integer", "description": "Transaction id from list_transactions." },
                    "goal_id": { "type": "integer", "description": "Goal id from list_goals; omit/null to untag." }
                },
                "required": ["transaction_id"]
            }
        },
```

- [ ] **Step 4: Add the dispatch arms + handlers**

In `backend/src/assistant/dispatcher.rs` `dispatch()` match, add (near the instrument arms):
```rust
        "list_goals" => list_goals(db).await,
        "tag_transaction_to_goal" => tag_transaction_to_goal(db, input).await,
```

Add the handler functions (near `edit_instrument`):
```rust
async fn list_goals(db: &Db) -> Result<String, String> {
    let goals = crate::repo::goals::list(db).await.map_err(|e| format!("db error: {e}"))?;
    if goals.is_empty() {
        return Ok("belum ada goal".to_string());
    }
    let lines: Vec<String> = goals
        .iter()
        .map(|g| format!("#{} {} — target Rp{} ({})", g.id, g.label, g.target_idr, g.current_kind))
        .collect();
    Ok(lines.join("\n"))
}

async fn tag_transaction_to_goal(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let txn_id = id_arg(input, "transaction_id")?;
    let goal_id = optional_id(input, "goal_id")?;
    if let Some(gid) = goal_id {
        crate::repo::goals::get(db, gid).await.map_err(|_| format!("goal #{gid} nggak ada"))?;
    }
    crate::repo::transactions::set_txn_goal(db, txn_id, goal_id)
        .await
        .map_err(|_| format!("transaksi #{txn_id} nggak ada"))?;
    match goal_id {
        Some(gid) => {
            let g = crate::repo::goals::get(db, gid).await.map_err(|e| format!("db error: {e}"))?;
            Ok(format!("transaksi #{txn_id} di-tag ke goal '{}'", g.label))
        }
        None => Ok(format!("transaksi #{txn_id} dilepas dari goal")),
    }
}
```

> Confirm the exact return type of `dispatch` and the signatures of `id_arg`/`optional_id` against the file (the explorer reported `id_arg(input, "id") -> Result<i64, String>` and `optional_id(input, key) -> Result<Option<i64>, String>`). Match them exactly.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml tag_transaction_to_goal`
Expected: PASS.

- [ ] **Step 6: Full backend suite + clippy**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: PASS (whole suite).

Run: `cargo clippy --manifest-path backend/Cargo.toml --all-targets`
Expected: no new warnings in files this plan touched.

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(goals): assistant list_goals + tag_transaction_to_goal tools"
```

---

## Done criteria (Phase 3)

- A transaction can be tagged to a goal (`txn.goal_id`), via `POST /transactions/:id/goal` and via the `tag_transaction_to_goal` chat tool; one txn → at most one goal.
- Goals support `current_kind='tagged'`: `GET /goals` / `POST /goals` / `PATCH /goals/:id` return market value (`current_idr`), `invested_idr`, `gain_loss_idr`, `progress_pct`, `target_date`, and `required_monthly_idr` (when a target date is set).
- `cash`/`networth`/`manual` goals keep their existing behavior; the new response fields are populated sensibly (P&L null for non-tagged; progress/required-monthly computed).
- The domain `Transaction` model is unchanged; all prior tests stay green; full suite + clippy clean.

## Deliberately deferred (note to reviewer — not gaps)

- **FIFO cost basis** — `invested_idr` uses net-cash (Σ buy − Σ sell), a documented approximation; fine for goal progress.
- **Tagging a transaction at creation time** — not added to `NewTransaction`; tagging is a separate step (endpoint/tool). Can be added later if needed.
- **fx-gap backfill for invested** — `invested_idr` uses each txn's stored `fx_to_idr` (1 for IDR txns); historical non-IDR txns with a zero stored rate are not back-filled here (build_summary's `resolve_fx_gaps` is portfolio-specific). Acceptable for a goal-progress view; documented.
- **Per-instrument breakdown array on `GoalResponse`** — the spec lists a "contributing instruments" breakdown for the goal card. It is intentionally NOT built here: its exact shape (symbol, units, market value, invested per instrument) is best designed alongside the Phase 4 goal-card UI that renders it. The core progress totals ship now; the breakdown is a Phase-4 add-on (the data already lives in the tagged txns). Likewise the spec's `current_market_idr` is served by the existing `current_idr` field (= market value for `tagged`, per the spec note), not a second field.
- **Phase 4** (goals frontend: cards + transaction goal selector) is a separate plan.
```
