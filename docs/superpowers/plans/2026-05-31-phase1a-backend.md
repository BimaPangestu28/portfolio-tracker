# Investment Tracker — Phase 1A (Backend) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the Rust backend (domain ledger, cost-basis/XIRR/valuation/allocation logic, pricing service, REST API) that consolidates investments into net worth, performance, and allocation-vs-target — input manual, single-user, self-host.

**Architecture:** Single `axum` crate over SQLite (`sqlx`). The transaction ledger is the single source of truth; positions, valuation (dual IDR+USD), performance, and allocation are derived. Pure domain logic (cost-basis, XIRR, valuation, allocation) lives in side-effect-free functions that are unit-tested hard; IO (repos, pricing providers, HTTP) wraps them.

**Tech Stack:** Rust, axum, sqlx (SQLite), rust_decimal (money), chrono (dates), reqwest (price providers), thiserror + anyhow (errors), tokio, tracing. Money/quantities stored as TEXT (decimal strings) — never floats. No `unwrap()`/`panic!()` in production paths.

**Scope note:** This is Phase 1A of 4. Frontend dashboard = Plan 1B (separate). Auto-sync connectors, LLM OCR/statement ingestion + budgeting, and chatbot = Phases 2–4 (separate specs).

---

### Task 1: Project skeleton + health endpoint

**Files:**
- Create: `backend/Cargo.toml`
- Create: `backend/src/main.rs`
- Create: `backend/src/error.rs`

- [ ] **Step 1: Create `backend/Cargo.toml`**

```toml
[package]
name = "portfolio-tracker"
version = "0.1.0"
edition = "2021"

[dependencies]
axum = "0.7"
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.5", features = ["cors", "trace"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sqlx = { version = "0.7", features = ["runtime-tokio-rustls", "sqlite", "chrono", "macros"] }
rust_decimal = { version = "1", features = ["serde-with-str"] }
rust_decimal_macros = "1"
chrono = { version = "0.4", features = ["serde"] }
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
thiserror = "1"
anyhow = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

[dev-dependencies]
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

- [ ] **Step 2: Write `backend/src/error.rs`**

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("invalid input: {0}")]
    BadRequest(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Other(e) => {
                tracing::error!("internal error: {e:#}");
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".to_string())
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            other => AppError::Other(other.into()),
        }
    }
}
```

- [ ] **Step 3: Write `backend/src/main.rs`**

```rust
mod error;

use axum::{routing::get, Router};

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let app = Router::new().route("/health", get(health));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Verify it builds and runs**

Run: `cd backend && cargo build`
Expected: compiles clean.
Run: `cargo run &` then `curl -s localhost:8080/health` → `ok`; then `kill %1`.

- [ ] **Step 5: Commit**

```bash
cd backend && git add Cargo.toml src/ && git commit -m "feat: backend skeleton with health endpoint"
```

---

### Task 2: Database schema + migrations + pool

**Files:**
- Create: `backend/migrations/0001_init.sql`
- Create: `backend/src/db.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Write `backend/migrations/0001_init.sql`**

```sql
CREATE TABLE account (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    account_type TEXT NOT NULL,
    institution TEXT,
    native_currency TEXT NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL
);

CREATE TABLE category (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    target_pct TEXT NOT NULL,
    tolerance_band_pct TEXT,
    sort_order INTEGER NOT NULL DEFAULT 0,
    color TEXT
);

CREATE TABLE instrument (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    symbol TEXT NOT NULL,
    name TEXT NOT NULL,
    instrument_type TEXT NOT NULL,
    native_currency TEXT NOT NULL,
    category_id INTEGER REFERENCES category(id),
    price_source TEXT NOT NULL,
    decimals INTEGER NOT NULL DEFAULT 8,
    note TEXT
);

CREATE TABLE txn (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    account_id INTEGER NOT NULL REFERENCES account(id),
    instrument_id INTEGER NOT NULL REFERENCES instrument(id),
    txn_type TEXT NOT NULL,
    executed_at TEXT NOT NULL,
    quantity TEXT NOT NULL,
    price_native TEXT NOT NULL,
    fee_native TEXT NOT NULL DEFAULT '0',
    currency TEXT NOT NULL,
    fx_to_idr TEXT NOT NULL,
    fx_to_usd TEXT NOT NULL,
    note TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX idx_txn_instrument ON txn(instrument_id, executed_at);

CREATE TABLE price_quote (
    instrument_id INTEGER NOT NULL REFERENCES instrument(id),
    as_of TEXT NOT NULL,
    price_native TEXT NOT NULL,
    currency TEXT NOT NULL,
    source TEXT NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY (instrument_id, as_of, kind)
);

CREATE TABLE fx_rate (
    as_of TEXT NOT NULL,
    base TEXT NOT NULL,
    quote TEXT NOT NULL,
    rate TEXT NOT NULL,
    PRIMARY KEY (as_of, base, quote)
);

CREATE TABLE valuation_snapshot (
    as_of TEXT PRIMARY KEY,
    total_idr TEXT NOT NULL,
    total_usd TEXT NOT NULL,
    breakdown_json TEXT NOT NULL
);
```

- [ ] **Step 2: Write `backend/src/db.rs`**

```rust
use sqlx::sqlite::{SqlitePool, SqlitePoolOptions, SqliteConnectOptions};
use std::str::FromStr;

pub type Db = SqlitePool;

pub async fn connect(url: &str) -> anyhow::Result<Db> {
    let opts = SqliteConnectOptions::from_str(url)?.create_if_missing(true);
    let pool = SqlitePoolOptions::new().max_connections(5).connect_with(opts).await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    Ok(pool)
}
```

- [ ] **Step 3: Wire pool into `main.rs`**

Replace the body of `main` so it builds the pool and stores it as router state. Add `mod db;` at top.

```rust
mod db;
mod error;

use axum::{routing::get, Router};
use db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

async fn health() -> &'static str { "ok" }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://portfolio.db".into());
    let db = db::connect(&url).await?;
    let state = AppState { db };
    let app = Router::new().route("/health", get(health)).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 4: Verify migrations apply**

Run: `cd backend && DATABASE_URL="sqlite://test.db" cargo run &` ; wait 2s ; `sqlite3 test.db ".tables"` → lists `account category fx_rate instrument price_quote txn valuation_snapshot`; `kill %1; rm test.db`.

- [ ] **Step 5: Commit**

```bash
git add migrations/ src/db.rs src/main.rs && git commit -m "feat: sqlite schema, migrations, and pool"
```

---

### Task 3: Domain models + enums

**Files:**
- Create: `backend/src/domain/mod.rs`
- Create: `backend/src/domain/models.rs`
- Test: `backend/src/domain/models.rs` (inline `#[cfg(test)]`)
- Modify: `backend/src/main.rs` (add `mod domain;`)

- [ ] **Step 1: Write the failing test (enum round-trips via string)**

In `backend/src/domain/models.rs`:

```rust
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

macro_rules! str_enum {
    ($name:ident { $($variant:ident => $s:literal),+ $(,)? }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn as_str(&self) -> &'static str { match self { $(Self::$variant => $s),+ } }
        }
        impl FromStr for $name {
            type Err = String;
            fn from_str(s: &str) -> Result<Self, String> {
                match s { $($s => Ok(Self::$variant),)+ other => Err(format!("invalid {}: {}", stringify!($name), other)) }
            }
        }
    };
}

str_enum!(AccountType { Exchange => "exchange", Broker => "broker", Bank => "bank", Wallet => "wallet", Manual => "manual" });
str_enum!(InstrumentType { Crypto => "crypto", StockId => "stock_id", StockUs => "stock_us", Etf => "etf", MutualFund => "mutual_fund", Cash => "cash", Bond => "bond", Gold => "gold", Other => "other" });
str_enum!(TxnType { Buy => "buy", Sell => "sell", Dividend => "dividend", Interest => "interest", Fee => "fee", Deposit => "deposit", Withdrawal => "withdrawal", OpeningBalance => "opening_balance" });

#[derive(Debug, Clone)]
pub struct Transaction {
    pub id: i64,
    pub account_id: i64,
    pub instrument_id: i64,
    pub txn_type: TxnType,
    pub executed_at: DateTime<Utc>,
    pub quantity: Decimal,
    pub price_native: Decimal,
    pub fee_native: Decimal,
    pub currency: String,
    pub fx_to_idr: Decimal,
    pub fx_to_usd: Decimal,
    pub note: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn txn_type_round_trip() {
        for t in ["buy","sell","dividend","interest","fee","deposit","withdrawal","opening_balance"] {
            assert_eq!(TxnType::from_str(t).unwrap().as_str(), t);
        }
    }
    #[test]
    fn invalid_txn_type_errors() {
        assert!(TxnType::from_str("nope").is_err());
    }
}
```

