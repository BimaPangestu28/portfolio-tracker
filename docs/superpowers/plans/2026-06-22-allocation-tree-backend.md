# Allocation Tree — Backend (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a free-depth allocation **tree** (`plan_node`) overlaid on the existing flat `category` model, with a recursive rollup that reports actual vs target per node, plus CRUD + tree API.

**Architecture:** Approach A (overlay) from the design spec. A new `plan_node` adjacency-list table references the existing `category`/`instrument` tables. Targets move from `category.target_pct` to `plan_node.target_pct` (categories migrate to root nodes). A new domain module computes a recursive `PlanNodeAllocation` tree; a service function assembles it by reusing `build_summary`'s per-instrument market values. New axum routes expose CRUD + the computed tree. Frontend is a separate later plan.

**Tech Stack:** Rust, axum, sqlx (SQLite), rust_decimal, anyhow/thiserror, tokio.

**Spec:** `docs/superpowers/specs/2026-06-22-planner-tree-and-goals-design.md`

## Global Constraints

- **No rustfmt.** Never run `cargo fmt`. Match surrounding style by hand. Verify with `cargo test` + `cargo clippy` only.
- **Decimals are TEXT.** All money/percent columns are stored as `TEXT` and parsed with `crate::repo::dec(&str) -> anyhow::Result<Decimal>`. Never use floats for money.
- **No `unwrap()`/`panic!()`/`expect()` in non-test code.** Propagate with `?` and `anyhow`/`AppError`. `unwrap()` is fine inside `#[cfg(test)]`.
- **Migration versioning.** New migration files go in `backend/migrations/`, named `NNNN_<name>.sql` with a unique numeric prefix. Highest existing is `0029`; use `0030`. The `db::tests::migration_versions_are_unique` test enforces uniqueness.
- **Migrations run automatically** via `sqlx::migrate!("./migrations")` inside `db::connect`. No manual registration. Tests use `crate::db::connect("sqlite::memory:")`.
- **Repo conventions:** one module per table under `backend/src/repo/`, registered in `backend/src/repo/mod.rs`. Pattern: `XxxRow` (sqlx::FromRow + Serialize), `NewXxx`/`UpdateXxx` (Deserialize), free `create/get/list/delete/update` fns taking `&Db`, `#[cfg(test)] mod tests` with in-memory db.
- **Conventional commits** (`feat:`/`fix:`/`refactor:`). Commit after every green test cycle.
- **Test command (run from repo root):** `cargo test --manifest-path backend/Cargo.toml`. Scope to a module with e.g. `cargo test --manifest-path backend/Cargo.toml plan_nodes`.
- No new crate dependencies are required; do not touch `Cargo.toml`/`Cargo.lock`.

---

## File structure

- **Create** `backend/migrations/0030_plan_tree.sql` — `plan_node` table + data migration from `category`.
- **Create** `backend/src/repo/plan_nodes.rs` — `plan_node` repo (CRUD + reparent/move + validation).
- **Create** `backend/src/domain/plan_alloc.rs` — pure recursive rollup (`compute_plan_tree`).
- **Create** `backend/src/api/plan.rs` — axum handlers for `/plan/*`.
- **Modify** `backend/src/repo/mod.rs` — add `pub mod plan_nodes;`.
- **Modify** `backend/src/domain/mod.rs` — add `pub mod plan_alloc;`.
- **Modify** `backend/src/api/mod.rs` — add `pub mod plan;` + route registrations.
- **Modify** `backend/src/service/portfolio.rs` — add `build_plan_tree(db)`.

---

## Task 1: Migration — `plan_node` table + category backfill

