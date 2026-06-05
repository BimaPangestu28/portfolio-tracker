# FX-Aware P&L Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Decompose IDR P&L into price vs FX components (unrealized + realized) using purchase-time FX rates already stored per transaction, and remove the dead IDR/USD top-bar toggle.

**Architecture:** The average-cost engine (`cost_basis.rs`) gains a parallel IDR cost track (`avg_cost_idr` per unit, using `txn.fx_to_idr` at each buy). Valuation derives `unrealized_pnl_idr = mv_native × fx_now − cost_basis_idr`, decomposed as `price = native_pnl × fx_now` and `fx = total − price` (residual, so the invariant `price + fx = total` is exact). Sells realize the decomposition the same way. The summary aggregates the components; daily snapshots store them in two new nullable columns; the frontend displays the breakdown on Dashboard, Holdings, and Performance.

**Tech Stack:** Rust (axum, sqlx/SQLite, rust_decimal), React + TypeScript (zod, vitest + msw, recharts).

**Spec:** `docs/superpowers/specs/2026-06-05-fx-aware-pnl-design.md`

**Conventions (from CLAUDE.md / project memory):**
- NEVER run `cargo fmt` / rustfmt — this repo deliberately doesn't use it. Match surrounding style by hand.
- Backend verification = `cargo clippy` + `cargo test` from `backend/`.
- Frontend verification = `npx vitest run` + `npm run build` from `frontend/`.
- No `unwrap()`/`panic!()` in production paths. Conventional commits.
- Branch: `feat/fx-aware-pnl` (already created, spec committed).

**Formula reference (used by Tasks 1–3):**

```
# Per-unit IDR average cost is accumulated on buys:
avg_cost_idr := Σ(buy cost_native × fx_to_idr at buy) / qty      (average-cost, like avg_cost)

# Unrealized (at valuation time, fx_now = current native→IDR rate):
unrealized_pnl_idr       = mv_native × fx_now − qty × avg_cost_idr
unrealized_price_pnl_idr = (mv_native − cost_basis_total) × fx_now
unrealized_fx_pnl_idr    = unrealized_pnl_idr − unrealized_price_pnl_idr

# Realized (per sell, fx_sell = txn.fx_to_idr of the sell):
realized_delta_native    = (price − avg_cost) × sold − fee
realized_idr_delta       = (price × fx_sell − avg_cost_idr) × sold − fee × fx_sell
realized_price_idr_delta = realized_delta_native × fx_sell
realized_fx_idr_delta    = realized_idr_delta − realized_price_idr_delta
```

For IDR instruments `fx_to_idr = 1` everywhere, so the FX component is exactly 0 with no special-casing.

---

### Task 1: Engine — dual-currency cost basis

**Files:**
- Modify: `backend/src/domain/cost_basis.rs`

- [ ] **Step 1: Write the failing tests**

In the existing `tests` module of `backend/src/domain/cost_basis.rs`, add a helper that controls the FX rate (the existing `tx` helper hardcodes `fx_to_idr: dec!(16000)` — keep it, add `tx_fx`):

```rust
    fn tx_fx(t: TxnType, qty: Decimal, price: Decimal, fee: Decimal, fx_to_idr: Decimal) -> Transaction {
        Transaction {
            id: 0,
            account_id: 1,
            instrument_id: 1,
            txn_type: t,
            executed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            quantity: qty,
            price_native: price,
            fee_native: fee,
            currency: "USD".into(),
            fx_to_idr,
            fx_to_usd: dec!(1),
            note: None,
        }
    }

    #[test]
    fn idr_cost_basis_uses_purchase_fx() {
        // 1 @ 100 at fx 16000, 1 @ 100 at fx 17000 -> avg_idr 1,650,000/unit.
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(1), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Buy, dec!(1), dec!(100), dec!(0), dec!(17000)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.avg_cost_idr, dec!(1650000));
        assert_eq!(cb.cost_basis_idr_total, dec!(3300000));
    }

    #[test]
    fn sell_decomposes_realized_into_price_and_fx() {
        // Buy 2 @ 100 at fx 16000 (avg_idr 1.6M). Sell 1 @ 150 at fx 17000:
        //   native realized = 50
        //   total idr = 150*17000 - 1,600,000 = 950,000
        //   price idr = 50*17000 = 850,000
        //   fx idr    = 100,000  (IDR weakened on the principal)
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(2), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Sell, dec!(1), dec!(150), dec!(0), dec!(17000)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl, dec!(50));
        assert_eq!(cb.realized_pnl_idr, dec!(950000));
        assert_eq!(cb.realized_price_pnl_idr, dec!(850000));
        assert_eq!(cb.realized_fx_pnl_idr, dec!(100000));
        // Remaining lot keeps its purchase-rate IDR basis.
        assert_eq!(cb.cost_basis_idr_total, dec!(1600000));
    }

    #[test]
    fn sell_fee_hits_both_idr_components_consistently() {
        // Buy 1 @ 100 fx 16000. Sell 1 @ 110 fee 5 fx 16500:
        //   native realized = (110-100)*1 - 5 = 5
        //   total idr = 110*16500 - 1,600,000 - 5*16500 = 132,500
        //   price idr = 5*16500 = 82,500 ; fx idr = 50,000
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(1), dec!(100), dec!(0), dec!(16000)),
            tx_fx(TxnType::Sell, dec!(1), dec!(110), dec!(5), dec!(16500)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl, dec!(5));
        assert_eq!(cb.realized_pnl_idr, dec!(132500));
        assert_eq!(cb.realized_price_pnl_idr, dec!(82500));
        assert_eq!(cb.realized_fx_pnl_idr, dec!(50000));
    }

    #[test]
    fn idr_instrument_has_zero_fx_component() {
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(2), dec!(1000), dec!(0), dec!(1)),
            tx_fx(TxnType::Sell, dec!(1), dec!(1500), dec!(0), dec!(1)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl_idr, dec!(500));
        assert_eq!(cb.realized_price_pnl_idr, dec!(500));
        assert_eq!(cb.realized_fx_pnl_idr, dec!(0));
        assert_eq!(cb.cost_basis_idr_total, dec!(1000));
    }

    #[test]
    fn realized_idr_invariant_price_plus_fx_equals_total() {
        // Multi-leg sequence with mixed rates; the residual definition makes this
        // exact, but keep the regression guard.
        let txns = vec![
            tx_fx(TxnType::Buy, dec!(3), dec!(80), dec!(2), dec!(15500)),
            tx_fx(TxnType::Buy, dec!(1), dec!(120), dec!(1), dec!(16200)),
            tx_fx(TxnType::Sell, dec!(2), dec!(110), dec!(3), dec!(16800)),
        ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.realized_pnl_idr, cb.realized_price_pnl_idr + cb.realized_fx_pnl_idr);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test cost_basis`