- [ ] **Step 2: Write `backend/src/domain/mod.rs`**

```rust
pub mod models;
pub mod cost_basis;
pub mod xirr;
pub mod valuation;
pub mod allocation;
```

(`cost_basis`, `xirr`, `valuation`, `allocation` are created in later tasks; comment them out until then or create empty files now: `touch src/domain/cost_basis.rs src/domain/xirr.rs src/domain/valuation.rs src/domain/allocation.rs`.)

Add `mod domain;` to `main.rs`.

- [ ] **Step 3: Run tests**

Run: `cargo test domain::models`
Expected: `txn_type_round_trip` and `invalid_txn_type_errors` PASS.

- [ ] **Step 4: Commit**

```bash
git add src/domain/ src/main.rs && git commit -m "feat: domain models and string enums"
```

---

### Task 4: Cost-basis engine (average cost)

**Files:**
- Create/replace: `backend/src/domain/cost_basis.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::domain::models::{Transaction, TxnType};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

#[derive(Debug, Clone, PartialEq)]
pub struct CostBasis {
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub cost_basis_total: Decimal,
    pub realized_pnl: Decimal,
    pub income: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn tx(t: TxnType, qty: Decimal, price: Decimal, fee: Decimal) -> Transaction {
        Transaction {
            id: 0, account_id: 1, instrument_id: 1, txn_type: t,
            executed_at: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
            quantity: qty, price_native: price, fee_native: fee,
            currency: "USD".into(), fx_to_idr: dec!(16000), fx_to_usd: dec!(1), note: None,
        }
    }

    #[test]
    fn buy_then_buy_averages_cost() {
        let txns = vec![ tx(TxnType::Buy, dec!(1), dec!(100), dec!(0)),
                         tx(TxnType::Buy, dec!(1), dec!(200), dec!(0)) ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(2));
        assert_eq!(cb.avg_cost, dec!(150));
        assert_eq!(cb.cost_basis_total, dec!(300));
        assert_eq!(cb.realized_pnl, dec!(0));
    }

    #[test]
    fn fee_increases_cost_basis() {
        let txns = vec![ tx(TxnType::Buy, dec!(1), dec!(100), dec!(10)) ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.avg_cost, dec!(110));
        assert_eq!(cb.cost_basis_total, dec!(110));
    }

    #[test]
    fn sell_realizes_pnl_at_average() {
        let txns = vec![ tx(TxnType::Buy, dec!(2), dec!(100), dec!(0)),
                         tx(TxnType::Sell, dec!(1), dec!(150), dec!(0)) ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(1));
        assert_eq!(cb.avg_cost, dec!(100));
        assert_eq!(cb.realized_pnl, dec!(50));
    }

    #[test]
    fn dividend_is_income_not_position() {
        let txns = vec![ tx(TxnType::Buy, dec!(1), dec!(100), dec!(0)),
                         tx(TxnType::Dividend, dec!(1), dec!(5), dec!(0)) ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(1));
        assert_eq!(cb.income, dec!(5));
    }

    #[test]
    fn opening_balance_seeds_position() {
        let txns = vec![ tx(TxnType::OpeningBalance, dec!(3), dec!(50), dec!(0)) ];
        let cb = compute_cost_basis(&txns);
        assert_eq!(cb.quantity, dec!(3));
        assert_eq!(cb.avg_cost, dec!(50));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test domain::cost_basis`
Expected: FAIL — `compute_cost_basis` not found.

- [ ] **Step 3: Implement `compute_cost_basis`** (add above the tests module)

```rust
/// Average-cost engine. `txns` MUST be sorted ascending by `executed_at`.
pub fn compute_cost_basis(txns: &[Transaction]) -> CostBasis {
    let mut qty = Decimal::ZERO;
    let mut avg = Decimal::ZERO;
    let mut realized = Decimal::ZERO;
    let mut income = Decimal::ZERO;

    for t in txns {
        match t.txn_type {
            TxnType::Buy | TxnType::OpeningBalance | TxnType::Deposit => {
                let added_cost = t.quantity * t.price_native + t.fee_native;
                let new_qty = qty + t.quantity;
                if new_qty.is_zero() {
                    avg = Decimal::ZERO;
                } else {
                    avg = (qty * avg + added_cost) / new_qty;
                }
                qty = new_qty;
            }
            TxnType::Sell | TxnType::Withdrawal => {
                realized += (t.price_native - avg) * t.quantity - t.fee_native;
                qty -= t.quantity;
                if qty <= Decimal::ZERO { qty = Decimal::ZERO; }
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
    }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test domain::cost_basis`
Expected: all 5 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/cost_basis.rs && git commit -m "feat: average-cost basis engine with realized P&L"
```

---

### Task 5: XIRR (annualized return)

**Files:**
- Create/replace: `backend/src/domain/xirr.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use chrono::NaiveDate;

#[derive(Debug, Clone, Copy)]
pub struct CashFlow {
    pub date: NaiveDate,
    /// Negative = money out (invested), positive = money in (returned / current value).
    pub amount: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn d(y: i32, m: u32, day: u32) -> NaiveDate { NaiveDate::from_ymd_opt(y, m, day).unwrap() }

    #[test]
    fn doubling_in_one_year_is_about_100pct() {
        let flows = vec![
            CashFlow { date: d(2025, 1, 1), amount: -100.0 },
            CashFlow { date: d(2026, 1, 1), amount: 200.0 },
        ];
        let r = xirr(&flows).unwrap();
        assert!((r - 1.0).abs() < 0.01, "got {r}");
    }

    #[test]
    fn flat_value_is_about_zero() {
        let flows = vec![
            CashFlow { date: d(2025, 1, 1), amount: -100.0 },
            CashFlow { date: d(2026, 1, 1), amount: 100.0 },
        ];
        let r = xirr(&flows).unwrap();
        assert!(r.abs() < 0.01, "got {r}");
    }