**Files:**
- Create: `backend/migrations/0030_plan_tree.sql`
- Test: `backend/src/repo/plan_nodes.rs` (a migration-backfill test lives with the repo, added in Task 2; this task's verification is the existing `migration_versions_are_unique` test + a compile/boot check)

**Interfaces:**
- Produces: table `plan_node(id, parent_id, name, target_pct, tolerance_band_pct, bind_kind, category_id, instrument_id, sort_order, color)`. After migration, one root `plan_node` per existing `category` row.

- [ ] **Step 1: Write the migration SQL**

Create `backend/migrations/0030_plan_tree.sql`:

```sql
CREATE TABLE plan_node (
  id                 INTEGER PRIMARY KEY AUTOINCREMENT,
  parent_id          INTEGER REFERENCES plan_node(id) ON DELETE CASCADE,
  name               TEXT NOT NULL,
  target_pct         TEXT NOT NULL,
  tolerance_band_pct TEXT,
  bind_kind          TEXT NOT NULL,                       -- 'group' | 'category' | 'instrument'
  category_id        INTEGER REFERENCES category(id),
  instrument_id      INTEGER REFERENCES instrument(id),
  sort_order         INTEGER NOT NULL DEFAULT 0,
  color              TEXT
);
CREATE INDEX idx_plan_node_parent ON plan_node(parent_id);

-- Backfill: every existing category becomes a root node bound to that category,
-- carrying its target/tolerance/color/order. category.target_pct is now deprecated
-- (kept for rollback safety) and no longer read for targets.
INSERT INTO plan_node (parent_id, name, target_pct, tolerance_band_pct, bind_kind, category_id, instrument_id, sort_order, color)
SELECT NULL, name, target_pct, tolerance_band_pct, 'category', id, NULL, sort_order, color
FROM category;
```

- [ ] **Step 2: Verify it boots and migration versions stay unique**

Run: `cargo test --manifest-path backend/Cargo.toml migration_versions_are_unique`
Expected: PASS (no duplicate `0030`).

Run: `cargo test --manifest-path backend/Cargo.toml --lib db::`
Expected: PASS — `connect("sqlite::memory:")` runs all migrations including `0030` without error.

- [ ] **Step 3: Commit**

```bash
git add backend/migrations/0030_plan_tree.sql
git commit -m "feat(planner): add plan_node tree table + backfill from category"
```

---

## Task 2: `plan_node` repo — CRUD + validation + backfill test

**Files:**
- Create: `backend/src/repo/plan_nodes.rs`
- Modify: `backend/src/repo/mod.rs` (add `pub mod plan_nodes;`)

**Interfaces:**
- Consumes: `crate::db::Db`, `crate::repo::dec`.
- Produces:
  - `pub struct PlanNodeRow { id: i64, parent_id: Option<i64>, name: String, target_pct: String, tolerance_band_pct: Option<String>, bind_kind: String, category_id: Option<i64>, instrument_id: Option<i64>, sort_order: i64, color: Option<String> }`
  - `pub struct NewPlanNode { parent_id: Option<i64>, name: String, target_pct: String, tolerance_band_pct: Option<String>, bind_kind: String, category_id: Option<i64>, instrument_id: Option<i64>, sort_order: Option<i64>, color: Option<String> }`
  - `pub struct UpdatePlanNode { name: Option<String>, target_pct: Option<String>, tolerance_band_pct: Option<String>, sort_order: Option<i64>, color: Option<String> }`
  - `pub struct MovePlanNode { parent_id: Option<i64>, sort_order: i64 }`
  - `async fn create(&Db, &NewPlanNode) -> anyhow::Result<PlanNodeRow>`
  - `async fn get(&Db, i64) -> anyhow::Result<PlanNodeRow>`
  - `async fn list(&Db) -> anyhow::Result<Vec<PlanNodeRow>>`
  - `async fn update(&Db, i64, &UpdatePlanNode) -> anyhow::Result<PlanNodeRow>`
  - `async fn delete(&Db, i64) -> anyhow::Result<()>`
  - `async fn move_node(&Db, i64, &MovePlanNode) -> anyhow::Result<PlanNodeRow>`
  - `fn validate_bind(bind_kind, category_id, instrument_id) -> anyhow::Result<()>` (module-private)

- [ ] **Step 1: Register the module**

In `backend/src/repo/mod.rs`, add alongside the other `pub mod` lines (keep alphabetical-ish grouping near `categories`):

```rust
pub mod plan_nodes;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/repo/plan_nodes.rs` with only the test module first (the types/fns are added in Step 4; tests fail to compile until then, which counts as failing):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> crate::db::Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_and_list_round_trip() {
        // NB: the migration backfill runs at connect() time against whatever
        // categories exist then. A fresh in-memory test DB has none, so backfill
        // produces no rows here — backfill is verified by the migration booting
        // cleanly (db:: tests) and by prod data, not by this unit test. Here we
        // create a category first so the category_id FK is satisfied (foreign_keys=ON).
        let db = mem_db().await;
        let cat = crate::repo::categories::create(&db, &crate::repo::categories::NewCategory {
            name: "Saham IDX".into(), target_pct: "30".into(),
            tolerance_band_pct: Some("5".into()), sort_order: Some(1), color: None,
        }).await.unwrap();
        let made = create(&db, &NewPlanNode {
            parent_id: None, name: "Saham".into(), target_pct: "30".into(),
            tolerance_band_pct: Some("5".into()), bind_kind: "category".into(),
            category_id: Some(cat.id), instrument_id: None, sort_order: Some(0), color: None,
        }).await.unwrap();
        assert_eq!(made.bind_kind, "category");
        assert_eq!(made.category_id, Some(cat.id));
        let all = list(&db).await.unwrap();
        assert!(all.iter().any(|n| n.id == made.id));
    }

    #[tokio::test]
    async fn rejects_instrument_bind_without_instrument_id() {
        let db = mem_db().await;
        let r = create(&db, &NewPlanNode {
            parent_id: None, name: "Bad".into(), target_pct: "10".into(),
            tolerance_band_pct: None, bind_kind: "instrument".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn rejects_group_bind_with_a_binding() {
        let db = mem_db().await;
        let r = create(&db, &NewPlanNode {
            parent_id: None, name: "Bad".into(), target_pct: "10".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: Some(1), instrument_id: None, sort_order: None, color: None,
        }).await;
        assert!(r.is_err());
    }

    #[tokio::test]
    async fn update_changes_target_keeping_other_fields() {
        let db = mem_db().await;
        let n = create(&db, &NewPlanNode {
            parent_id: None, name: "Saham".into(), target_pct: "30".into(),
            tolerance_band_pct: Some("5".into()), bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        let u = update(&db, n.id, &UpdatePlanNode {
            name: None, target_pct: Some("40".into()),
            tolerance_band_pct: None, sort_order: None, color: None,
        }).await.unwrap();
        assert_eq!(u.target_pct, "40");
        assert_eq!(u.name, "Saham");
        assert_eq!(u.tolerance_band_pct.as_deref(), Some("5"));
    }

    #[tokio::test]
    async fn delete_cascades_to_children() {
        let db = mem_db().await;
        let root = create(&db, &NewPlanNode {
            parent_id: None, name: "Saham".into(), target_pct: "30".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        let child = create(&db, &NewPlanNode {
            parent_id: Some(root.id), name: "BBCA".into(), target_pct: "40".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        delete(&db, root.id).await.unwrap();
        let all = list(&db).await.unwrap();
        assert!(!all.iter().any(|n| n.id == root.id || n.id == child.id));
    }

    #[tokio::test]
    async fn move_rejects_cycle() {
        let db = mem_db().await;
        let a = create(&db, &NewPlanNode {
            parent_id: None, name: "A".into(), target_pct: "50".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        let b = create(&db, &NewPlanNode {
            parent_id: Some(a.id), name: "B".into(), target_pct: "50".into(),
            tolerance_band_pct: None, bind_kind: "group".into(),
            category_id: None, instrument_id: None, sort_order: None, color: None,
        }).await.unwrap();
        // Making A a child of B would create a cycle (B is A's descendant).
        let r = move_node(&db, a.id, &MovePlanNode { parent_id: Some(b.id), sort_order: 0 }).await;
        assert!(r.is_err());
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml plan_nodes`
Expected: FAIL — `cannot find function create`/types not defined (compile error).

- [ ] **Step 4: Implement the repo**

Prepend to `backend/src/repo/plan_nodes.rs` (above the test module):

```rust
use crate::db::Db;
use crate::repo::dec;
use serde::{Deserialize, Serialize};

const VALID_BIND_KINDS: &[&str] = &["group", "category", "instrument"];

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlanNodeRow {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub bind_kind: String,
    pub category_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub sort_order: i64,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewPlanNode {
    pub parent_id: Option<i64>,
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub bind_kind: String,
    pub category_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub sort_order: Option<i64>,
    pub color: Option<String>,
}

/// Partial update; absent fields keep their current values. Binding fields
/// (bind_kind/category_id/instrument_id) are intentionally immutable — change a
/// node's binding by deleting and recreating it.
#[derive(Debug, Deserialize)]
pub struct UpdatePlanNode {
    pub name: Option<String>,
    pub target_pct: Option<String>,
    pub tolerance_band_pct: Option<String>,
    pub sort_order: Option<i64>,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MovePlanNode {
    pub parent_id: Option<i64>,
    pub sort_order: i64,
}

/// Enforce the bind_kind invariants:
/// - 'instrument' => instrument_id set, category_id null
/// - 'category'   => category_id set, instrument_id null
/// - 'group'      => both null
fn validate_bind(bind_kind: &str, category_id: Option<i64>, instrument_id: Option<i64>) -> anyhow::Result<()> {
    if !VALID_BIND_KINDS.contains(&bind_kind) {
        anyhow::bail!("bind_kind must be one of {VALID_BIND_KINDS:?}, got '{bind_kind}'");
    }
    match bind_kind {
        "instrument" if instrument_id.is_none() || category_id.is_some() =>
            anyhow::bail!("bind_kind='instrument' requires instrument_id and no category_id"),
        "category" if category_id.is_none() || instrument_id.is_some() =>
            anyhow::bail!("bind_kind='category' requires category_id and no instrument_id"),
        "group" if category_id.is_some() || instrument_id.is_some() =>
            anyhow::bail!("bind_kind='group' must not set category_id or instrument_id"),
        _ => Ok(()),
    }
}

pub async fn create(db: &Db, n: &NewPlanNode) -> anyhow::Result<PlanNodeRow> {
    validate_bind(&n.bind_kind, n.category_id, n.instrument_id)?;
    dec(&n.target_pct)?;
    if let Some(t) = n.tolerance_band_pct.as_deref() { dec(t)?; }
    let id = sqlx::query(
        "INSERT INTO plan_node (parent_id, name, target_pct, tolerance_band_pct, bind_kind, category_id, instrument_id, sort_order, color)
         VALUES (?,?,?,?,?,?,?,?,?)",
    )
    .bind(n.parent_id).bind(&n.name).bind(&n.target_pct).bind(&n.tolerance_band_pct)
    .bind(&n.bind_kind).bind(n.category_id).bind(n.instrument_id)
    .bind(n.sort_order.unwrap_or(0)).bind(&n.color)
    .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<PlanNodeRow> {
    Ok(sqlx::query_as::<_, PlanNodeRow>("SELECT * FROM plan_node WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<PlanNodeRow>> {
    Ok(sqlx::query_as::<_, PlanNodeRow>("SELECT * FROM plan_node ORDER BY sort_order, id")
        .fetch_all(db).await?)
}

pub async fn update(db: &Db, id: i64, u: &UpdatePlanNode) -> anyhow::Result<PlanNodeRow> {
    let cur = get(db, id).await?;
    if let Some(t) = u.target_pct.as_deref() { dec(t)?; }
    if let Some(t) = u.tolerance_band_pct.as_deref() { dec(t)?; }
    let name = u.name.clone().unwrap_or(cur.name);
    let target_pct = u.target_pct.clone().unwrap_or(cur.target_pct);
    let tolerance = u.tolerance_band_pct.clone().or(cur.tolerance_band_pct);
    let sort_order = u.sort_order.unwrap_or(cur.sort_order);
    let color = u.color.clone().or(cur.color);
    sqlx::query("UPDATE plan_node SET name=?, target_pct=?, tolerance_band_pct=?, sort_order=?, color=? WHERE id=?")
        .bind(&name).bind(&target_pct).bind(&tolerance).bind(sort_order).bind(&color).bind(id)
        .execute(db).await?;
    get(db, id).await
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // ON DELETE CASCADE (foreign_keys pragma is ON) removes the subtree.
    sqlx::query("DELETE FROM plan_node WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

/// Reparent and/or reorder a node. Rejects a move that would put the node under
/// one of its own descendants (cycle), which SQLite would otherwise allow.
pub async fn move_node(db: &Db, id: i64, m: &MovePlanNode) -> anyhow::Result<PlanNodeRow> {
    get(db, id).await?; // 404 surfaces as RowNotFound -> caller maps to NotFound
    if let Some(new_parent) = m.parent_id {
        if new_parent == id {
            anyhow::bail!("a node cannot be its own parent");
        }
        // Walk up from the proposed parent; if we reach `id`, it's a cycle.
        let rows = list(db).await?;
        let parent_of: std::collections::HashMap<i64, Option<i64>> =
            rows.iter().map(|r| (r.id, r.parent_id)).collect();
        let mut cur = Some(new_parent);
        while let Some(c) = cur {
            if c == id {
                anyhow::bail!("move would create a cycle");
            }
            cur = parent_of.get(&c).copied().flatten();
        }
    }
    sqlx::query("UPDATE plan_node SET parent_id=?, sort_order=? WHERE id=?")
        .bind(m.parent_id).bind(m.sort_order).bind(id)
        .execute(db).await?;
    get(db, id).await
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml plan_nodes`
Expected: PASS (all 6 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/repo/plan_nodes.rs backend/src/repo/mod.rs
git commit -m "feat(planner): plan_node repo with bind validation + cycle-safe move"
```

---

## Task 3: Domain — recursive `compute_plan_tree`

**Files:**
- Create: `backend/src/domain/plan_alloc.rs`
- Modify: `backend/src/domain/mod.rs` (add `pub mod plan_alloc;`)

**Interfaces:**
- Consumes: `rust_decimal::Decimal`, `std::collections::HashMap`.
- Produces:
  - `pub struct PlanNodeInput { id, parent_id: Option<i64>, name: String, target_pct: Decimal, tolerance_band_pct: Option<Decimal>, bind_kind: String, category_id: Option<i64>, instrument_id: Option<i64>, sort_order: i64, color: Option<String> }`
  - `pub struct PlanNodeAllocation { id: i64, name: String, bind_kind: String, target_pct: Decimal, tolerance_band_pct: Option<Decimal>, actual_pct: Decimal, actual_value_idr: Decimal, target_value_idr: Decimal, drift_pct: Decimal, out_of_band: bool, rebalance_idr: Decimal, color: Option<String>, children: Vec<PlanNodeAllocation> }`
  - `pub fn compute_plan_tree(nodes: &[PlanNodeInput], instrument_value: &HashMap<i64, Decimal>, instrument_category: &HashMap<i64, Option<i64>>, total_idr: Decimal) -> Vec<PlanNodeAllocation>`
- Synthetic "Lainnya" nodes use id `-1` (root remainder) and `-2 - category_id` (per-category remainder) so they never collide with real ids.

- [ ] **Step 1: Register the module**

In `backend/src/domain/mod.rs`, add (near `allocation`):

```rust
pub mod plan_alloc;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/domain/plan_alloc.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn n(id: i64, parent: Option<i64>, name: &str, target: Decimal, tol: Option<Decimal>, kind: &str, cat: Option<i64>, ins: Option<i64>) -> PlanNodeInput {
        PlanNodeInput {
            id, parent_id: parent, name: name.into(), target_pct: target,
            tolerance_band_pct: tol, bind_kind: kind.into(),
            category_id: cat, instrument_id: ins, sort_order: 0, color: None,
        }
    }

    #[test]
    fn category_root_with_instrument_child_and_lainnya() {
        // Portfolio = 100 IDR. Category "Saham" (id=1) total = 20 (BBCA 12 + BBRI 8).
        // Uncategorized = 80.
        let nodes = vec![
            n(1, None, "Saham", dec!(30), Some(dec!(5)), "category", Some(1), None),
            n(2, Some(1), "BBCA", dec!(40), None, "instrument", None, Some(10)),
        ];
        let mut iv = std::collections::HashMap::new();
        iv.insert(10, dec!(12)); // BBCA
        iv.insert(11, dec!(8));  // BBRI (in Saham, not broken out)
        iv.insert(99, dec!(80)); // uncategorized
        let mut ic = std::collections::HashMap::new();
        ic.insert(10, Some(1));
        ic.insert(11, Some(1));
        ic.insert(99, None);

        let tree = compute_plan_tree(&nodes, &iv, &ic, dec!(100));

        // Roots: Saham + synthetic root "Lainnya" (80).
        let saham = tree.iter().find(|x| x.id == 1).unwrap();
        assert_eq!(saham.actual_value_idr, dec!(20));
        assert_eq!(saham.actual_pct, dec!(20));      // 20/100
        assert_eq!(saham.target_value_idr, dec!(30)); // 30% of 100
        assert_eq!(saham.drift_pct, dec!(-10));       // 20 - 30
        assert!(saham.out_of_band);                   // |10| > 5
        assert_eq!(saham.rebalance_idr, dec!(10));    // 30 - 20

        // BBCA child: 12 of Saham's 20 = 60% (vs 40% target).
        let bbca = saham.children.iter().find(|x| x.id == 2).unwrap();
        assert_eq!(bbca.actual_value_idr, dec!(12));
        assert_eq!(bbca.actual_pct, dec!(60));
        assert_eq!(bbca.target_value_idr, dec!(12)); // 40% of Saham target 30
        assert_eq!(bbca.drift_pct, dec!(20));

        // Synthetic "Lainnya" under Saham: 20 - 12 = 8.
        let saham_lain = saham.children.iter().find(|x| x.actual_value_idr == dec!(8)).unwrap();
        assert_eq!(saham_lain.name, "Lainnya");
        assert_eq!(saham_lain.actual_pct, dec!(40)); // 8/20
        assert!(!saham_lain.out_of_band);

        // Root "Lainnya": 100 - 20 = 80.
        let root_lain = tree.iter().find(|x| x.id == -1).unwrap();
        assert_eq!(root_lain.actual_value_idr, dec!(80));
        assert_eq!(root_lain.actual_pct, dec!(80));
        assert!(!root_lain.out_of_band);

        // Whole tree reconciles to net worth.
        let root_total: Decimal = tree.iter().map(|x| x.actual_value_idr).sum();
        assert_eq!(root_total, dec!(100));
    }

    #[test]
    fn group_node_sums_children() {
        // Group "Equity" with two instrument children; no category binding.
        let nodes = vec![
            n(1, None, "Equity", dec!(100), None, "group", None, None),
            n(2, Some(1), "A", dec!(50), None, "instrument", None, Some(10)),
            n(3, Some(1), "B", dec!(50), None, "instrument", None, Some(11)),
        ];
        let mut iv = std::collections::HashMap::new();
        iv.insert(10, dec!(30));
        iv.insert(11, dec!(70));
        let ic = std::collections::HashMap::new(); // categories irrelevant for group/instrument
        let tree = compute_plan_tree(&nodes, &iv, &ic, dec!(100));
        let equity = &tree[0];
        assert_eq!(equity.actual_value_idr, dec!(100)); // 30 + 70
        // Group nodes get NO synthetic Lainnya (only category nodes do).
        assert_eq!(equity.children.len(), 2);
    }

    #[test]
    fn empty_portfolio_is_zero_not_panic() {
        let nodes = vec![n(1, None, "Saham", dec!(100), None, "category", Some(1), None)];
        let iv = std::collections::HashMap::new();
        let ic = std::collections::HashMap::new();
        let tree = compute_plan_tree(&nodes, &iv, &ic, dec!(0));
        assert_eq!(tree[0].actual_value_idr, dec!(0));
        assert_eq!(tree[0].actual_pct, dec!(0));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --manifest-path backend/Cargo.toml plan_alloc`
Expected: FAIL — types/`compute_plan_tree` not defined (compile error).

- [ ] **Step 4: Implement the rollup**

Prepend to `backend/src/domain/plan_alloc.rs`:

```rust
use rust_decimal::Decimal;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct PlanNodeInput {
    pub id: i64,
    pub parent_id: Option<i64>,
    pub name: String,
    pub target_pct: Decimal,
    pub tolerance_band_pct: Option<Decimal>,
    pub bind_kind: String,
    pub category_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub sort_order: i64,
    pub color: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct PlanNodeAllocation {
    pub id: i64,
    pub name: String,
    pub bind_kind: String,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str_option")]
    pub tolerance_band_pct: Option<Decimal>,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_pct: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub actual_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub target_value_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub drift_pct: Decimal,
    pub out_of_band: bool,
    #[serde(with = "rust_decimal::serde::str")]
    pub rebalance_idr: Decimal,
    pub color: Option<String>,
    pub children: Vec<PlanNodeAllocation>,
}

/// Compute the recursive allocation tree.
///
/// - instrument leaf => its market value.
/// - group node      => sum of children.
/// - category node   => total IDR of all instruments in that category; explicit
///   children break it down and the unclaimed remainder surfaces as a synthetic
///   "Lainnya" child.
/// Percentages and drift are computed RELATIVE TO THE PARENT (root parent = total).
pub fn compute_plan_tree(
    nodes: &[PlanNodeInput],
    instrument_value: &HashMap<i64, Decimal>,
    instrument_category: &HashMap<i64, Option<i64>>,
    total_idr: Decimal,
) -> Vec<PlanNodeAllocation> {
    // children index
    let mut children: HashMap<i64, Vec<&PlanNodeInput>> = HashMap::new();
    let mut roots: Vec<&PlanNodeInput> = Vec::new();
    for node in nodes {
        match node.parent_id {
            Some(p) => children.entry(p).or_default().push(node),
            None => roots.push(node),
        }
    }
    let ctx = Ctx { children, instrument_value, instrument_category };

    let mut out: Vec<PlanNodeAllocation> = roots
        .iter()
        .map(|r| build(r, total_idr, total_idr, &ctx))
        .collect();
    // Root-level "Lainnya": everything not covered by a root node.
    let claimed: Decimal = out.iter().map(|x| x.actual_value_idr).sum();
    let remainder = total_idr - claimed;
    if remainder > Decimal::ZERO {
        out.push(lainnya(-1, remainder, total_idr));
    }
    out
}

struct Ctx<'a> {
    children: HashMap<i64, Vec<&'a PlanNodeInput>>,
    instrument_value: &'a HashMap<i64, Decimal>,
    instrument_category: &'a HashMap<i64, Option<i64>>,
}

fn category_total(cat_id: i64, ctx: &Ctx) -> Decimal {
    ctx.instrument_value
        .iter()
        .filter(|(iid, _)| ctx.instrument_category.get(iid).copied().flatten() == Some(cat_id))
        .map(|(_, v)| *v)
        .sum()
}

fn actual_value(node: &PlanNodeInput, ctx: &Ctx) -> Decimal {
    match node.bind_kind.as_str() {
        "instrument" => node
            .instrument_id
            .and_then(|iid| ctx.instrument_value.get(&iid).copied())
            .unwrap_or(Decimal::ZERO),
        "category" => node.category_id.map(|c| category_total(c, ctx)).unwrap_or(Decimal::ZERO),
        _ /* group */ => sorted_children(node, ctx).iter().map(|c| actual_value(c, ctx)).sum(),
    }
}

fn sorted_children<'a>(node: &PlanNodeInput, ctx: &Ctx<'a>) -> Vec<&'a PlanNodeInput> {
    let mut kids = ctx.children.get(&node.id).cloned().unwrap_or_default();
    kids.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.id.cmp(&b.id)));
    kids
}

fn build(node: &PlanNodeInput, parent_actual: Decimal, parent_target: Decimal, ctx: &Ctx) -> PlanNodeAllocation {
    let hundred = Decimal::from(100);
    let actual = actual_value(node, ctx);
    let target_value = parent_target * node.target_pct / hundred;
    let actual_pct = if parent_actual.is_zero() { Decimal::ZERO } else { actual / parent_actual * hundred };
    let drift = actual_pct - node.target_pct;
    let out_of_band = match node.tolerance_band_pct {
        Some(band) => drift.abs() > band,
        None => false,
    };

    let mut children: Vec<PlanNodeAllocation> = sorted_children(node, ctx)
        .iter()
        .map(|c| build(c, actual, target_value, ctx))
        .collect();

    // Category nodes surface their unbroken-down remainder as "Lainnya".
    if node.bind_kind == "category" {
        let claimed: Decimal = children.iter().map(|x| x.actual_value_idr).sum();
        let remainder = actual - claimed;
        if remainder > Decimal::ZERO {
            let syn_id = -2 - node.category_id.unwrap_or(0);
            children.push(lainnya(syn_id, remainder, actual));
        }
    }

    PlanNodeAllocation {
        id: node.id,
        name: node.name.clone(),
        bind_kind: node.bind_kind.clone(),
        target_pct: node.target_pct,
        tolerance_band_pct: node.tolerance_band_pct,
        actual_pct,
        actual_value_idr: actual,
        target_value_idr: target_value,
        drift_pct: drift,
        out_of_band,
        rebalance_idr: target_value - actual,
        color: node.color.clone(),
        children,
    }
}

/// A synthetic, target-less remainder node. Never flags out-of-band.
fn lainnya(id: i64, value: Decimal, parent_actual: Decimal) -> PlanNodeAllocation {
    let hundred = Decimal::from(100);
    let actual_pct = if parent_actual.is_zero() { Decimal::ZERO } else { value / parent_actual * hundred };
    PlanNodeAllocation {
        id,
        name: "Lainnya".to_string(),
        bind_kind: "lainnya".to_string(),
        target_pct: Decimal::ZERO,
        tolerance_band_pct: None,
        actual_pct,
        actual_value_idr: value,
        target_value_idr: Decimal::ZERO,
        drift_pct: actual_pct,
        out_of_band: false,
        rebalance_idr: Decimal::ZERO,
        color: None,
        children: Vec::new(),
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --manifest-path backend/Cargo.toml plan_alloc`
Expected: PASS (3 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/domain/plan_alloc.rs backend/src/domain/mod.rs
git commit -m "feat(planner): recursive plan-tree allocation rollup with synthetic Lainnya"
```

---

## Task 4: Service — `build_plan_tree`

**Files:**
- Modify: `backend/src/service/portfolio.rs`

**Interfaces:**
- Consumes: `build_summary(db) -> PortfolioSummary` (existing; `.positions: Vec<Position>` with `instrument_id: i64`, `market_value_idr: Decimal`; `.net_worth_idr: Decimal`), `repo::plan_nodes::list`, `repo::instruments::list`, `repo::dec`, `domain::plan_alloc::{PlanNodeInput, PlanNodeAllocation, compute_plan_tree}`.
- Produces: `pub async fn build_plan_tree(db: &Db) -> anyhow::Result<Vec<PlanNodeAllocation>>`.

- [ ] **Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` block in `backend/src/service/portfolio.rs`:

```rust
#[tokio::test]
async fn plan_tree_breaks_category_into_instrument_and_lainnya() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
    let cat = categories::create(&db, &categories::NewCategory { name:"Saham IDX".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
    // Migration backfill only covers categories that existed at connect() time; this
    // category was created afterward, so create its root plan_node explicitly (this is
    // what the frontend "add allocation" flow will do for new categories).
    let root = crate::repo::plan_nodes::create(&db, &crate::repo::plan_nodes::NewPlanNode{
        parent_id: None, name: "Saham".into(), target_pct: "100".into(),
        tolerance_band_pct: Some("5".into()), bind_kind: "category".into(),
        category_id: Some(cat.id), instrument_id: None, sort_order: None, color: None,
    }).await.unwrap();

    // Two stocks in the category, each worth 100 IDR.
    let bbca = instruments::create(&db, &instruments::NewInstrument{ symbol:"BBCA".into(), name:"BBCA".into(), instrument_type:"stock".into(), native_currency:"IDR".into(), category_id:Some(cat.id), price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
    let bbri = instruments::create(&db, &instruments::NewInstrument{ symbol:"BBRI".into(), name:"BBRI".into(), instrument_type:"stock".into(), native_currency:"IDR".into(), category_id:Some(cat.id), price_source:"manual".into(), decimals:Some(0), note:None }).await.unwrap();
    for ins in [bbca.id, bbri.id] {
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins, txn_type:"buy".into(), executed_at:chrono::Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"IDR".into(), fx_to_idr:"1".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins, dec("100").unwrap(), "IDR", "test", "2099-01-01").await.unwrap();
    }
    // Break out BBCA as an instrument child of the category root.
    crate::repo::plan_nodes::create(&db, &crate::repo::plan_nodes::NewPlanNode{
        parent_id: Some(root.id), name: "BBCA".into(), target_pct: "50".into(),
        tolerance_band_pct: None, bind_kind: "instrument".into(),
        category_id: None, instrument_id: Some(bbca.id), sort_order: None, color: None,
    }).await.unwrap();

    let tree = build_plan_tree(&db).await.unwrap();
    let saham = tree.iter().find(|n| n.id == root.id).unwrap();
    assert_eq!(saham.actual_value_idr, dec("200").unwrap()); // both stocks
    let bbca_node = saham.children.iter().find(|n| n.name == "BBCA").unwrap();
    assert_eq!(bbca_node.actual_value_idr, dec("100").unwrap());
    let lain = saham.children.iter().find(|n| n.name == "Lainnya").unwrap();
    assert_eq!(lain.actual_value_idr, dec("100").unwrap()); // BBRI remainder
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml plan_tree_breaks_category`
Expected: FAIL — `cannot find function build_plan_tree`.

- [ ] **Step 3: Implement `build_plan_tree`**

Add the import to the top of `backend/src/service/portfolio.rs` (extend the existing `use crate::domain::...` lines):

```rust
use crate::domain::plan_alloc::{compute_plan_tree, PlanNodeAllocation, PlanNodeInput};
use crate::repo::plan_nodes;
```

Add the function after `build_summary`:

```rust
/// Build the recursive allocation tree (plan_node overlay). Reuses build_summary's
/// per-instrument market values so valuation stays single-sourced.
pub async fn build_plan_tree(db: &Db) -> anyhow::Result<Vec<PlanNodeAllocation>> {
    let summary = build_summary(db).await?;
    let total = summary.net_worth_idr;

    let mut instrument_value = std::collections::HashMap::new();
    for p in &summary.positions {
        instrument_value.insert(p.instrument_id, p.market_value_idr);
    }
    let mut instrument_category = std::collections::HashMap::new();
    for ins in instruments::list(db).await? {
        instrument_category.insert(ins.id, ins.category_id);
    }

    let mut inputs = Vec::new();
    for r in plan_nodes::list(db).await? {
        inputs.push(PlanNodeInput {
            id: r.id,
            parent_id: r.parent_id,
            name: r.name,
            target_pct: dec(&r.target_pct)?,
            tolerance_band_pct: r.tolerance_band_pct.as_deref().map(dec).transpose()?,
            bind_kind: r.bind_kind,
            category_id: r.category_id,
            instrument_id: r.instrument_id,
            sort_order: r.sort_order,
            color: r.color,
        });
    }

    Ok(compute_plan_tree(&inputs, &instrument_value, &instrument_category, total))
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path backend/Cargo.toml plan_tree_breaks_category`
Expected: PASS.

- [ ] **Step 5: Run the whole service module to catch regressions**

Run: `cargo test --manifest-path backend/Cargo.toml service::portfolio`
Expected: PASS (existing summary tests still green).

- [ ] **Step 6: Commit**

```bash
git add backend/src/service/portfolio.rs
git commit -m "feat(planner): build_plan_tree service reusing summary valuations"
```

---

## Task 5: API — `/plan/*` routes

**Files:**
- Create: `backend/src/api/plan.rs`
- Modify: `backend/src/api/mod.rs` (add `pub mod plan;` + routes)

**Interfaces:**
- Consumes: `AppState` (`.db`), `AppError`, `repo::plan_nodes::{list,get,create,update,delete,move_node,NewPlanNode,UpdatePlanNode,MovePlanNode,PlanNodeRow}`, `service::portfolio::build_plan_tree`.
- Produces handlers: `get_tree`, `list_nodes`, `create_node`, `update_node`, `delete_node`, `move_node` and routes:
  - `GET  /plan/tree`        -> computed `Vec<PlanNodeAllocation>`
  - `GET  /plan/nodes`       -> raw `Vec<PlanNodeRow>`
  - `POST /plan/nodes`       -> `PlanNodeRow`
  - `PATCH /plan/nodes/:id`  -> `PlanNodeRow`
  - `DELETE /plan/nodes/:id` -> `()`
  - `POST /plan/nodes/:id/move` -> `PlanNodeRow`

- [ ] **Step 1: Write the failing test (route protection)**

Add to `backend/src/api/mod.rs` `router_tests` module:

```rust
#[serial]
#[tokio::test]
async fn plan_routes_are_protected() {
    std::env::set_var("AUTH_PASSWORD", "pw");
    std::env::set_var("JWT_SECRET", "router-test-plan");
    let app = router(test_state().await);
    let cases = [("/plan/tree", "GET"), ("/plan/nodes", "GET"), ("/plan/nodes", "POST"), ("/plan/nodes/1", "PATCH"), ("/plan/nodes/1/move", "POST")];
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

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path backend/Cargo.toml plan_routes_are_protected`
Expected: FAIL — routes return 404 (`NOT_FOUND`), not 401, because they aren't registered yet.

- [ ] **Step 3: Implement the handlers**

Create `backend/src/api/plan.rs`:

```rust
use crate::error::AppError;
use crate::repo::plan_nodes;
use crate::service::portfolio::build_plan_tree;
use crate::AppState;
use axum::{extract::{Path, State}, Json};

pub async fn get_tree(State(s): State<AppState>) -> Result<Json<Vec<crate::domain::plan_alloc::PlanNodeAllocation>>, AppError> {
    Ok(Json(build_plan_tree(&s.db).await.map_err(AppError::Other)?))
}

pub async fn list_nodes(State(s): State<AppState>) -> Result<Json<Vec<plan_nodes::PlanNodeRow>>, AppError> {
    Ok(Json(plan_nodes::list(&s.db).await.map_err(AppError::Other)?))
}

pub async fn create_node(State(s): State<AppState>, Json(b): Json<plan_nodes::NewPlanNode>) -> Result<Json<plan_nodes::PlanNodeRow>, AppError> {
    // Validate referenced parent/category/instrument up-front for clear 400s.
    if let Some(pid) = b.parent_id {
        plan_nodes::get(&s.db, pid).await.map_err(|_| AppError::BadRequest(format!("unknown parent_id {pid}")))?;
    }
    Ok(Json(plan_nodes::create(&s.db, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}

pub async fn update_node(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<plan_nodes::UpdatePlanNode>) -> Result<Json<plan_nodes::PlanNodeRow>, AppError> {
    plan_nodes::get(&s.db, id).await.map_err(|_| AppError::NotFound)?;
    Ok(Json(plan_nodes::update(&s.db, id, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}

pub async fn delete_node(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    plan_nodes::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

pub async fn move_node(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<plan_nodes::MovePlanNode>) -> Result<Json<plan_nodes::PlanNodeRow>, AppError> {
    plan_nodes::get(&s.db, id).await.map_err(|_| AppError::NotFound)?;
    Ok(Json(plan_nodes::move_node(&s.db, id, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}
```

- [ ] **Step 4: Register the module and routes**

In `backend/src/api/mod.rs`: add `pub mod plan;` with the other module declarations (near `pub mod portfolio;`).

Then add these routes inside the `protected` router (next to the `/categories` routes), using the already-imported `delete`, `get`, `post` and `axum::routing::patch`:

```rust
        .route("/plan/tree", get(plan::get_tree))
        .route("/plan/nodes", get(plan::list_nodes).post(plan::create_node))
        .route(
            "/plan/nodes/:id",
            delete(plan::delete_node).patch(plan::update_node),
        )
        .route("/plan/nodes/:id/move", post(plan::move_node))
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --manifest-path backend/Cargo.toml plan_routes_are_protected`
Expected: PASS (all 5 routes return 401).

- [ ] **Step 6: Full backend test + clippy**

Run: `cargo test --manifest-path backend/Cargo.toml`
Expected: PASS (whole suite).

Run: `cargo clippy --manifest-path backend/Cargo.toml --all-targets`
Expected: no new warnings in the files touched by this plan.

- [ ] **Step 7: Commit**

```bash
git add backend/src/api/plan.rs backend/src/api/mod.rs
git commit -m "feat(planner): /plan tree + node CRUD/move API"
```

---

## Done criteria (Phase 1)

- `plan_node` table exists and is backfilled from `category` at migration time.
- `GET /plan/tree` returns a recursive allocation tree with per-node actual/target/drift, synthetic "Lainnya" remainders, and a reconciling root total.
- Nodes can be created/updated/deleted/moved via `/plan/nodes*`, with bind-kind and cycle validation.
- Existing portfolio/allocation behavior is unchanged (all prior tests green).

## Follow-up plans (not in this plan)

- **Plan 2 — Allocation Tree frontend:** Planner page tree UI (drill-down, inline target edit, add/move/delete, per-level sum indicator), driven by `/plan/*`.
- **Plan 3 — Goals backend:** migration `0031` (`txn.goal_id`, `goal.target_date`, `current_kind='tagged'`), goal progress compute (market value + invested + P&L), `PATCH /goals/:id`, extended `GoalResponse`, transaction-tag endpoint + assistant tool.
- **Plan 4 — Goals frontend:** goal cards (market/invested/P&L, target-date countdown), transaction goal selector.
```