Expected: COMPILE ERROR — `CostBasis` has no field `avg_cost_idr` (etc.). A compile failure in the test module is the failing state here.

- [ ] **Step 3: Implement the dual-currency engine**

Replace the `CostBasis` struct and `compute_cost_basis` body in `backend/src/domain/cost_basis.rs`:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct CostBasis {
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub cost_basis_total: Decimal,
    pub realized_pnl: Decimal,
    pub income: Decimal,
    /// Average cost per unit in IDR at purchase-time FX (`txn.fx_to_idr`).
    pub avg_cost_idr: Decimal,
    pub cost_basis_idr_total: Decimal,
    /// Realized P&L in IDR, decomposed: price (native P&L × FX at sell) + fx (residual).
    pub realized_pnl_idr: Decimal,
    pub realized_price_pnl_idr: Decimal,
    pub realized_fx_pnl_idr: Decimal,
}

/// Average-cost engine. `txns` MUST be sorted ascending by `executed_at`.
pub fn compute_cost_basis(txns: &[Transaction]) -> CostBasis {
    let mut qty = Decimal::ZERO;
    let mut avg = Decimal::ZERO;
    let mut avg_idr = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut realized_idr = Decimal::ZERO;
    let mut realized_price_idr = Decimal::ZERO;
    let mut realized_fx_idr = Decimal::ZERO;
    let mut income = Decimal::ZERO;

    for t in txns {
        match t.txn_type {
            TxnType::Buy | TxnType::OpeningBalance | TxnType::Deposit => {
                let added_cost = t.quantity * t.price_native + t.fee_native;
                let added_cost_idr = added_cost * t.fx_to_idr;
                let new_qty = qty + t.quantity;
                if new_qty.is_zero() {
                    avg = Decimal::ZERO;
                    avg_idr = Decimal::ZERO;
                } else {
                    avg = (qty * avg + added_cost) / new_qty;
                    avg_idr = (qty * avg_idr + added_cost_idr) / new_qty;
                }
                qty = new_qty;
            }
            TxnType::Sell | TxnType::Withdrawal => {
                // Cap the sold quantity at what is actually held. Overselling (recording a
                // sell larger than the current position) would otherwise realize P&L on
                // phantom units. We realize only on owned units and zero the position.
                // NOTE: surfacing the oversell as a user-facing validation error is a
                // Phase 3 (review-queue) concern; here we keep the math sound.
                let sold = if t.quantity > qty { qty } else { t.quantity };
                let realized_delta = (t.price_native - avg) * sold - t.fee_native;
                realized += realized_delta;
                // IDR realized: proceeds at the sell-time rate minus the purchase-rate
                // basis of the units consumed. Decomposed as price (native P&L at the
                // sell-time rate) + fx (residual on the principal), so price+fx == total.
                let realized_idr_delta =
                    (t.price_native * t.fx_to_idr - avg_idr) * sold - t.fee_native * t.fx_to_idr;
                let realized_price_idr_delta = realized_delta * t.fx_to_idr;
                realized_idr += realized_idr_delta;
                realized_price_idr += realized_price_idr_delta;
                realized_fx_idr += realized_idr_delta - realized_price_idr_delta;
                qty -= sold;
                if qty < Decimal::ZERO {
                    qty = Decimal::ZERO;
                }
            }
            TxnType::Dividend | TxnType::Interest => {
                income += t.quantity * t.price_native;
            }
            TxnType::Fee => {
                income -= t.quantity * t.price_native;
            }
        }
    }

    CostBasis {
        quantity: qty,
        avg_cost: avg,
        cost_basis_total: qty * avg,
        realized_pnl: realized,
        income,
        avg_cost_idr: avg_idr,
        cost_basis_idr_total: qty * avg_idr,
        realized_pnl_idr: realized_idr,
        realized_price_pnl_idr: realized_price_idr,
        realized_fx_pnl_idr: realized_fx_idr,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test cost_basis`
Expected: PASS — all pre-existing tests (`buy_then_buy_averages_cost`, `oversell_caps_at_held_quantity`, …) plus the 5 new ones.

- [ ] **Step 5: Commit**

```bash
git add backend/src/domain/cost_basis.rs
git commit -m "feat(engine): track IDR cost basis and decompose realized P&L into price vs FX"
```

---

### Task 2: Valuation — Position decomposition

**Files:**
- Modify: `backend/src/domain/valuation.rs`

- [ ] **Step 1: Write the failing test**

In the `tests` module of `backend/src/domain/valuation.rs`, the existing `tx` helper hardcodes `fx_to_idr: dec!(16000)` — that's what we want for the buy. Add:

```rust
    #[test]
    fn position_decomposes_unrealized_pnl_into_price_and_fx() {
        // Buy 2 @ 100 at fx 16000 (cost 3.2M IDR). Now price 150, fx 17000:
        //   mv_idr = 300*17000 = 5,100,000 ; total = 1,900,000
        //   price  = (300-200)*17000 = 1,700,000 ; fx = 200,000
        let txns = vec![tx(TxnType::Buy, dec!(2), dec!(100))];
        let cb = compute_cost_basis(&txns);
        let ctx = PriceContext {
            instrument_id: 7,
            latest_price_native: dec!(150),
            price_stale: false,
            fx_native_to_idr: dec!(17000),
            fx_native_to_usd: dec!(1),
        };
        let p = build_position(7, &cb, &ctx);
        assert_eq!(p.cost_basis_idr_total, dec!(3200000));
        assert_eq!(p.unrealized_pnl_idr, dec!(1900000));
        assert_eq!(p.unrealized_price_pnl_idr, dec!(1700000));
        assert_eq!(p.unrealized_fx_pnl_idr, dec!(200000));
        assert_eq!(p.unrealized_pnl_idr, p.unrealized_price_pnl_idr + p.unrealized_fx_pnl_idr);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test valuation`
Expected: COMPILE ERROR — `Position` has no field `cost_basis_idr_total`.

- [ ] **Step 3: Extend Position and build_position**

In `backend/src/domain/valuation.rs`, add fields to `Position` (after `income`, keeping the serde attribute style):

```rust
    #[serde(with = "rust_decimal::serde::str")]
    pub cost_basis_idr_total: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub unrealized_fx_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub realized_fx_pnl_idr: Decimal,
    /// True when some txns had no usable purchase-time FX rate (see service::portfolio),
    /// making the IDR decomposition for this position incomplete.
    pub fx_incomplete: bool,
```

Replace `build_position`:

```rust
pub fn build_position(instrument_id: i64, cb: &CostBasis, ctx: &PriceContext) -> Position {
    let mv_native = cb.quantity * ctx.latest_price_native;
    let mv_idr = mv_native * ctx.fx_native_to_idr;
    // FX-aware unrealized P&L: current value at today's rate minus the purchase-rate
    // basis. Price component is the native P&L at today's rate; FX is the residual,
    // so price + fx == total exactly.
    let unrealized_idr = mv_idr - cb.cost_basis_idr_total;
    let unrealized_price_idr = (mv_native - cb.cost_basis_total) * ctx.fx_native_to_idr;
    Position {
        instrument_id,
        quantity: cb.quantity,
        avg_cost: cb.avg_cost,
        cost_basis_total: cb.cost_basis_total,
        latest_price: ctx.latest_price_native,
        price_stale: ctx.price_stale,
        market_value_native: mv_native,
        market_value_idr: mv_idr,
        market_value_usd: mv_native * ctx.fx_native_to_usd,
        unrealized_pnl: mv_native - cb.cost_basis_total,
        realized_pnl: cb.realized_pnl,
        income: cb.income,
        cost_basis_idr_total: cb.cost_basis_idr_total,
        unrealized_pnl_idr: unrealized_idr,
        unrealized_price_pnl_idr: unrealized_price_idr,
        unrealized_fx_pnl_idr: unrealized_idr - unrealized_price_idr,
        realized_pnl_idr: cb.realized_pnl_idr,
        realized_price_pnl_idr: cb.realized_price_pnl_idr,
        realized_fx_pnl_idr: cb.realized_fx_pnl_idr,
        fx_incomplete: false, // overridden by service::portfolio after FX-gap resolution
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test valuation`
Expected: PASS (existing `position_values_in_dual_currency` + new test).

- [ ] **Step 5: Commit**

```bash
git add backend/src/domain/valuation.rs
git commit -m "feat(valuation): decompose unrealized P&L (IDR) into price vs FX components"
```

---

### Task 3: FX-gap resolution — `fx_on` repo lookup + service patch

**Files:**
- Modify: `backend/src/repo/prices.rs`
- Modify: `backend/src/service/portfolio.rs` (helper only; aggregation is Task 4)

- [ ] **Step 1: Write the failing repo test**

In the `tests` module of `backend/src/repo/prices.rs`:

```rust
    #[tokio::test]
    async fn fx_on_returns_rate_at_or_before_date() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert_fx(&db, "USD", "IDR", d!(15000), "2026-01-01").await.unwrap();
        upsert_fx(&db, "USD", "IDR", d!(16000), "2026-03-01").await.unwrap();
        // Exact date
        assert_eq!(fx_on(&db, "USD", "IDR", "2026-03-01").await.unwrap(), Some(d!(16000)));
        // Between rows -> most recent before
        assert_eq!(fx_on(&db, "USD", "IDR", "2026-02-15").await.unwrap(), Some(d!(15000)));
        // Before any row -> None
        assert_eq!(fx_on(&db, "USD", "IDR", "2025-12-31").await.unwrap(), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test fx_on`
Expected: COMPILE ERROR — `fx_on` not found.

- [ ] **Step 3: Implement `fx_on`**

In `backend/src/repo/prices.rs`, next to `latest_fx`:

```rust
pub async fn fx_on(db: &Db, base: &str, quote: &str, as_of: &str) -> anyhow::Result<Option<Decimal>> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT rate FROM fx_rate WHERE base=? AND quote=? AND as_of<=? ORDER BY as_of DESC LIMIT 1")
        .bind(base).bind(quote).bind(as_of).fetch_optional(db).await?;
    match row { Some((r,)) => Ok(Some(dec(&r)?)), None => Ok(None) }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend && cargo test fx_on`
Expected: PASS.

- [ ] **Step 5: Add the service-level gap resolver (with test)**

In `backend/src/service/portfolio.rs`, add above `build_summary`:

```rust
/// Backfill zero `fx_to_idr` on historical txns from the fx_rate table (rate
/// at-or-before the txn date). Returns true when at least one txn still has no
/// usable rate — callers must surface this (Position.fx_incomplete), never
/// silently substitute the current rate.
async fn resolve_fx_gaps(db: &Db, txns: &mut [crate::domain::models::Transaction]) -> anyhow::Result<bool> {
    let mut incomplete = false;
    for t in txns.iter_mut() {
        if !t.fx_to_idr.is_zero() {
            continue;
        }
        if t.currency == "IDR" {
            t.fx_to_idr = Decimal::ONE;
            continue;
        }
        let date = t.executed_at.format("%Y-%m-%d").to_string();
        match prices::fx_on(db, &t.currency, "IDR", &date).await? {
            Some(rate) if !rate.is_zero() => t.fx_to_idr = rate,
            _ => incomplete = true,
        }
    }
    Ok(incomplete)
}
```

And in the `tests` module of `service/portfolio.rs`:

```rust
    #[tokio::test]
    async fn resolve_fx_gaps_backfills_from_fx_rate_table() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        prices::upsert_fx(&db, "USD", "IDR", dec("15500").unwrap(), "2026-01-01").await.unwrap();
        let mut txns = vec![crate::domain::models::Transaction {
            id: 1, account_id: 1, instrument_id: 1,
            txn_type: TxnType::Buy,
            executed_at: chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z").unwrap().with_timezone(&Utc),
            quantity: dec("1").unwrap(), price_native: dec("100").unwrap(),
            fee_native: dec("0").unwrap(), currency: "USD".into(),
            fx_to_idr: dec("0").unwrap(), fx_to_usd: dec("1").unwrap(), note: None,
        }];
        let incomplete = resolve_fx_gaps(&db, &mut txns).await.unwrap();
        assert!(!incomplete);
        assert_eq!(txns[0].fx_to_idr, dec("15500").unwrap());
    }

    #[tokio::test]
    async fn resolve_fx_gaps_flags_unresolvable_txn() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // No fx_rate rows at all -> can't backfill, must flag.
        let mut txns = vec![crate::domain::models::Transaction {
            id: 1, account_id: 1, instrument_id: 1,
            txn_type: TxnType::Buy,
            executed_at: chrono::DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z").unwrap().with_timezone(&Utc),
            quantity: dec("1").unwrap(), price_native: dec("100").unwrap(),
            fee_native: dec("0").unwrap(), currency: "USD".into(),
            fx_to_idr: dec("0").unwrap(), fx_to_usd: dec("1").unwrap(), note: None,
        }];
        let incomplete = resolve_fx_gaps(&db, &mut txns).await.unwrap();
        assert!(incomplete);
        assert_eq!(txns[0].fx_to_idr, dec("0").unwrap()); // untouched, not guessed
    }
```

Note: `dec` here is `crate::repo::dec` (already imported in this file); `Utc` needs `use chrono::Utc;` which the existing test module already has.

- [ ] **Step 6: Run tests, verify pass**

Run: `cd backend && cargo test resolve_fx_gaps`
Expected: 2 PASS. (`resolve_fx_gaps` is now used only by tests — `build_summary` wires it in Task 4; if clippy complains about dead_code at this intermediate point, that resolves in Task 4.)

- [ ] **Step 7: Commit**

```bash
git add backend/src/repo/prices.rs backend/src/service/portfolio.rs
git commit -m "feat(fx): dated FX lookup and explicit gap resolution for purchase-time rates"
```

---

### Task 4: Summary aggregation — FX-aware totals

**Files:**
- Modify: `backend/src/service/portfolio.rs`

- [ ] **Step 1: Write the failing test**

In the `tests` module of `backend/src/service/portfolio.rs` (mirror the setup of `summary_consolidates_one_position`):

```rust
    #[tokio::test]
    async fn summary_captures_fx_gain_on_usd_position() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let cat = categories::create(&db, &categories::NewCategory { name:"Crypto".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument{ symbol:"BTC".into(), name:"BTC".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:Some(cat.id), price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        // Bought at fx 16000; IDR has since weakened to 17000.
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None }).await.unwrap();
        prices::upsert_latest(&db, ins.id, dec("150").unwrap(), "USD", "test", "2099-01-01").await.unwrap();
        prices::upsert_fx(&db, "USD", "IDR", dec("17000").unwrap(), "2099-01-01").await.unwrap();

        let s = build_summary(&db).await.unwrap();
        // total = 150*17000 - 100*16000 = 950,000 ; price = 50*17000 = 850,000 ; fx = 100,000
        assert_eq!(s.total_unrealized_pnl_idr, dec("950000").unwrap());
        assert_eq!(s.total_unrealized_price_pnl_idr, dec("850000").unwrap());
        assert_eq!(s.total_unrealized_fx_pnl_idr, dec("100000").unwrap());
        let p = &s.positions[0];
        assert!(!p.fx_incomplete);
        assert_eq!(p.unrealized_fx_pnl_idr, dec("100000").unwrap());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test summary_captures_fx_gain`
Expected: COMPILE ERROR — `PortfolioSummary` has no field `total_unrealized_price_pnl_idr`.

- [ ] **Step 3: Extend PortfolioSummary and build_summary**

In `backend/src/service/portfolio.rs`, add to `PortfolioSummary` after `total_realized_pnl_idr`:

```rust
    #[serde(with = "rust_decimal::serde::str")]
    pub total_unrealized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_unrealized_fx_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_realized_price_pnl_idr: Decimal,
    #[serde(with = "rust_decimal::serde::str")]
    pub total_realized_fx_pnl_idr: Decimal,
```

In `build_summary`:

1. Add accumulators next to `unreal_idr`/`real_idr`:

```rust
    let mut unreal_price_idr = Decimal::ZERO;
    let mut unreal_fx_idr = Decimal::ZERO;
    let mut real_price_idr = Decimal::ZERO;
    let mut real_fx_idr = Decimal::ZERO;
```

2. Change the position loop to own its txn group mutably and resolve FX gaps before computing (replace `for (instrument_id, txns) in &grouped {` and the `compute_cost_basis`/`instruments::get`/`prices::latest` lines — note `grouped` is consumed, and `instrument_id` is now by-value, so the `*instrument_id` derefs below it drop their `*`):

```rust
    for (instrument_id, mut txns) in grouped {
        let fx_incomplete = resolve_fx_gaps(db, &mut txns).await?;
        let cb = compute_cost_basis(&txns);
        let ins = instruments::get(db, instrument_id).await?;
        let latest = prices::latest(db, instrument_id).await?;
```

3. Replace the aggregation lines (`unreal_idr += p.unrealized_pnl * to_idr;` and `real_idr += p.realized_pnl * to_idr;`) with the FX-aware position figures, and surface the gap flag:

```rust
        let mut p = build_position(instrument_id, &cb, &ctx);
        p.fx_incomplete = fx_incomplete;
        net_idr += p.market_value_idr;
        net_usd += p.market_value_usd;
        // FX-aware totals: cost basis at purchase-time rates, value at today's rate.
        unreal_idr += p.unrealized_pnl_idr;
        real_idr += p.realized_pnl_idr;
        unreal_price_idr += p.unrealized_price_pnl_idr;
        unreal_fx_idr += p.unrealized_fx_pnl_idr;
        real_price_idr += p.realized_price_pnl_idr;
        real_fx_idr += p.realized_fx_pnl_idr;
        positions.push(p);
```

(The `let ctx = PriceContext { instrument_id, ... }` line also drops its `*`.)

4. Extend the final struct literal:

```rust
        total_unrealized_price_pnl_idr: unreal_price_idr,
        total_unrealized_fx_pnl_idr: unreal_fx_idr,
        total_realized_price_pnl_idr: real_price_idr,
        total_realized_fx_pnl_idr: real_fx_idr,
```

- [ ] **Step 4: Run the full backend suite**

Run: `cd backend && cargo test`
Expected: PASS — including the pre-existing `summary_consolidates_one_position` (its txn fx 16000 == current fx 16000, so totals are unchanged by the new math).

- [ ] **Step 5: Commit**

```bash
git add backend/src/service/portfolio.rs
git commit -m "feat(portfolio): FX-aware P&L totals with price/FX decomposition in summary"
```

---

### Task 5: Snapshot decomposition columns

**Files:**
- Create: `backend/migrations/0010_snapshot_pnl_decomposition.sql`
- Modify: `backend/src/repo/snapshots.rs`
- Modify: `backend/src/scheduler.rs:15`
- Modify: `backend/src/service/performance.rs` (3 `snapshots::upsert` test call sites)

- [ ] **Step 1: Write the migration**

`backend/migrations/0010_snapshot_pnl_decomposition.sql`:

```sql
-- Unrealized P&L decomposition (price vs FX, in IDR) captured per daily snapshot.
-- Nullable: rows written before this feature have no decomposition and stay NULL.
ALTER TABLE valuation_snapshot ADD COLUMN price_pnl_idr TEXT;
ALTER TABLE valuation_snapshot ADD COLUMN fx_pnl_idr TEXT;
```

- [ ] **Step 2: Write the failing repo test**

Replace the existing test in `backend/src/repo/snapshots.rs` (its `upsert` signature changes):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn snapshot_upsert_and_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert(&db, "2026-05-31", "1000", "0.06", "{}", None, None).await.unwrap();
        upsert(&db, "2026-05-31", "1100", "0.07", "{}", Some("900"), Some("200")).await.unwrap();
        let rows = history(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].total_idr, "1100");
        assert_eq!(rows[0].price_pnl_idr.as_deref(), Some("900"));
        assert_eq!(rows[0].fx_pnl_idr.as_deref(), Some("200"));
    }

    #[tokio::test]
    async fn snapshot_decomposition_is_nullable() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert(&db, "2026-05-31", "1000", "0.06", "{}", None, None).await.unwrap();
        let rows = history(&db).await.unwrap();
        assert_eq!(rows[0].price_pnl_idr, None);
        assert_eq!(rows[0].fx_pnl_idr, None);
    }
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cd backend && cargo test snapshot_`
Expected: COMPILE ERROR — `upsert` takes 5 arguments.

- [ ] **Step 4: Implement repo changes**

Replace `upsert` and `SnapshotRow` in `backend/src/repo/snapshots.rs`:

```rust
pub async fn upsert(db: &Db, as_of: &str, total_idr: &str, total_usd: &str, breakdown_json: &str, price_pnl_idr: Option<&str>, fx_pnl_idr: Option<&str>) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO valuation_snapshot (as_of, total_idr, total_usd, breakdown_json, price_pnl_idr, fx_pnl_idr) VALUES (?,?,?,?,?,?)
         ON CONFLICT(as_of) DO UPDATE SET total_idr=excluded.total_idr, total_usd=excluded.total_usd, breakdown_json=excluded.breakdown_json, price_pnl_idr=excluded.price_pnl_idr, fx_pnl_idr=excluded.fx_pnl_idr")
        .bind(as_of).bind(total_idr).bind(total_usd).bind(breakdown_json).bind(price_pnl_idr).bind(fx_pnl_idr)
        .execute(db).await?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct SnapshotRow { pub as_of: String, pub total_idr: String, pub total_usd: String, pub breakdown_json: String, pub price_pnl_idr: Option<String>, pub fx_pnl_idr: Option<String> }
```

- [ ] **Step 5: Update callers**

`backend/src/scheduler.rs:15` — pass the decomposition (unrealized totals as-of today):

```rust
                    let _ = crate::repo::snapshots::upsert(
                        &db,
                        &today,
                        &s.net_worth_idr.to_string(),
                        &s.net_worth_usd.to_string(),
                        &breakdown,
                        Some(&s.total_unrealized_price_pnl_idr.to_string()),
                        Some(&s.total_unrealized_fx_pnl_idr.to_string()),
                    ).await;
```

`backend/src/service/performance.rs` — the 3 test call sites (`snapshots::upsert(&db, "2026-01-01", "1000000", "65", "{}")` etc., around lines 162-165, 202-207, 239) each gain `, None, None` before `.await`.

- [ ] **Step 6: Run the full backend suite + clippy**

Run: `cd backend && cargo test && cargo clippy --all-targets`
Expected: tests PASS, clippy clean. (sqlx runs migration 0010 automatically via `migrate!`.)

- [ ] **Step 7: Commit**

```bash
git add backend/migrations/0010_snapshot_pnl_decomposition.sql backend/src/repo/snapshots.rs backend/src/scheduler.rs backend/src/service/performance.rs
git commit -m "feat(snapshot): persist daily price/FX P&L decomposition (nullable columns)"
```

---

### Task 6: Frontend schemas + test fixtures

**Files:**
- Modify: `frontend/src/api/schemas.ts:53-99`
- Modify: `frontend/src/test/server.ts:5-11`
- Modify: `frontend/src/pages/HoldingsPage.test.tsx` (2 summary fixtures)
- Modify: `frontend/src/pages/DashboardPage.test.tsx:34-40`

- [ ] **Step 1: Extend zod schemas**

In `frontend/src/api/schemas.ts`, add to `PositionSchema` after `income`:

```ts
  cost_basis_idr_total: z.string(),
  unrealized_pnl_idr: z.string(),
  unrealized_price_pnl_idr: z.string(),
  unrealized_fx_pnl_idr: z.string(),
  realized_pnl_idr: z.string(),
  realized_price_pnl_idr: z.string(),
  realized_fx_pnl_idr: z.string(),
  fx_incomplete: z.boolean(),
```

Add to `PortfolioSummarySchema` after `total_realized_pnl_idr`:

```ts
  total_unrealized_price_pnl_idr: z.string(),
  total_unrealized_fx_pnl_idr: z.string(),
  total_realized_price_pnl_idr: z.string(),
  total_realized_fx_pnl_idr: z.string(),
```

Add to `SnapshotSchema` after `breakdown_json` (genuinely nullable — pre-feature rows):

```ts
  price_pnl_idr: z.string().nullable().optional(),
  fx_pnl_idr: z.string().nullable().optional(),
```

- [ ] **Step 2: Update the default msw summary fixture**

`frontend/src/test/server.ts` summary handler becomes:

```ts
  http.get("/api/portfolio/summary", () =>
    HttpResponse.json({
      net_worth_idr: "4875000", net_worth_usd: "300",
      total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0", xirr: 1.68,
      total_unrealized_price_pnl_idr: "80", total_unrealized_fx_pnl_idr: "20",
      total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
      positions: [], allocation: [],
    }),
  ),
```

- [ ] **Step 3: Update HoldingsPage fixtures**

`frontend/src/pages/HoldingsPage.test.tsx` — first fixture (IDX stock, IDR native: IDR figures mirror native, FX component 0). Add to the summary object:

```ts
        total_unrealized_price_pnl_idr: "2400000",
        total_unrealized_fx_pnl_idr: "0",
        total_realized_price_pnl_idr: "0",
        total_realized_fx_pnl_idr: "0",
```

and to its position object after `income: "0"`:

```ts
            cost_basis_idr_total: "39600000",
            unrealized_pnl_idr: "2400000",
            unrealized_price_pnl_idr: "2400000",
            unrealized_fx_pnl_idr: "0",
            realized_pnl_idr: "0",
            realized_price_pnl_idr: "0",
            realized_fx_pnl_idr: "0",
            fx_incomplete: false,
```

Second fixture (AAPL, USD native, bought at fx 16000, current implied fx 16000 → FX component 0; native P&L 250 × 16000 = 4,000,000). Add to the summary object:

```ts
        total_unrealized_price_pnl_idr: "4000000",
        total_unrealized_fx_pnl_idr: "0",
        total_realized_price_pnl_idr: "0",
        total_realized_fx_pnl_idr: "0",
```

and to its position object after `income: "0"`:

```ts
            cost_basis_idr_total: "12000000",
            unrealized_pnl_idr: "4000000",
            unrealized_price_pnl_idr: "4000000",
            unrealized_fx_pnl_idr: "0",
            realized_pnl_idr: "0",
            realized_price_pnl_idr: "0",
            realized_fx_pnl_idr: "0",
            fx_incomplete: false,
```

- [ ] **Step 4: Update DashboardPage fixture**

`frontend/src/pages/DashboardPage.test.tsx:34-40` — add inside the summary object:

```ts
        total_unrealized_price_pnl_idr: "-10000", total_unrealized_fx_pnl_idr: "0",
        total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
```

- [ ] **Step 5: Run the frontend suite**

Run: `cd frontend && npx vitest run`
Expected: PASS — schema additions are satisfied by every fixture; no UI change yet.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/test/server.ts frontend/src/pages/HoldingsPage.test.tsx frontend/src/pages/DashboardPage.test.tsx
git commit -m "feat(frontend): decode price/FX P&L decomposition fields from the API"
```

---

### Task 7: HoldingsPage — FX-aware P&L per row

**Files:**
- Modify: `frontend/src/pages/HoldingsPage.tsx`
- Test: `frontend/src/pages/HoldingsPage.test.tsx`

- [ ] **Step 1: Write the failing test**

Append to `frontend/src/pages/HoldingsPage.test.tsx` (a USD position where IDR weakened after purchase: bought at fx 15000, now 16000 — price P&L $250×16000 = 4M, FX P&L $750 principal ×1000 = 750k, total 4.75M):

```ts
test("shows FX-aware P&L with an FX sub-line for a USD instrument", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "16000000",
        net_worth_usd: "1000",
        total_unrealized_pnl_idr: "4750000",
        total_realized_pnl_idr: "0",
        total_unrealized_price_pnl_idr: "4000000",
        total_unrealized_fx_pnl_idr: "750000",
        total_realized_price_pnl_idr: "0",
        total_realized_fx_pnl_idr: "0",
        xirr: null,
        positions: [
          {
            instrument_id: 2,
            quantity: "5",
            avg_cost: "150",
            cost_basis_total: "750",
            latest_price: "200",
            price_stale: false,
            market_value_native: "1000",
            market_value_idr: "16000000",
            market_value_usd: "1000",
            unrealized_pnl: "250",
            realized_pnl: "0",
            income: "0",
            cost_basis_idr_total: "11250000",
            unrealized_pnl_idr: "4750000",
            unrealized_price_pnl_idr: "4000000",
            unrealized_fx_pnl_idr: "750000",
            realized_pnl_idr: "0",
            realized_price_pnl_idr: "0",
            realized_fx_pnl_idr: "0",
            fx_incomplete: false,
          },
        ],
        allocation: [],
      }),
    ),
    http.get("/api/instruments", () =>
      HttpResponse.json([
        {
          id: 2,
          symbol: "AAPL",
          name: "Apple Inc.",
          instrument_type: "stock",
          native_currency: "USD",
          category_id: null,
          price_source: "yahoo",
          decimals: 2,
          note: null,
        },
      ]),
    ),
  );

  render(<HoldingsPage />, { wrapper });

  // Primary P&L is the FX-aware IDR figure (4.75M), not native x current fx (4M).
  expect(await screen.findByText(/Rp\s*4\.750\.000/)).toBeInTheDocument();
  // FX contribution is broken out on its own sub-line.
  expect(screen.getByText(/FX\s*\+\s*Rp\s*750\.000/)).toBeInTheDocument();
  // Percent is against the purchase-rate IDR cost basis: 4.75M / 11.25M = 42.2%.
  expect(screen.getByText(/42[.,]2\s*%/)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/pages/HoldingsPage.test.tsx`
Expected: FAIL — `Rp 4.750.000` not found (page still renders native×fx = 4.000.000).

- [ ] **Step 3: Implement the row changes**

In `frontend/src/pages/HoldingsPage.tsx`:

1. Sort key (line 8): replace `"unrealized_pnl"` with `"unrealized_pnl_idr"` in the `SortKey` union, and at line 110 change `{th("unrealized_pnl", "Unrealized P&L", true)}` to `{th("unrealized_pnl_idr", "Unrealized P&L", true)}`.

2. Replace the per-row P&L computation (lines 117-119):

```tsx
                    const pnl = parseNum(p.unrealized_pnl); // native, secondary context
                    const pnlIdr = parseNum(p.unrealized_pnl_idr); // FX-aware, primary
                    const fxPnlIdr = parseNum(p.unrealized_fx_pnl_idr);
                    const costBasisIdr = parseNum(p.cost_basis_idr_total);
                    const pnlPct = costBasisIdr !== 0 ? (pnlIdr / costBasisIdr) * 100 : 0;
```

3. Replace the P&L cell (lines 162-175) — primary figure is the FX-aware IDR P&L; native P&L and the FX contribution appear as sub-lines for non-IDR instruments:

```tsx
                        <td className="r">
                          <div className={"num " + (pnlIdr >= 0 ? "gain" : "loss")} style={{ fontWeight: 500 }}>
                            {pnlIdr >= 0 ? "+" : "−"}
                            {formatIDR(Math.abs(pnlIdr))}
                            {p.fx_incomplete && (
                              <span className="badge badge-warn" style={{ marginLeft: 6 }} title="Sebagian transaksi tidak punya kurs historis — dekomposisi FX tidak lengkap">
                                FX?
                              </span>
                            )}
                          </div>
                          {currency !== "IDR" && (
                            <>
                              <div className={"t-xs num " + (pnl >= 0 ? "gain" : "loss")}>
                                {pnl >= 0 ? "+" : "−"}{formatCurrency(Math.abs(pnl), currency)}
                              </div>
                              <div className="t-xs num t-muted">
                                FX {fxPnlIdr >= 0 ? "+" : "−"}{formatIDR(Math.abs(fxPnlIdr))}
                              </div>
                            </>
                          )}
                          <div className={"t-xs num " + (pnlIdr >= 0 ? "gain" : "loss")}>
                            {formatPct(pnlPct)}
                          </div>
                        </td>
```

(The `fx` implied-rate variable at line 124 stays — it's still used by the avg-cost and latest-price cells.)

- [ ] **Step 4: Run the file's tests**

Run: `cd frontend && npx vitest run src/pages/HoldingsPage.test.tsx`
Expected: PASS — including the two pre-existing formatting tests (their fixtures have FX component 0 and `unrealized_pnl_idr` equal to the previously displayed converted figure, so the visible Rp amounts are unchanged).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/HoldingsPage.tsx frontend/src/pages/HoldingsPage.test.tsx
git commit -m "feat(holdings): FX-aware P&L per row with FX contribution sub-line"
```

---

### Task 8: DashboardPage — price/FX breakdown on the P&L card

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`
- Test: `frontend/src/pages/DashboardPage.test.tsx`

- [ ] **Step 1: Write the failing test**

Append to `frontend/src/pages/DashboardPage.test.tsx`:

```ts
test("unrealized P&L card shows the price vs FX breakdown", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "2550000", net_worth_usd: "150",
        total_unrealized_pnl_idr: "950000", total_realized_pnl_idr: "0", xirr: null,
        total_unrealized_price_pnl_idr: "850000", total_unrealized_fx_pnl_idr: "100000",
        total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
        positions: [], allocation: [],
      }),
    ),
  );
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Harga\s*Rp\s*850\.000/)).toBeInTheDocument(),
  );
  expect(screen.getByText(/FX\s*Rp\s*100\.000/)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/pages/DashboardPage.test.tsx`
Expected: FAIL — `Harga Rp 850.000` not found.

- [ ] **Step 3: Implement**

In `frontend/src/pages/DashboardPage.tsx`:

1. Add to `HeroProps` (after `unrealizedPnl: string;`):

```ts
  unrealizedPricePnl: string;
  unrealizedFxPnl: string;
```

2. Add the two params to the `HeroSection` destructuring (after `unrealizedPnl,`):

```ts
  unrealizedPricePnl,
  unrealizedFxPnl,
```

3. Replace the Unrealized P&L `StatCard`'s `sub` prop:

```tsx
          sub={
            <div className="flex col" style={{ gap: 2 }}>
              <span className={cn("stat-delta num", pnlPos ? "gain" : "loss")}>
                {pnlPos ? "▲" : "▼"} {formatPct(pnlPct)}
              </span>
              <span className="t-xs t-muted num">
                Harga {formatIDR(unrealizedPricePnl)} · FX {formatIDR(unrealizedFxPnl)}
              </span>
            </div>
          }
```

4. Pass the props at the `<HeroSection>` call site (after `unrealizedPnl={...}`):

```tsx
          unrealizedPricePnl={summary.data.total_unrealized_price_pnl_idr}
          unrealizedFxPnl={summary.data.total_unrealized_fx_pnl_idr}
```

Note: the pre-existing `pnlCostBasis = netWorth − pnl` percent (lines 132-134) stays as-is — with FX-aware P&L it now equals the purchase-rate IDR cost basis, which is exactly the right denominator.

- [ ] **Step 4: Run the file's tests**

Run: `cd frontend && npx vitest run src/pages/DashboardPage.test.tsx`
Expected: PASS — including the pre-existing percent test (its fixture has FX 0).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx frontend/src/pages/DashboardPage.test.tsx
git commit -m "feat(dashboard): show price vs FX breakdown on the unrealized P&L card"
```

---

### Task 9: PerformancePage — decomposition cards + historical chart

**Files:**
- Modify: `frontend/src/pages/PerformancePage.tsx`
- Test: `frontend/src/pages/PerformancePage.test.tsx`

- [ ] **Step 1: Write the failing test**

Append to `frontend/src/pages/PerformancePage.test.tsx` (the default msw handlers already serve `/api/portfolio/summary` with `total_unrealized_price_pnl_idr: "80"` / `fx: "20"` and `/api/portfolio/history` with `[]` — see Task 6):

```ts
test("renders the current P&L decomposition cards from the summary", async () => {
  server.use(
    http.get("/api/portfolio/performance", () =>
      HttpResponse.json({
        base: "idr",
        points: [],
        metrics: { total_return: 0, annualized: null, max_drawdown: 0, volatility: 0 },
        insufficient_data: true,
      }),
    ),
  );
  wrap();
  expect(await screen.findByText("P&L Harga")).toBeInTheDocument();
  expect(screen.getByText("P&L Kurs (FX)")).toBeInTheDocument();
  expect(screen.getByText(/Rp\s*80/)).toBeInTheDocument();
  expect(screen.getByText(/Rp\s*20/)).toBeInTheDocument();
  // History is empty -> the historical split chart is replaced by the explainer.
  expect(screen.getByText(/Dekomposisi historis terkumpul/)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/pages/PerformancePage.test.tsx`
Expected: FAIL — "P&L Harga" not found.

- [ ] **Step 3: Implement**

In `frontend/src/pages/PerformancePage.tsx`:

1. Extend imports: add `useSummary, useHistory` to the `../api/hooks` import, and `formatIDR, parseNum` from `../lib/format`:

```ts
import { usePerformance, useSummary, useHistory } from "../api/hooks";
import { formatIDR, parseNum } from "../lib/format";
```

2. In the component body (after `const performance = performanceQuery.data;`):

```ts
  const summary = useSummary();
  const history = useHistory();

  // Historical decomposition exists only for snapshots written after the FX-aware
  // P&L feature (older rows are null) — chart the rows that have it.
  const decompData = (history.data ?? [])
    .filter((s) => s.price_pnl_idr != null && s.fx_pnl_idr != null)
    .map((s) => ({
      date: s.as_of,
      pricePnl: parseNum(s.price_pnl_idr ?? "0"),
      fxPnl: parseNum(s.fx_pnl_idr ?? "0"),
    }));
```

3. After the closing `</QueryState>` of the TWR section, add the decomposition section (sibling block inside the page's root `div`):

```tsx
      <div>
        <h2 className="text-base font-semibold tracking-tight">Dekomposisi P&L</h2>
        <p className="text-sm text-muted-foreground">
          kontribusi pergerakan harga vs kurs IDR (unrealized, saat ini)
        </p>
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <StatCard
            label="P&L Harga"
            value={formatIDR(summary.data?.total_unrealized_price_pnl_idr ?? "0")}
            tone={returnTone(parseNum(summary.data?.total_unrealized_price_pnl_idr ?? "0"))}
          />
          <StatCard
            label="P&L Kurs (FX)"
            value={formatIDR(summary.data?.total_unrealized_fx_pnl_idr ?? "0")}
            tone={returnTone(parseNum(summary.data?.total_unrealized_fx_pnl_idr ?? "0"))}
          />
          <StatCard
            label="Realized Harga"
            value={formatIDR(summary.data?.total_realized_price_pnl_idr ?? "0")}
            tone={returnTone(parseNum(summary.data?.total_realized_price_pnl_idr ?? "0"))}
          />
          <StatCard
            label="Realized Kurs (FX)"
            value={formatIDR(summary.data?.total_realized_fx_pnl_idr ?? "0")}
            tone={returnTone(parseNum(summary.data?.total_realized_fx_pnl_idr ?? "0"))}
          />
        </div>
      </QueryState>

      {decompData.length >= 2 ? (
        <div className="h-64 w-full rounded-lg border bg-card p-4">
          <ResponsiveContainer width="100%" height="100%">
            <AreaChart data={decompData} margin={{ top: 10, right: 20, bottom: 0, left: 0 }}>
              <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
              <XAxis dataKey="date" fontSize={11} minTickGap={32} stroke="hsl(var(--muted-foreground))" tickLine={false} axisLine={false} />
              <YAxis tickFormatter={(value: number) => formatIDR(value)} width={86} fontSize={11} stroke="hsl(var(--muted-foreground))" tickLine={false} axisLine={false} />
              <Tooltip
                formatter={(value: number, name: string) => [formatIDR(value), name === "pricePnl" ? "Harga" : "FX"]}
                contentStyle={{
                  background: "hsl(var(--popover))",
                  border: "1px solid hsl(var(--border))",
                  borderRadius: "var(--radius)",
                  color: "hsl(var(--popover-foreground))",
                  fontSize: 12,
                }}
              />
              <Area type="monotone" dataKey="pricePnl" stroke="hsl(var(--chart-1))" strokeWidth={1.5} fill="hsl(var(--chart-1))" fillOpacity={0.15} dot={false} />
              <Area type="monotone" dataKey="fxPnl" stroke="hsl(var(--chart-2))" strokeWidth={1.5} fill="hsl(var(--chart-2))" fillOpacity={0.15} dot={false} />
            </AreaChart>
          </ResponsiveContainer>
        </div>
      ) : (
        <div className="rounded-lg border bg-card p-6 text-center">
          <p className="text-sm text-muted-foreground">
            Dekomposisi historis terkumpul seiring snapshot harian baru — grafik muncul setelah ≥2 hari data.
          </p>
        </div>
      )}
```

(Note: ratio formatters like `returnTone` accept any sign-bearing number; passing an IDR amount only selects the color, which is the intended use here. `returnTone` already exists at the top of this file.)

- [ ] **Step 4: Run the file's tests**

Run: `cd frontend && npx vitest run src/pages/PerformancePage.test.tsx`
Expected: PASS — pre-existing tests still pass (the new section renders alongside; default handlers serve summary/history).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/PerformancePage.tsx frontend/src/pages/PerformancePage.test.tsx
git commit -m "feat(performance): current price/FX P&L decomposition cards + historical split chart"
```

---

### Task 10: Remove the dead IDR/USD top-bar toggle

**Files:**
- Modify: `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Verify nothing consumes the context**

Run: `cd frontend && grep -rn "useCurrency\|BaseCurrency\|CurrencyContext\|pt-base\|SegControl" src --include="*.ts*"`
Expected: matches ONLY inside `src/components/AppShell.tsx`. If anything else matches, STOP and re-evaluate before deleting.

- [ ] **Step 2: Delete the dead code**

In `frontend/src/components/AppShell.tsx`:

1. Delete the whole "Currency context" block (lines 62-69): `BaseCurrency` type, the `createContext, useContext` import, `CurrencyCtx` interface, `CurrencyContext`, and `useCurrency`.
2. Delete the `SegControl` component (lines 91-114) — its only consumer is the topbar toggle.
3. In `Topbar` (lines 292-325): remove the `base`/`setBase` props from the signature and the `<SegControl …/>` JSX block, leaving:

```tsx
function Topbar({ onHamburger }: { onHamburger: () => void }) {
```

4. In `AppShell` (lines 375-410): remove the `base` state, `handleSetBase`, the `CurrencyContext.Provider` wrapper (keep its children), and pass only `onHamburger` to `Topbar`:

```tsx
export default function AppShell() {
  const [collapsed, setCollapsed] = useState(false);
  const [sheet, setSheet] = useState(false);

  return (
    <div className="pt-shell">
      {/* Desktop sidebar */}
      <Sidebar collapsed={collapsed} onCollapse={() => setCollapsed((c) => !c)} />

      {/* Mobile sheet + scrim */}
      <MobileSheet open={sheet} onClose={() => setSheet(false)} />

      {/* Main area */}
      <div className="pt-main">
        <Topbar onHamburger={() => setSheet(true)} />
        <div className="pt-page-scroll">
          <div className="pt-page">
            <Outlet />
          </div>
        </div>
      </div>

      {/* Mobile bottom nav */}
      <BottomNav onMore={() => setSheet(true)} />
    </div>
  );
}
```

Do NOT touch the `pt-seg` CSS class or PerformancePage's local `Segmented` control — they stay.

- [ ] **Step 3: Verify build + suite**

Run: `cd frontend && npm run build && npx vitest run`
Expected: tsc clean (catches any missed reference), all tests PASS.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/AppShell.tsx
git commit -m "refactor(frontend): remove dead IDR/USD top-bar toggle and currency context"
```

---

### Task 11: Final verification + PR

- [ ] **Step 1: Full verification**

Run: `cd backend && cargo clippy --all-targets && cargo test`
Expected: clippy clean, all tests PASS. (NO `cargo fmt`.)

Run: `cd frontend && npm run build && npx vitest run`
Expected: build clean, all tests PASS.

- [ ] **Step 2: Spec invariants spot-check**

Confirm against `docs/superpowers/specs/2026-06-05-fx-aware-pnl-design.md`:
- `price + fx == total` asserted for both realized (Task 1) and unrealized (Task 2) — yes if those tests pass.
- IDR instruments produce FX = 0 with no special case (Task 1 test).
- Missing FX → fx_rate fallback by date, else explicit `fx_incomplete` flag, never current-rate substitution (Task 3 tests).
- Old snapshots stay NULL; UI falls back (Task 5 nullable test, Task 9 explainer branch).
- Toggle removed without touching PerformancePage's local toggle (Task 10).

- [ ] **Step 3: Push and open PR**

```bash
git push -u origin feat/fx-aware-pnl
gh pr create --title "feat: FX-aware P&L decomposition (price vs FX) + remove dead currency toggle" --body "$(cat <<'EOF'
## Summary
- P&L on USD-denominated assets now captures FX gain/loss from IDR/USD movements: cost basis is tracked in IDR at purchase-time rates (txn.fx_to_idr), so `unrealized_pnl_idr = mv × fx_now − cost_idr`, decomposed into price vs FX components (invariant: price + fx = total, exact)
- Realized P&L decomposes the same way at sell time (average-cost engine)
- Daily snapshots persist the decomposition in new nullable columns (old rows stay NULL; UI falls back)
- Dashboard P&L card, Holdings rows, and Performance page show the Harga/FX breakdown
- Transactions missing a purchase-time rate are backfilled from fx_rate by date or flagged `fx_incomplete` — never silently valued at today's rate
- Removed the dead IDR/USD top-bar toggle (nothing consumed its context)

Spec: docs/superpowers/specs/2026-06-05-fx-aware-pnl-design.md

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

(Per repo memory: CI/CD auto-deploys on main push — merging the PR deploys to prod.)