    #[test]
    fn no_sign_change_returns_none() {
        let flows = vec![
            CashFlow { date: d(2025, 1, 1), amount: -100.0 },
            CashFlow { date: d(2026, 1, 1), amount: -50.0 },
        ];
        assert!(xirr(&flows).is_none());
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test domain::xirr`
Expected: FAIL — `xirr` not found.

- [ ] **Step 3: Implement `xirr`** (Newton-Raphson with bisection fallback)

```rust
fn npv(rate: f64, flows: &[CashFlow], t0: NaiveDate) -> f64 {
    flows.iter().map(|f| {
        let years = (f.date - t0).num_days() as f64 / 365.0;
        f.amount / (1.0 + rate).powf(years)
    }).sum()
}

/// Returns annualized internal rate of return as a fraction (0.10 = 10%).
/// `None` if there is no sign change or it fails to converge.
pub fn xirr(flows: &[CashFlow]) -> Option<f64> {
    if flows.len() < 2 { return None; }
    let has_pos = flows.iter().any(|f| f.amount > 0.0);
    let has_neg = flows.iter().any(|f| f.amount < 0.0);
    if !(has_pos && has_neg) { return None; }

    let t0 = flows.iter().map(|f| f.date).min()?;

    // Newton-Raphson
    let mut rate = 0.1;
    for _ in 0..100 {
        let f = npv(rate, flows, t0);
        let df = (npv(rate + 1e-6, flows, t0) - f) / 1e-6;
        if df.abs() < 1e-12 { break; }
        let next = rate - f / df;
        if !next.is_finite() { break; }
        if (next - rate).abs() < 1e-7 { return Some(next); }
        rate = next;
    }

    // Bisection fallback over a sane range
    let (mut lo, mut hi) = (-0.9999_f64, 10.0_f64);
    let (mut flo, fhi) = (npv(lo, flows, t0), npv(hi, flows, t0));
    if flo * fhi > 0.0 { return None; }
    for _ in 0..200 {
        let mid = (lo + hi) / 2.0;
        let fmid = npv(mid, flows, t0);
        if fmid.abs() < 1e-7 { return Some(mid); }
        if flo * fmid < 0.0 { hi = mid; } else { lo = mid; flo = fmid; }
    }
    Some((lo + hi) / 2.0)
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test domain::xirr`
Expected: 3 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/xirr.rs && git commit -m "feat: XIRR via newton-raphson with bisection fallback"
```

---

### Task 6: Valuation (positions, dual currency, performance)

**Files:**
- Create/replace: `backend/src/domain/valuation.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use crate::domain::cost_basis::{compute_cost_basis, CostBasis};
use crate::domain::models::Transaction;
use rust_decimal::Decimal;

#[derive(Debug, Clone, serde::Serialize)]
pub struct Position {
    pub instrument_id: i64,
    pub quantity: Decimal,
    pub avg_cost: Decimal,
    pub cost_basis_total: Decimal,
    pub latest_price: Decimal,
    pub price_stale: bool,
    pub market_value_native: Decimal,
    pub market_value_idr: Decimal,
    pub market_value_usd: Decimal,
    pub unrealized_pnl: Decimal,
    pub realized_pnl: Decimal,
    pub income: Decimal,
}

/// Latest price + FX context for one instrument at valuation time.
#[derive(Debug, Clone)]
pub struct PriceContext {
    pub instrument_id: i64,
    pub latest_price_native: Decimal,
    pub price_stale: bool,
    pub fx_native_to_idr: Decimal,
    pub fx_native_to_usd: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::TxnType;
    use chrono::{TimeZone, Utc};
    use rust_decimal_macros::dec;

    fn tx(t: TxnType, qty: Decimal, price: Decimal) -> Transaction {
        Transaction { id: 0, account_id: 1, instrument_id: 7, txn_type: t,
            executed_at: Utc.with_ymd_and_hms(2026,1,1,0,0,0).unwrap(),
            quantity: qty, price_native: price, fee_native: dec!(0),
            currency: "USD".into(), fx_to_idr: dec!(16000), fx_to_usd: dec!(1), note: None }
    }

    #[test]
    fn position_values_in_dual_currency() {
        let txns = vec![ tx(TxnType::Buy, dec!(2), dec!(100)) ];
        let cb = compute_cost_basis(&txns);
        let ctx = PriceContext { instrument_id: 7, latest_price_native: dec!(150),
            price_stale: false, fx_native_to_idr: dec!(16000), fx_native_to_usd: dec!(1) };
        let p = build_position(7, &cb, &ctx);
        assert_eq!(p.market_value_native, dec!(300));
        assert_eq!(p.market_value_usd, dec!(300));
        assert_eq!(p.market_value_idr, dec!(4800000));
        assert_eq!(p.unrealized_pnl, dec!(100)); // 300 - 200 cost
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test domain::valuation`
Expected: FAIL — `build_position` not found.

- [ ] **Step 3: Implement `build_position`**

```rust
pub fn build_position(instrument_id: i64, cb: &CostBasis, ctx: &PriceContext) -> Position {
    let mv_native = cb.quantity * ctx.latest_price_native;
    Position {
        instrument_id,
        quantity: cb.quantity,
        avg_cost: cb.avg_cost,
        cost_basis_total: cb.cost_basis_total,
        latest_price: ctx.latest_price_native,
        price_stale: ctx.price_stale,
        market_value_native: mv_native,
        market_value_idr: mv_native * ctx.fx_native_to_idr,
        market_value_usd: mv_native * ctx.fx_native_to_usd,
        unrealized_pnl: mv_native - cb.cost_basis_total,
        realized_pnl: cb.realized_pnl,
        income: cb.income,
    }
}

/// Helper: group transactions by instrument id, preserving order.
pub fn group_by_instrument(txns: Vec<Transaction>) -> std::collections::BTreeMap<i64, Vec<Transaction>> {
    let mut map: std::collections::BTreeMap<i64, Vec<Transaction>> = std::collections::BTreeMap::new();
    for t in txns { map.entry(t.instrument_id).or_default().push(t); }
    map
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test domain::valuation`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/valuation.rs && git commit -m "feat: dual-currency position valuation"
```

---

### Task 7: Allocation planner (target vs actual + drift)

**Files:**
- Create/replace: `backend/src/domain/allocation.rs`

- [ ] **Step 1: Write the failing tests**

```rust
use rust_decimal::Decimal;

#[derive(Debug, Clone, serde::Serialize)]
pub struct CategoryAllocation {
    pub category_id: i64,
    pub name: String,
    pub target_pct: Decimal,
    pub tolerance_band_pct: Option<Decimal>,
    pub actual_pct: Decimal,
    pub actual_value_idr: Decimal,
    pub drift_pct: Decimal,         // actual - target
    pub out_of_band: bool,
    pub rebalance_idr: Decimal,     // +buy / -sell to hit target
}

pub struct CategoryInput {
    pub category_id: i64,
    pub name: String,
    pub target_pct: Decimal,
    pub tolerance_band_pct: Option<Decimal>,
    pub value_idr: Decimal,
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn computes_drift_and_out_of_band() {
        let cats = vec![
            CategoryInput { category_id: 1, name: "USD ETF".into(), target_pct: dec!(50), tolerance_band_pct: Some(dec!(5)), value_idr: dec!(400) },
            CategoryInput { category_id: 2, name: "Saham ID".into(), target_pct: dec!(50), tolerance_band_pct: Some(dec!(5)), value_idr: dec!(600) },
        ];
        let out = compute_allocation(&cats);
        // total = 1000; cat1 actual 40%, target 50% => drift -10, out of band (|10|>5)
        let c1 = &out[0];
        assert_eq!(c1.actual_pct, dec!(40));
        assert_eq!(c1.drift_pct, dec!(-10));
        assert!(c1.out_of_band);
        assert_eq!(c1.rebalance_idr, dec!(100)); // need +100 to reach 500
    }

    #[test]
    fn empty_portfolio_is_zero_not_panic() {
        let cats = vec![ CategoryInput { category_id: 1, name: "X".into(), target_pct: dec!(100), tolerance_band_pct: None, value_idr: dec!(0) } ];
        let out = compute_allocation(&cats);
        assert_eq!(out[0].actual_pct, dec!(0));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test domain::allocation`
Expected: FAIL — `compute_allocation` not found.

- [ ] **Step 3: Implement `compute_allocation`**

```rust
pub fn compute_allocation(cats: &[CategoryInput]) -> Vec<CategoryAllocation> {
    let total: Decimal = cats.iter().map(|c| c.value_idr).sum();
    let hundred = Decimal::from(100);
    cats.iter().map(|c| {
        let actual_pct = if total.is_zero() { Decimal::ZERO } else { c.value_idr / total * hundred };
        let drift = actual_pct - c.target_pct;
        let out_of_band = match c.tolerance_band_pct {
            Some(band) => drift.abs() > band,
            None => false,
        };
        let target_value = total * c.target_pct / hundred;
        CategoryAllocation {
            category_id: c.category_id,
            name: c.name.clone(),
            target_pct: c.target_pct,
            tolerance_band_pct: c.tolerance_band_pct,
            actual_pct,
            actual_value_idr: c.value_idr,
            drift_pct: drift,
            out_of_band,
            rebalance_idr: target_value - c.value_idr,
        }
    }).collect()
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test domain::allocation`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/domain/allocation.rs && git commit -m "feat: allocation planner with drift and rebalance hint"
```

---

### Task 8: Repositories (accounts, categories, instruments)

**Files:**
- Create: `backend/src/repo/mod.rs`
- Create: `backend/src/repo/accounts.rs`
- Create: `backend/src/repo/categories.rs`
- Create: `backend/src/repo/instruments.rs`
- Modify: `backend/src/main.rs` (`mod repo;`)

- [ ] **Step 1: Write `backend/src/repo/mod.rs`** with shared DTOs

```rust
pub mod accounts;
pub mod categories;
pub mod instruments;
pub mod transactions;
pub mod prices;

use rust_decimal::Decimal;
use std::str::FromStr;

/// Parse a TEXT decimal column into Decimal, mapping errors to anyhow.
pub fn dec(s: &str) -> anyhow::Result<Decimal> {
    Decimal::from_str(s).map_err(|e| anyhow::anyhow!("bad decimal '{s}': {e}"))
}
```

- [ ] **Step 2: Write `backend/src/repo/accounts.rs`** with an integration test

```rust
use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AccountRow {
    pub id: i64,
    pub name: String,
    pub account_type: String,
    pub institution: Option<String>,
    pub native_currency: String,
    pub note: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct NewAccount {
    pub name: String,
    pub account_type: String,
    pub institution: Option<String>,
    pub native_currency: String,
    pub note: Option<String>,
}

pub async fn create(db: &Db, a: &NewAccount) -> anyhow::Result<AccountRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO account (name, account_type, institution, native_currency, note, created_at) VALUES (?,?,?,?,?,?)")
        .bind(&a.name).bind(&a.account_type).bind(&a.institution)
        .bind(&a.native_currency).bind(&a.note).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<AccountRow> {
    let row = sqlx::query_as::<_, AccountRow>("SELECT * FROM account WHERE id = ?")
        .bind(id).fetch_one(db).await?;
    Ok(row)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<AccountRow>> {
    let rows = sqlx::query_as::<_, AccountRow>("SELECT * FROM account ORDER BY id")
        .fetch_all(db).await?;
    Ok(rows)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM account WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn create_and_list_account() {
        let db = mem_db().await;
        let a = NewAccount { name: "Binance".into(), account_type: "exchange".into(),
            institution: None, native_currency: "USD".into(), note: None };
        let created = create(&db, &a).await.unwrap();
        assert_eq!(created.name, "Binance");
        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
```

- [ ] **Step 3: Run to verify failure, then it should pass once compiled**

Run: `cargo test repo::accounts`
Expected: PASS (`create_and_list_account`). If `sqlx::memory` migration path fails, ensure `db::connect` runs `migrate!` (it does).

- [ ] **Step 4: Write `categories.rs` and `instruments.rs`** following the same pattern

`backend/src/repo/categories.rs`:

```rust
use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct CategoryRow {
    pub id: i64,
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub sort_order: i64,
    pub color: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewCategory {
    pub name: String,
    pub target_pct: String,
    pub tolerance_band_pct: Option<String>,
    pub sort_order: Option<i64>,
    pub color: Option<String>,
}

pub async fn create(db: &Db, c: &NewCategory) -> anyhow::Result<CategoryRow> {
    let id = sqlx::query(
        "INSERT INTO category (name, target_pct, tolerance_band_pct, sort_order, color) VALUES (?,?,?,?,?)")
        .bind(&c.name).bind(&c.target_pct).bind(&c.tolerance_band_pct)
        .bind(c.sort_order.unwrap_or(0)).bind(&c.color)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<CategoryRow> {
    Ok(sqlx::query_as::<_, CategoryRow>("SELECT * FROM category WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<CategoryRow>> {
    Ok(sqlx::query_as::<_, CategoryRow>("SELECT * FROM category ORDER BY sort_order, id").fetch_all(db).await?)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM category WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}
```

`backend/src/repo/instruments.rs`:

```rust
use crate::db::Db;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InstrumentRow {
    pub id: i64,
    pub symbol: String,
    pub name: String,
    pub instrument_type: String,
    pub native_currency: String,
    pub category_id: Option<i64>,
    pub price_source: String,
    pub decimals: i64,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct NewInstrument {
    pub symbol: String,
    pub name: String,
    pub instrument_type: String,
    pub native_currency: String,
    pub category_id: Option<i64>,
    pub price_source: String,
    pub decimals: Option<i64>,
    pub note: Option<String>,
}

pub async fn create(db: &Db, i: &NewInstrument) -> anyhow::Result<InstrumentRow> {
    let id = sqlx::query(
        "INSERT INTO instrument (symbol, name, instrument_type, native_currency, category_id, price_source, decimals, note) VALUES (?,?,?,?,?,?,?,?)")
        .bind(&i.symbol).bind(&i.name).bind(&i.instrument_type).bind(&i.native_currency)
        .bind(i.category_id).bind(&i.price_source).bind(i.decimals.unwrap_or(8)).bind(&i.note)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<InstrumentRow> {
    Ok(sqlx::query_as::<_, InstrumentRow>("SELECT * FROM instrument WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<InstrumentRow>> {
    Ok(sqlx::query_as::<_, InstrumentRow>("SELECT * FROM instrument ORDER BY id").fetch_all(db).await?)
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM instrument WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}
```

Add `mod repo;` to `main.rs`.

- [ ] **Step 5: Run all repo tests and commit**

Run: `cargo test repo::`
Expected: PASS.

```bash
git add src/repo/ src/main.rs && git commit -m "feat: repositories for accounts, categories, instruments"
```

---

### Task 9: Transactions repository (with Decimal mapping)

**Files:**
- Create: `backend/src/repo/transactions.rs`

- [ ] **Step 1: Write the failing test**

```rust
use crate::db::Db;
use crate::domain::models::{Transaction, TxnType};
use crate::repo::dec;
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
pub struct NewTransaction {
    pub account_id: i64,
    pub instrument_id: i64,
    pub txn_type: String,
    pub executed_at: DateTime<Utc>,
    pub quantity: String,
    pub price_native: String,
    pub fee_native: Option<String>,
    pub currency: String,
    pub fx_to_idr: String,
    pub fx_to_usd: String,
    pub note: Option<String>,
}

#[derive(sqlx::FromRow)]
struct TxnRowRaw {
    id: i64, account_id: i64, instrument_id: i64, txn_type: String,
    executed_at: String, quantity: String, price_native: String, fee_native: String,
    currency: String, fx_to_idr: String, fx_to_usd: String, note: Option<String>,
}

impl TxnRowRaw {
    fn into_domain(self) -> anyhow::Result<Transaction> {
        Ok(Transaction {
            id: self.id, account_id: self.account_id, instrument_id: self.instrument_id,
            txn_type: TxnType::from_str(&self.txn_type).map_err(|e| anyhow::anyhow!(e))?,
            executed_at: DateTime::parse_from_rfc3339(&self.executed_at)?.with_timezone(&Utc),
            quantity: dec(&self.quantity)?, price_native: dec(&self.price_native)?,
            fee_native: dec(&self.fee_native)?, currency: self.currency,
            fx_to_idr: dec(&self.fx_to_idr)?, fx_to_usd: dec(&self.fx_to_usd)?, note: self.note,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::{accounts, instruments};
    use rust_decimal_macros::dec as d;

    #[tokio::test]
    async fn insert_and_load_transactions_as_domain() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"BTC".into(), name:"Bitcoin".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        let nt = NewTransaction { account_id: acc.id, instrument_id: ins.id, txn_type:"buy".into(),
            executed_at: Utc::now(), quantity:"0.5".into(), price_native:"100".into(),
            fee_native: Some("1".into()), currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None };
        create(&db, &nt).await.unwrap();
        let all = list_for_instrument(&db, ins.id).await.unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].quantity, d!(0.5));
        assert_eq!(all[0].fee_native, d!(1));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test repo::transactions`
Expected: FAIL — `create` / `list_for_instrument` not found.

- [ ] **Step 3: Implement create / list functions**

```rust
pub async fn create(db: &Db, t: &NewTransaction) -> anyhow::Result<Transaction> {
    // Validate type before insert.
    TxnType::from_str(&t.txn_type).map_err(|e| anyhow::anyhow!(e))?;
    let now = Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO txn (account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note, created_at) VALUES (?,?,?,?,?,?,?,?,?,?,?,?)")
        .bind(t.account_id).bind(t.instrument_id).bind(&t.txn_type)
        .bind(t.executed_at.to_rfc3339()).bind(&t.quantity).bind(&t.price_native)
        .bind(t.fee_native.clone().unwrap_or_else(|| "0".into()))
        .bind(&t.currency).bind(&t.fx_to_idr).bind(&t.fx_to_usd).bind(&t.note).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<Transaction> {
    let raw = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE id = ?")
        .bind(id).fetch_one(db).await?;
    raw.into_domain()
}

pub async fn list_all(db: &Db) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn ORDER BY executed_at")
        .fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

pub async fn list_for_instrument(db: &Db, instrument_id: i64) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>("SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note FROM txn WHERE instrument_id = ? ORDER BY executed_at")
        .bind(instrument_id).fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}

pub async fn delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM txn WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}
```

Add `pub mod transactions;` to `repo/mod.rs` (already declared in Task 8 Step 1).

- [ ] **Step 4: Run tests**

Run: `cargo test repo::transactions`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/repo/transactions.rs && git commit -m "feat: transactions repo with decimal/domain mapping"
```

---

### Task 10: Price cache repo + FX repo

**Files:**
- Create: `backend/src/repo/prices.rs`

- [ ] **Step 1: Write the failing test**

```rust
use crate::db::Db;
use crate::repo::dec;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct LatestPrice { pub price: Decimal, pub as_of: String, pub source: String }

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec as d;
    #[tokio::test]
    async fn upsert_then_read_latest_price() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert_latest(&db, 1, d!(123.45), "USD", "coingecko", "2026-05-31").await.unwrap();
        upsert_latest(&db, 1, d!(130), "USD", "coingecko", "2026-06-01").await.unwrap();
        let p = latest(&db, 1).await.unwrap().unwrap();
        assert_eq!(p.price, d!(130));
    }
    #[tokio::test]
    async fn fx_round_trip() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert_fx(&db, "USD", "IDR", d!(16250), "2026-05-31").await.unwrap();
        assert_eq!(latest_fx(&db, "USD", "IDR").await.unwrap().unwrap(), d!(16250));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test repo::prices`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement**

```rust
pub async fn upsert_latest(db: &Db, instrument_id: i64, price: Decimal, currency: &str, source: &str, as_of: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO price_quote (instrument_id, as_of, price_native, currency, source, kind) VALUES (?,?,?,?,?, 'latest')
         ON CONFLICT(instrument_id, as_of, kind) DO UPDATE SET price_native=excluded.price_native, source=excluded.source")
        .bind(instrument_id).bind(as_of).bind(price.to_string()).bind(currency).bind(source)
        .execute(db).await?;
    Ok(())
}

pub async fn latest(db: &Db, instrument_id: i64) -> anyhow::Result<Option<LatestPrice>> {
    let row = sqlx::query_as::<_, (String, String, String)>(
        "SELECT price_native, as_of, source FROM price_quote WHERE instrument_id = ? AND kind='latest' ORDER BY as_of DESC LIMIT 1")
        .bind(instrument_id).fetch_optional(db).await?;
    match row {
        Some((p, as_of, source)) => Ok(Some(LatestPrice { price: dec(&p)?, as_of, source })),
        None => Ok(None),
    }
}

pub async fn upsert_fx(db: &Db, base: &str, quote: &str, rate: Decimal, as_of: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO fx_rate (as_of, base, quote, rate) VALUES (?,?,?,?)
         ON CONFLICT(as_of, base, quote) DO UPDATE SET rate=excluded.rate")
        .bind(as_of).bind(base).bind(quote).bind(rate.to_string())
        .execute(db).await?;
    Ok(())
}

pub async fn latest_fx(db: &Db, base: &str, quote: &str) -> anyhow::Result<Option<Decimal>> {
    let row = sqlx::query_as::<_, (String,)>(
        "SELECT rate FROM fx_rate WHERE base=? AND quote=? ORDER BY as_of DESC LIMIT 1")
        .bind(base).bind(quote).fetch_optional(db).await?;
    match row { Some((r,)) => Ok(Some(dec(&r)?)), None => Ok(None) }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test repo::prices`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/repo/prices.rs && git commit -m "feat: price quote cache and fx rate repos"
```

---

### Task 11: PriceProvider trait + provider error

**Files:**
- Create: `backend/src/pricing/mod.rs`
- Modify: `backend/src/main.rs` (`mod pricing;`)

- [ ] **Step 1: Write `backend/src/pricing/mod.rs`**

```rust
pub mod coingecko;
pub mod fx;

use rust_decimal::Decimal;

#[derive(Debug, thiserror::Error)]
pub enum PriceError {
    #[error("http error: {0}")]
    Http(String),
    #[error("price not found for {0}")]
    NotFound(String),
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Clone)]
pub struct Quote {
    pub price: Decimal,
    pub currency: String,
}

#[async_trait::async_trait]
pub trait PriceProvider: Send + Sync {
    /// `ext_id` is the provider-specific id, e.g. "bitcoin" for CoinGecko.
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError>;
}
```

- [ ] **Step 2: Add `async-trait` to Cargo.toml**

Add under `[dependencies]`: `async-trait = "0.1"`.

- [ ] **Step 3: Verify it builds**

Run: `cargo build`
Expected: compiles (providers referenced in `mod` are created next; create empty `coingecko.rs`/`fx.rs` with `// placeholder` so the module resolves, or implement Task 12/13 before building).

- [ ] **Step 4: Commit**

```bash
git add src/pricing/mod.rs Cargo.toml src/main.rs && git commit -m "feat: PriceProvider trait and quote types"
```

---

### Task 12: CoinGecko + FX providers

**Files:**
- Create: `backend/src/pricing/coingecko.rs`
- Create: `backend/src/pricing/fx.rs`

- [ ] **Step 1: Write `coingecko.rs`** (parsing unit-testable, network call separate)

```rust
use super::{PriceError, PriceProvider, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct CoinGecko { base: String, client: reqwest::Client }

impl CoinGecko {
    pub fn new() -> Self {
        Self { base: "https://api.coingecko.com/api/v3".into(), client: reqwest::Client::new() }
    }
}

/// Pure parser: CoinGecko simple/price JSON -> Quote. Unit-tested without network.
pub fn parse_simple_price(body: &serde_json::Value, ext_id: &str, vs: &str) -> Result<Quote, PriceError> {
    let v = body.get(ext_id).and_then(|o| o.get(vs))
        .ok_or_else(|| PriceError::NotFound(ext_id.into()))?;
    let s = v.to_string();
    let price = Decimal::from_str(s.trim_matches('"')).map_err(|e| PriceError::Parse(e.to_string()))?;
    Ok(Quote { price, currency: vs.to_uppercase() })
}

#[async_trait::async_trait]
impl PriceProvider for CoinGecko {
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError> {
        let url = format!("{}/simple/price?ids={}&vs_currencies=usd", self.base, ext_id);
        let resp = self.client.get(&url).send().await.map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_simple_price(&body, ext_id, "usd")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    #[test]
    fn parses_simple_price() {
        let body = serde_json::json!({ "bitcoin": { "usd": 67000.5 } });
        let q = parse_simple_price(&body, "bitcoin", "usd").unwrap();
        assert_eq!(q.price, dec!(67000.5));
        assert_eq!(q.currency, "USD");
    }
    #[test]
    fn missing_id_is_not_found() {
        let body = serde_json::json!({});
        assert!(matches!(parse_simple_price(&body, "bitcoin", "usd"), Err(PriceError::NotFound(_))));
    }
}
```

- [ ] **Step 2: Write `fx.rs`** (USD→IDR via exchangerate.host-style JSON; pure parser tested)

```rust
use super::PriceError;
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct FxClient { base: String, client: reqwest::Client }

impl FxClient {
    pub fn new() -> Self {
        Self { base: "https://api.exchangerate.host".into(), client: reqwest::Client::new() }
    }
    pub async fn usd_to_idr(&self) -> Result<Decimal, PriceError> {
        let url = format!("{}/latest?base=USD&symbols=IDR", self.base);
        let resp = self.client.get(&url).send().await.map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_fx(&body, "IDR")
    }
}

pub fn parse_fx(body: &serde_json::Value, symbol: &str) -> Result<Decimal, PriceError> {
    let v = body.get("rates").and_then(|r| r.get(symbol))
        .ok_or_else(|| PriceError::NotFound(symbol.into()))?;
    Decimal::from_str(v.to_string().trim_matches('"')).map_err(|e| PriceError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    #[test]
    fn parses_fx_rate() {
        let body = serde_json::json!({ "rates": { "IDR": 16250.0 } });
        assert_eq!(parse_fx(&body, "IDR").unwrap(), dec!(16250));
    }
}
```

- [ ] **Step 3: Run tests**

Run: `cargo test pricing::`
Expected: parser tests PASS (no network used).

- [ ] **Step 4: Commit**

```bash
git add src/pricing/coingecko.rs src/pricing/fx.rs && git commit -m "feat: coingecko and fx providers with pure parsers"
```

---

### Task 13: Pricing service — refresh + stale detection

**Files:**
- Create: `backend/src/pricing/service.rs`
- Modify: `backend/src/pricing/mod.rs` (`pub mod service;`)

- [ ] **Step 1: Write the failing test (stale logic is pure)**

In `service.rs`:

```rust
use chrono::{DateTime, Duration, Utc};

/// A price is stale if older than `max_age_hours`.
pub fn is_stale(as_of: &str, now: DateTime<Utc>, max_age_hours: i64) -> bool {
    match DateTime::parse_from_rfc3339(as_of).or_else(|_| DateTime::parse_from_rfc3339(&format!("{as_of}T00:00:00+00:00"))) {
        Ok(t) => now.signed_duration_since(t.with_timezone(&Utc)) > Duration::hours(max_age_hours),
        Err(_) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    #[test]
    fn fresh_price_not_stale() {
        let now = Utc.with_ymd_and_hms(2026,5,31,12,0,0).unwrap();
        assert!(!is_stale("2026-05-31T10:00:00+00:00", now, 24));
    }
    #[test]
    fn old_price_is_stale() {
        let now = Utc.with_ymd_and_hms(2026,5,31,12,0,0).unwrap();
        assert!(is_stale("2026-05-29", now, 24));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test pricing::service`
Expected: FAIL — `is_stale` not found (compile) → then PASS after Step 1 compiles. (Write test + fn together; run to confirm PASS.)

- [ ] **Step 3: Add the refresh routine**

```rust
use crate::db::Db;
use crate::pricing::{coingecko::CoinGecko, fx::FxClient, PriceProvider};
use crate::repo::{instruments, prices};

/// Refresh latest prices for all instruments whose price_source is "coingecko:<id>".
/// Also refreshes USD/IDR FX. Failures are logged, not fatal.
pub async fn refresh_all(db: &Db) -> anyhow::Result<()> {
    let today = Utc::now().format("%Y-%m-%d").to_string();
    let cg = CoinGecko::new();
    let fx = FxClient::new();

    match fx.usd_to_idr().await {
        Ok(rate) => { let _ = prices::upsert_fx(db, "USD", "IDR", rate, &today).await; }
        Err(e) => tracing::warn!("fx refresh failed: {e}"),
    }

    for ins in instruments::list(db).await? {
        if let Some(ext) = ins.price_source.strip_prefix("coingecko:") {
            match cg.latest(ext).await {
                Ok(q) => { let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "coingecko", &today).await; }
                Err(e) => tracing::warn!("price refresh failed for {}: {e}", ins.symbol),
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test pricing::service`
Expected: 2 tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/pricing/service.rs src/pricing/mod.rs && git commit -m "feat: pricing service refresh with stale detection"
```

---

### Task 14: Portfolio summary assembler (ties domain + repos)

**Files:**
- Create: `backend/src/service/portfolio.rs`
- Create: `backend/src/service/mod.rs`
- Modify: `backend/src/main.rs` (`mod service;`)

- [ ] **Step 1: Write `service/mod.rs`**

```rust
pub mod portfolio;
```

- [ ] **Step 2: Write the failing integration test in `service/portfolio.rs`**

```rust
use crate::db::Db;
use crate::domain::allocation::{compute_allocation, CategoryAllocation, CategoryInput};
use crate::domain::cost_basis::compute_cost_basis;
use crate::domain::valuation::{build_position, group_by_instrument, PriceContext, Position};
use crate::domain::xirr::{xirr, CashFlow};
use crate::domain::models::TxnType;
use crate::repo::{dec, categories, instruments, prices, transactions};
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct PortfolioSummary {
    pub net_worth_idr: Decimal,
    pub net_worth_usd: Decimal,
    pub total_unrealized_pnl_idr: Decimal,
    pub total_realized_pnl_idr: Decimal,
    pub xirr: Option<f64>,
    pub positions: Vec<Position>,
    pub allocation: Vec<CategoryAllocation>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    #[tokio::test]
    async fn summary_consolidates_one_position() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = categories::create(&db, &categories::NewCategory { name:"Crypto".into(), target_pct:"100".into(), tolerance_band_pct:Some("5".into()), sort_order:None, color:None }).await.unwrap();
        let acct = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"X".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument{ symbol:"BTC".into(), name:"BTC".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:Some(acc.id), price_source:"coingecko:bitcoin".into(), decimals:Some(8), note:None }).await.unwrap();
        transactions::create(&db, &transactions::NewTransaction{ account_id:acct.id, instrument_id:ins.id, txn_type:"buy".into(), executed_at:Utc::now(), quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None }).await.unwrap();
        prices::upsert_latest(&db, ins.id, dec("150").unwrap(), "USD", "test", "2099-01-01").await.unwrap();
        prices::upsert_fx(&db, "USD", "IDR", dec("16000").unwrap(), "2099-01-01").await.unwrap();

        let s = build_summary(&db).await.unwrap();
        assert_eq!(s.positions.len(), 1);
        assert_eq!(s.net_worth_usd, dec("150").unwrap());
        assert_eq!(s.net_worth_idr, dec("2400000").unwrap());
        assert_eq!(s.allocation[0].actual_pct, dec("100").unwrap());
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test service::portfolio`
Expected: FAIL — `build_summary` not found.

- [ ] **Step 4: Implement `build_summary`**

```rust
pub async fn build_summary(db: &Db) -> anyhow::Result<PortfolioSummary> {
    let usd_idr = prices::latest_fx(db, "USD", "IDR").await?.unwrap_or(Decimal::ONE);
    let all_txns = transactions::list_all(db).await?;
    let grouped = group_by_instrument(all_txns.clone());

    let mut positions = Vec::new();
    let mut net_idr = Decimal::ZERO;
    let mut net_usd = Decimal::ZERO;
    let mut unreal_idr = Decimal::ZERO;
    let mut real_idr = Decimal::ZERO;

    for (instrument_id, txns) in &grouped {
        let cb = compute_cost_basis(txns);
        let ins = instruments::get(db, *instrument_id).await?;
        let latest = prices::latest(db, *instrument_id).await?;
        let (price, stale) = match latest {
            Some(lp) => (lp.price, crate::pricing::service::is_stale(&lp.as_of, chrono::Utc::now(), 24)),
            None => (cb.avg_cost, true), // fall back to cost, flagged stale — never silently zero
        };
        // FX from instrument native currency to IDR/USD.
        let (to_idr, to_usd) = if ins.native_currency == "IDR" {
            (Decimal::ONE, if usd_idr.is_zero() { Decimal::ZERO } else { Decimal::ONE / usd_idr })
        } else { // treat non-IDR as USD-denominated in Phase 1
            (usd_idr, Decimal::ONE)
        };
        let ctx = PriceContext { instrument_id: *instrument_id, latest_price_native: price, price_stale: stale, fx_native_to_idr: to_idr, fx_native_to_usd: to_usd };
        let p = build_position(*instrument_id, &cb, &ctx);
        net_idr += p.market_value_idr;
        net_usd += p.market_value_usd;
        unreal_idr += p.unrealized_pnl * to_idr;
        real_idr += p.realized_pnl * to_idr;
        positions.push(p);
    }

    // Allocation by category (value in IDR).
    let cats = categories::list(db).await?;
    let mut cat_inputs = Vec::new();
    for c in &cats {
        let value_idr: Decimal = positions.iter().filter(|p| {
            // map position -> category via instrument lookup is costly; precomputed below
            false
        }).map(|p| p.market_value_idr).sum();
        let _ = value_idr; // replaced below
        cat_inputs.push(CategoryInput { category_id: c.id, name: c.name.clone(), target_pct: dec(&c.target_pct)?, tolerance_band_pct: c.tolerance_band_pct.as_deref().map(dec).transpose()?, value_idr: Decimal::ZERO });
    }
    // Fill category values: build instrument_id -> category_id map.
    let mut ins_cat = std::collections::HashMap::new();
    for ins in instruments::list(db).await? { ins_cat.insert(ins.id, ins.category_id); }
    for p in &positions {
        if let Some(Some(cid)) = ins_cat.get(&p.instrument_id) {
            if let Some(ci) = cat_inputs.iter_mut().find(|c| c.category_id == *cid) {
                ci.value_idr += p.market_value_idr;
            }
        }
    }
    let allocation = compute_allocation(&cat_inputs);

    // XIRR from cashflows: buys/deposits negative, sells/dividends positive, plus current net worth as terminal inflow.
    let mut flows: Vec<CashFlow> = Vec::new();
    for t in &all_txns {
        let amount_usd = (t.quantity * t.price_native + t.fee_native).to_string().parse::<f64>().unwrap_or(0.0);
        let signed = match t.txn_type {
            TxnType::Buy | TxnType::Deposit | TxnType::OpeningBalance => -amount_usd,
            TxnType::Sell | TxnType::Withdrawal | TxnType::Dividend | TxnType::Interest => amount_usd,
            TxnType::Fee => -amount_usd,
        };
        flows.push(CashFlow { date: t.executed_at.date_naive(), amount: signed });
    }
    let terminal = net_usd.to_string().parse::<f64>().unwrap_or(0.0);
    flows.push(CashFlow { date: chrono::Utc::now().date_naive(), amount: terminal });
    let xirr_val = xirr(&flows);

    Ok(PortfolioSummary {
        net_worth_idr: net_idr,
        net_worth_usd: net_usd,
        total_unrealized_pnl_idr: unreal_idr,
        total_realized_pnl_idr: real_idr,
        xirr: xirr_val,
        positions,
        allocation,
    })
}
```

(Note: the dead `value_idr` filter block above is illustrative scaffolding — delete those four lines; the real values are filled by the `ins_cat` loop that follows.)

- [ ] **Step 5: Clean up the dead block, run tests, commit**

Remove the `let value_idr ... false ... let _ = value_idr;` lines flagged in Step 4. Then:

Run: `cargo test service::portfolio`
Expected: PASS.

```bash
git add src/service/ src/main.rs && git commit -m "feat: portfolio summary assembler (net worth, pnl, xirr, allocation)"
```

---

### Task 15: Valuation snapshot + scheduler

**Files:**
- Create: `backend/src/repo/snapshots.rs`
- Create: `backend/src/scheduler.rs`
- Modify: `backend/src/repo/mod.rs`, `backend/src/main.rs`

- [ ] **Step 1: Write `repo/snapshots.rs` with a test**

```rust
use crate::db::Db;

pub async fn upsert(db: &Db, as_of: &str, total_idr: &str, total_usd: &str, breakdown_json: &str) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT INTO valuation_snapshot (as_of, total_idr, total_usd, breakdown_json) VALUES (?,?,?,?)
         ON CONFLICT(as_of) DO UPDATE SET total_idr=excluded.total_idr, total_usd=excluded.total_usd, breakdown_json=excluded.breakdown_json")
        .bind(as_of).bind(total_idr).bind(total_usd).bind(breakdown_json)
        .execute(db).await?;
    Ok(())
}

#[derive(serde::Serialize, sqlx::FromRow)]
pub struct SnapshotRow { pub as_of: String, pub total_idr: String, pub total_usd: String, pub breakdown_json: String }

pub async fn history(db: &Db) -> anyhow::Result<Vec<SnapshotRow>> {
    Ok(sqlx::query_as::<_, SnapshotRow>("SELECT * FROM valuation_snapshot ORDER BY as_of").fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn snapshot_upsert_and_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        upsert(&db, "2026-05-31", "1000", "0.06", "{}").await.unwrap();
        upsert(&db, "2026-05-31", "1100", "0.07", "{}").await.unwrap(); // same day overwrites
        assert_eq!(history(&db).await.unwrap().len(), 1);
        assert_eq!(history(&db).await.unwrap()[0].total_idr, "1100");
    }
}
```

Add `pub mod snapshots;` to `repo/mod.rs`.

- [ ] **Step 2: Run tests**

Run: `cargo test repo::snapshots`
Expected: PASS.

- [ ] **Step 3: Write `scheduler.rs`**

```rust
use crate::db::Db;
use std::time::Duration;

/// Background loop: refresh prices, then snapshot valuation, every `interval`.
pub fn spawn(db: Db, interval: Duration) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = crate::pricing::service::refresh_all(&db).await {
                tracing::warn!("price refresh error: {e}");
            }
            match crate::service::portfolio::build_summary(&db).await {
                Ok(s) => {
                    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                    let breakdown = serde_json::to_string(&s.allocation).unwrap_or_else(|_| "[]".into());
                    let _ = crate::repo::snapshots::upsert(&db, &today, &s.net_worth_idr.to_string(), &s.net_worth_usd.to_string(), &breakdown).await;
                }
                Err(e) => tracing::warn!("snapshot build error: {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    });
}
```

Add `mod scheduler;` to `main.rs`.

- [ ] **Step 4: Verify build**

Run: `cargo build`
Expected: compiles.

- [ ] **Step 5: Commit**

```bash
git add src/repo/snapshots.rs src/scheduler.rs src/repo/mod.rs src/main.rs && git commit -m "feat: valuation snapshots and background scheduler"
```

---

### Task 16: API routes — CRUD + summary + history

**Files:**
- Create: `backend/src/api/mod.rs`
- Create: `backend/src/api/crud.rs`
- Create: `backend/src/api/portfolio.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Write `api/mod.rs` router**

```rust
pub mod crud;
pub mod portfolio;

use crate::AppState;
use axum::{routing::{get, post, delete}, Router};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/accounts", get(crud::list_accounts).post(crud::create_account))
        .route("/accounts/:id", delete(crud::delete_account))
        .route("/categories", get(crud::list_categories).post(crud::create_category))
        .route("/categories/:id", delete(crud::delete_category))
        .route("/instruments", get(crud::list_instruments).post(crud::create_instrument))
        .route("/instruments/:id", delete(crud::delete_instrument))
        .route("/transactions", get(crud::list_transactions).post(crud::create_transaction))
        .route("/transactions/:id", delete(crud::delete_transaction))
        .route("/prices/manual", post(crud::manual_price))
        .route("/prices/refresh", post(portfolio::refresh))
        .route("/portfolio/summary", get(portfolio::summary))
        .route("/portfolio/history", get(portfolio::history))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
```

- [ ] **Step 2: Write `api/crud.rs`** (handlers delegate to repos)

```rust
use crate::error::AppError;
use crate::repo::{accounts, categories, instruments, prices, transactions, dec};
use crate::AppState;
use axum::{extract::{Path, State}, Json};

pub async fn list_accounts(State(s): State<AppState>) -> Result<Json<Vec<accounts::AccountRow>>, AppError> {
    Ok(Json(accounts::list(&s.db).await.map_err(AppError::Other)?))
}
pub async fn create_account(State(s): State<AppState>, Json(b): Json<accounts::NewAccount>) -> Result<Json<accounts::AccountRow>, AppError> {
    Ok(Json(accounts::create(&s.db, &b).await.map_err(AppError::Other)?))
}
pub async fn delete_account(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    accounts::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

pub async fn list_categories(State(s): State<AppState>) -> Result<Json<Vec<categories::CategoryRow>>, AppError> {
    Ok(Json(categories::list(&s.db).await.map_err(AppError::Other)?))
}
pub async fn create_category(State(s): State<AppState>, Json(b): Json<categories::NewCategory>) -> Result<Json<categories::CategoryRow>, AppError> {
    Ok(Json(categories::create(&s.db, &b).await.map_err(AppError::Other)?))
}
pub async fn delete_category(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    categories::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

pub async fn list_instruments(State(s): State<AppState>) -> Result<Json<Vec<instruments::InstrumentRow>>, AppError> {
    Ok(Json(instruments::list(&s.db).await.map_err(AppError::Other)?))
}
pub async fn create_instrument(State(s): State<AppState>, Json(b): Json<instruments::NewInstrument>) -> Result<Json<instruments::InstrumentRow>, AppError> {
    Ok(Json(instruments::create(&s.db, &b).await.map_err(AppError::Other)?))
}
pub async fn delete_instrument(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    instruments::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

pub async fn list_transactions(State(s): State<AppState>) -> Result<Json<Vec<crate::domain::models::Transaction>>, AppError> {
    let txns = transactions::list_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(txns))
}
pub async fn create_transaction(State(s): State<AppState>, Json(b): Json<transactions::NewTransaction>) -> Result<Json<crate::domain::models::Transaction>, AppError> {
    let t = transactions::create(&s.db, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(t))
}
pub async fn delete_transaction(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    transactions::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

#[derive(serde::Deserialize)]
pub struct ManualPrice { pub instrument_id: i64, pub price: String, pub currency: String, pub as_of: String }
pub async fn manual_price(State(s): State<AppState>, Json(b): Json<ManualPrice>) -> Result<Json<()>, AppError> {
    let price = dec(&b.price).map_err(|e| AppError::BadRequest(e.to_string()))?;
    prices::upsert_latest(&s.db, b.instrument_id, price, &b.currency, "manual", &b.as_of).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
```

To serialize `Transaction` (with Decimal), derive `Serialize` on it: in `domain/models.rs` add `Serialize` to the `Transaction` derive and `#[serde(with = "rust_decimal::serde::str")]` on each Decimal field, or simpler add `serde-with-str` feature (already enabled) and derive `Serialize`. Update the struct derive to `#[derive(Debug, Clone, Serialize)]` and annotate Decimal fields with `#[serde(with = "rust_decimal::serde::str")]`.

- [ ] **Step 3: Write `api/portfolio.rs`**

```rust
use crate::error::AppError;
use crate::service::portfolio::{build_summary, PortfolioSummary};
use crate::repo::snapshots;
use crate::AppState;
use axum::{extract::State, Json};

pub async fn summary(State(s): State<AppState>) -> Result<Json<PortfolioSummary>, AppError> {
    Ok(Json(build_summary(&s.db).await.map_err(AppError::Other)?))
}
pub async fn history(State(s): State<AppState>) -> Result<Json<Vec<snapshots::SnapshotRow>>, AppError> {
    Ok(Json(snapshots::history(&s.db).await.map_err(AppError::Other)?))
}
pub async fn refresh(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    crate::pricing::service::refresh_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
```

- [ ] **Step 4: Wire router + scheduler in `main.rs`**

Replace router construction:

```rust
mod api;
mod db;
mod domain;
mod error;
mod pricing;
mod repo;
mod scheduler;
mod service;

use db::Db;

#[derive(Clone)]
pub struct AppState { pub db: Db }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://portfolio.db".into());
    let db = db::connect(&url).await?;
    let state = AppState { db: db.clone() };
    scheduler::spawn(db, std::time::Duration::from_secs(3600));
    let app = api::router(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 5: Build, smoke-test, commit**

Run: `cargo build`
Expected: compiles.
Run smoke test:
```bash
DATABASE_URL="sqlite://smoke.db" cargo run & sleep 3
curl -s -XPOST localhost:8080/accounts -H 'content-type: application/json' -d '{"name":"Manual","account_type":"manual","native_currency":"IDR"}'
curl -s localhost:8080/portfolio/summary
kill %1; rm -f smoke.db
```
Expected: account JSON returned; summary returns zeroed totals + empty arrays.

```bash
git add src/api/ src/main.rs src/domain/models.rs && git commit -m "feat: REST API for CRUD, portfolio summary, and history"
```

---

### Task 17: Yahoo Finance provider (IDX stocks / US ETF) + wire into refresh

**Files:**
- Create: `backend/src/pricing/yahoo.rs`
- Modify: `backend/src/pricing/mod.rs` (`pub mod yahoo;`)
- Modify: `backend/src/pricing/service.rs` (refresh `yahoo:` instruments)

- [ ] **Step 1: Write `yahoo.rs` with a pure parser test**

```rust
use super::{PriceError, PriceProvider, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct Yahoo { base: String, client: reqwest::Client }

impl Yahoo {
    pub fn new() -> Self {
        Self { base: "https://query1.finance.yahoo.com/v8/finance/chart".into(), client: reqwest::Client::new() }
    }
}

/// Parse Yahoo chart JSON -> Quote using meta.regularMarketPrice + meta.currency.
pub fn parse_chart(body: &serde_json::Value) -> Result<Quote, PriceError> {
    let meta = body.pointer("/chart/result/0/meta")
        .ok_or_else(|| PriceError::NotFound("meta".into()))?;
    let price = meta.get("regularMarketPrice")
        .ok_or_else(|| PriceError::NotFound("regularMarketPrice".into()))?;
    let currency = meta.get("currency").and_then(|c| c.as_str()).unwrap_or("USD").to_string();
    let price = Decimal::from_str(price.to_string().trim_matches('"')).map_err(|e| PriceError::Parse(e.to_string()))?;
    Ok(Quote { price, currency })
}

#[async_trait::async_trait]
impl PriceProvider for Yahoo {
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError> {
        // ext_id is a Yahoo symbol, e.g. "BBCA.JK" (IDX) or "VOO" (US ETF).
        let url = format!("{}/{}", self.base, ext_id);
        let resp = self.client.get(&url).header("User-Agent", "Mozilla/5.0")
            .send().await.map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_chart(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    #[test]
    fn parses_chart_price_and_currency() {
        let body = serde_json::json!({ "chart": { "result": [ { "meta": { "regularMarketPrice": 9500, "currency": "IDR" } } ] } });
        let q = parse_chart(&body).unwrap();
        assert_eq!(q.price, dec!(9500));
        assert_eq!(q.currency, "IDR");
    }
    #[test]
    fn missing_meta_is_not_found() {
        let body = serde_json::json!({ "chart": { "result": [] } });
        assert!(matches!(parse_chart(&body), Err(PriceError::NotFound(_))));
    }
}
```

- [ ] **Step 2: Run to verify the parser tests**

Run: `cargo test pricing::yahoo`
Expected: 2 tests PASS (no network).

- [ ] **Step 3: Wire `yahoo:` instruments into `refresh_all`**

In `service.rs`, inside the `for ins in instruments::list(db).await?` loop, after the `coingecko:` branch add:

```rust
        if let Some(ext) = ins.price_source.strip_prefix("yahoo:") {
            match crate::pricing::yahoo::Yahoo::new().latest(ext).await {
                Ok(q) => { let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "yahoo", &today).await; }
                Err(e) => tracing::warn!("yahoo price refresh failed for {}: {e}", ins.symbol),
            }
        }
```

Add `pub mod yahoo;` to `pricing/mod.rs`.

- [ ] **Step 4: Run tests + build**

Run: `cargo test pricing:: && cargo build`
Expected: PASS + compiles.

- [ ] **Step 5: Commit**

```bash
git add src/pricing/yahoo.rs src/pricing/mod.rs src/pricing/service.rs && git commit -m "feat: yahoo finance provider for IDX stocks and US ETF"
```

---

## Self-Review

**Spec coverage check (spec §3 in-scope → task):**
- Accounts/Instruments/Categories manual mgmt → Tasks 8, 16 ✅
- Transaction ledger (all 8 types) → Tasks 3, 9 ✅
- Average-cost engine + realized/unrealized → Tasks 4, 6 ✅
- Dual-currency valuation → Tasks 6, 14 ✅
- Pricing — CoinGecko (crypto) → Tasks 10–13; Yahoo (IDX stocks/US ETF) → Task 17; FX → Tasks 10,12,13; manual NAV → Task 16 (`/prices/manual`) ✅
- Net worth + performance (ROI inputs, XIRR) → Tasks 5, 14 ✅ (note: simple ROI is computable in the frontend from `net_worth + realized + income - net_invested`; backend exposes the components and XIRR)
- Allocation planner (target/band/drift/rebalance) → Tasks 7, 14 ✅
- History (daily snapshot) → Task 15, 16 ✅
- Error handling no-unwrap, stale not zero → Tasks 1, 14 (fallback flagged stale) ✅

**Known follow-ups (acceptable for 1A):**
- Reksadana NAV uses `/prices/manual`; no auto provider (per spec — manual in Phase 1).
- ROI scalar not returned by API; frontend derives it. If preferred, add to `PortfolioSummary` in Task 14.

**Placeholder scan:** Task 14 contains an intentional dead-code block that Step 5 explicitly removes — called out, not a hidden placeholder. No other TBDs.

**Type consistency:** `compute_cost_basis`, `build_position`, `compute_allocation`, `xirr`, `CashFlow`, `PriceContext`, `CategoryInput`, repo `NewX`/`XRow` names are used consistently across Tasks 4–17.

---

## Execution Handoff

Plan complete — 17 tasks, all spec §3 requirements mapped to a task.
