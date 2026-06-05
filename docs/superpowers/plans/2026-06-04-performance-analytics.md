# Performance Analytics (TWR + Risk) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a time-weighted return (TWR) performance curve (IDR + USD) with risk metrics (total/annualized return, max drawdown, volatility) on a new "Performa" page.

**Architecture:** A pure `domain/performance.rs` computes the TWR series + metrics from a NAV series and external-cashflow series; `service/performance.rs` loads daily `valuation_snapshot` rows and `Deposit`/`Withdrawal` transactions, windows by period, and assembles the view; `GET /portfolio/performance?base=&period=` serves it; a React "Performa" page renders the curve + cards with IDR/USD and period toggles.

**Tech Stack:** Rust/axum, `rust_decimal`, `chrono`; React/TS, react-query, zod, recharts, MSW.

Spec: `docs/superpowers/specs/2026-06-03-performance-analytics-design.md`

**Conventions (verified in this codebase):** backend JSON is **snake_case** (no `serde(rename_all)`); money is read from snapshot/txn strings; handlers `take State(s): State<AppState>` and return `Result<Json<T>, AppError>`; existing tests use `crate::db::connect("sqlite::memory:")`. `AppState` has fields `db` and `wa`.

---

## File Structure

- Create `backend/src/domain/performance.rs` — pure TWR + risk math (+ unit tests).
- Create `backend/src/service/performance.rs` — load/window snapshots + flows, build the view (+ tests).
- Modify `backend/src/domain/mod.rs` / `backend/src/service/mod.rs` — declare the new modules (match how siblings are declared).
- Modify `backend/src/api/portfolio.rs` — add the `performance` handler.
- Modify `backend/src/api/mod.rs` — add the protected route.
- Modify `frontend/src/api/schemas.ts` — `PerformanceSchema`.
- Modify `frontend/src/api/hooks.ts` — `usePerformance`.
- Create `frontend/src/pages/PerformancePage.tsx` (+ test) — the page.
- Modify `frontend/src/App.tsx` — route.
- Modify `frontend/src/components/AppShell.tsx` — nav item.

---

## Task 1: Domain TWR + risk math

**Files:**
- Create: `backend/src/domain/performance.rs`
- Modify: `backend/src/domain/mod.rs`

- [ ] **Step 1: Declare the module**

In `backend/src/domain/mod.rs`, add (alphabetical, alongside the other `pub mod` lines):

```rust
pub mod performance;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/domain/performance.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn deposit_only_yields_zero_return() {
        // NAV doubled but only because of a 100 deposit -> TWR must be 0.
        let navs = vec![(d("2026-01-01"), 100.0), (d("2026-01-02"), 200.0)];
        let flows = vec![(d("2026-01-02"), 100.0)];
        let (points, m) = compute(&navs, &flows);
        assert!((points.last().unwrap().cum_return).abs() < 1e-9);
        assert!(m.total_return.abs() < 1e-9);
    }

    #[test]
    fn pure_gain_yields_that_return() {
        let navs = vec![(d("2026-01-01"), 100.0), (d("2026-01-02"), 110.0)];
        let (_p, m) = compute(&navs, &[]);
        assert!((m.total_return - 0.10).abs() < 1e-9);
    }

    #[test]
    fn withdrawal_is_not_a_loss() {
        // NAV fell 100 -> 90 only because 10 was withdrawn -> 0 return.
        let navs = vec![(d("2026-01-01"), 100.0), (d("2026-01-02"), 90.0)];
        let flows = vec![(d("2026-01-02"), -10.0)];
        let (_p, m) = compute(&navs, &flows);
        assert!(m.total_return.abs() < 1e-9);
    }

    #[test]
    fn max_drawdown_of_known_wealth_series() {
        // wealth peaks at 1.1 then drops to 0.88 -> dd = 0.88/1.1 - 1 = -0.2
        let wealth = vec![1.0, 1.1, 0.88, 0.924];
        assert!((max_drawdown(&wealth) - (-0.2)).abs() < 1e-9);
    }

    #[test]
    fn fewer_than_two_navs_is_empty() {
        let (points, m) = compute(&[(d("2026-01-01"), 100.0)], &[]);
        assert!(points.is_empty());
        assert_eq!(m.total_return, 0.0);
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend && cargo test domain::performance`
Expected: FAIL — `compute` / `max_drawdown` not found.

- [ ] **Step 4: Write the implementation**

Prepend to `backend/src/domain/performance.rs`:

```rust
//! Time-weighted return (TWR) curve + risk metrics.
//!
//! Pure functions over a NAV series and an external-cashflow series. TWR daily-
//! links interval returns that exclude external flows:
//!   r = (NAV_end - flow_in_interval) / NAV_start - 1
//! so deposits/withdrawals don't show up as gains/losses.

use chrono::NaiveDate;

#[derive(Debug, Clone)]
pub struct PerfPoint {
    pub date: NaiveDate,
    pub cum_return: f64,
    pub nav: f64,
}

#[derive(Debug, Clone)]
pub struct PerfMetrics {
    pub total_return: f64,
    pub annualized: f64,
    pub max_drawdown: f64,
    pub volatility: f64,
}

const EMPTY_METRICS: PerfMetrics = PerfMetrics {
    total_return: 0.0,
    annualized: 0.0,
    max_drawdown: 0.0,
    volatility: 0.0,
};

/// Largest peak-to-trough decline of a wealth index (<= 0).
pub fn max_drawdown(wealth: &[f64]) -> f64 {
    let mut peak = f64::MIN;
    let mut worst = 0.0_f64;
    for &w in wealth {
        if w > peak {
            peak = w;
        }
        if peak > 0.0 {
            let dd = w / peak - 1.0;
            if dd < worst {
                worst = dd;
            }
        }
    }
    worst
}

/// Sample standard deviation. Returns 0 for fewer than 2 points.
fn stdev(xs: &[f64]) -> f64 {
    if xs.len() < 2 {
        return 0.0;
    }
    let mean = xs.iter().sum::<f64>() / xs.len() as f64;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (xs.len() as f64 - 1.0);
    var.sqrt()
}

/// Sum of flows falling in the half-open interval `(prev, cur]`.
fn flow_in(flows: &[(NaiveDate, f64)], prev: NaiveDate, cur: NaiveDate) -> f64 {
    flows
        .iter()
        .filter(|(date, _)| *date > prev && *date <= cur)
        .map(|(_, amt)| *amt)
        .sum()
}

/// Build the cumulative-return series and metrics. `navs` must be sorted by date.
/// Returns empty points + zero metrics when there are < 2 usable snapshots.
pub fn compute(navs: &[(NaiveDate, f64)], flows: &[(NaiveDate, f64)]) -> (Vec<PerfPoint>, PerfMetrics) {
    // Start at the first snapshot with a positive NAV.
    let start = match navs.iter().position(|(_, v)| *v > 0.0) {
        Some(i) => i,
        None => return (Vec::new(), EMPTY_METRICS),
    };
    let series = &navs[start..];
    if series.len() < 2 {
        return (Vec::new(), EMPTY_METRICS);
    }

    let mut wealth = 1.0_f64;
    let mut wealth_series = vec![1.0_f64];
    let mut returns: Vec<f64> = Vec::new();
    let mut points = vec![PerfPoint {
        date: series[0].0,
        cum_return: 0.0,
        nav: series[0].1,
    }];

    for w in series.windows(2) {
        let (prev_date, v_prev) = w[0];
        let (cur_date, v_cur) = w[1];
        let f = flow_in(flows, prev_date, cur_date);
        let r = if v_prev > 0.0 { (v_cur - f) / v_prev - 1.0 } else { 0.0 };
        returns.push(r);
        wealth *= 1.0 + r;
        wealth_series.push(wealth);
        points.push(PerfPoint {
            date: cur_date,
            cum_return: wealth - 1.0,
            nav: v_cur,
        });
    }

    let total_return = wealth - 1.0;
    let span_days = (series.last().unwrap().0 - series[0].0).num_days().max(1) as f64;
    let annualized = if wealth > 0.0 {
        wealth.powf(365.0 / span_days) - 1.0
    } else {
        -1.0
    };
    let avg_interval = span_days / returns.len() as f64;
    let periods_per_year = if avg_interval > 0.0 { 365.0 / avg_interval } else { 0.0 };
    let volatility = stdev(&returns) * periods_per_year.sqrt();

    (
        points,
        PerfMetrics {
            total_return,
            annualized,
            max_drawdown: max_drawdown(&wealth_series),
            volatility,
        },
    )
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test domain::performance`
Expected: PASS (5 tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/domain/performance.rs backend/src/domain/mod.rs
git commit -m "feat(backend): TWR performance + risk domain math"
```

---

## Task 2: Performance service (load + window + assemble)

**Files:**
- Create: `backend/src/service/performance.rs`
- Modify: `backend/src/service/mod.rs`

- [ ] **Step 1: Declare the module**

In `backend/src/service/mod.rs`, add alongside the other `pub mod` lines:

```rust
pub mod performance;
```

- [ ] **Step 2: Write the failing test**

Create `backend/src/service/performance.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn deposit_does_not_create_return() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // Two snapshots: 1,000,000 -> 2,000,000 IDR, but caused by a deposit.
        crate::repo::snapshots::upsert(&db, "2026-01-01", "1000000", "65", "{}").await.unwrap();
        crate::repo::snapshots::upsert(&db, "2026-01-02", "2000000", "130", "{}").await.unwrap();
        // Need an account + instrument to satisfy FKs for the txn.
        let acc = crate::repo::accounts::create(&db, "Cash", "cash").await.unwrap();
        let inst = crate::repo::instruments::create(&db, &new_cash_instrument()).await.unwrap();
        crate::repo::transactions::create(&db, &deposit_txn(acc.id, inst.id, "2026-01-02", "1000000")).await.unwrap();

        let view = build_performance(&db, "idr", "all").await.unwrap();
        assert!(!view.insufficient_data);
        assert_eq!(view.base, "idr");
        assert!(view.points.last().unwrap().cum_return.abs() < 1e-9);
        assert!(view.metrics.total_return.abs() < 1e-9);
    }

    #[tokio::test]
    async fn insufficient_when_one_snapshot() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::repo::snapshots::upsert(&db, "2026-01-01", "1000000", "65", "{}").await.unwrap();
        let view = build_performance(&db, "idr", "all").await.unwrap();
        assert!(view.insufficient_data);
        assert!(view.points.is_empty());
    }
}
```

> **Before writing helpers:** open `backend/src/repo/accounts.rs`, `backend/src/repo/instruments.rs`, and `backend/src/repo/transactions.rs` and confirm the exact constructor signatures (`accounts::create`, `instruments::create` with its `NewInstrument`-style input, and `transactions::create` with `NewTransaction`). Write the two test helpers `new_cash_instrument()` and `deposit_txn(account_id, instrument_id, date, amount)` to match those real signatures — a cash instrument (`instrument_type: "cash"`, `native_currency: "IDR"`, `price_native` "1") and a `Deposit` txn with `quantity = amount`, `price_native = "1"`, `fx_to_idr = "1"`, `fx_to_usd = "0.000065"`, `executed_at = "<date>T00:00:00Z"`. Keep the helpers in the test module.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend && cargo test service::performance`
Expected: FAIL — `build_performance` not found.

- [ ] **Step 4: Write the implementation**

Prepend to `backend/src/service/performance.rs`:

```rust
//! Loads NAV snapshots + external cashflows, windows them by period, and builds
//! the TWR performance view for the requested base currency.

use crate::db::Db;
use crate::domain::models::TxnType;
use crate::domain::performance::{compute, PerfMetrics};
use chrono::{Datelike, NaiveDate, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

#[derive(Serialize)]
pub struct PerfPointOut {
    pub date: String,
    pub cum_return: f64,
    pub nav: f64,
}

#[derive(Serialize)]
pub struct PerformanceView {
    pub base: String,
    pub points: Vec<PerfPointOut>,
    pub metrics: PerfMetrics,
    pub insufficient_data: bool,
}

// PerfMetrics needs to serialize; it lives in the domain. Add `#[derive(Serialize)]`
// there too (see Step 5 note).

/// Resolve a period string to an inclusive start date. `all` => None (no floor).
fn period_start(period: &str, today: NaiveDate) -> Option<NaiveDate> {
    match period {
        "all" => None,
        "ytd" => NaiveDate::from_ymd_opt(today.year(), 1, 1),
        "1m" => today.checked_sub_months(chrono::Months::new(1)),
        "3m" => today.checked_sub_months(chrono::Months::new(3)),
        "6m" => today.checked_sub_months(chrono::Months::new(6)),
        "1y" => today.checked_sub_months(chrono::Months::new(12)),
        _ => today.checked_sub_months(chrono::Months::new(12)), // default 1y
    }
}

pub async fn build_performance(db: &Db, base: &str, period: &str) -> anyhow::Result<PerformanceView> {
    let usd = base == "usd";
    let today = Utc::now().date_naive();
    let floor = period_start(period, today);

    // NAV series from snapshots, parsed to (date, f64) in the chosen base.
    let snaps = crate::repo::snapshots::history(db).await?;
    let mut navs: Vec<(NaiveDate, f64)> = Vec::new();
    for s in &snaps {
        let date = NaiveDate::parse_from_str(&s.as_of, "%Y-%m-%d")?;
        if let Some(f) = floor {
            if date < f {
                continue;
            }
        }
        let raw = if usd { &s.total_usd } else { &s.total_idr };
        let v = Decimal::from_str(raw).unwrap_or_default().to_f64().unwrap_or(0.0);
        navs.push((date, v));
    }

    // External flows: Deposit (+) / Withdrawal (-), valued in the chosen base.
    let txns = crate::repo::transactions::list_all(db).await?;
    let mut flows: Vec<(NaiveDate, f64)> = Vec::new();
    for t in &txns {
        let sign = match t.txn_type {
            TxnType::Deposit => Decimal::ONE,
            TxnType::Withdrawal => Decimal::NEGATIVE_ONE,
            _ => continue,
        };
        let date = t.executed_at.date_naive();
        if let Some(f) = floor {
            if date < f {
                continue;
            }
        }
        let fx = if usd { t.fx_to_usd } else { t.fx_to_idr };
        let value = (t.quantity * t.price_native * fx * sign).to_f64().unwrap_or(0.0);
        flows.push((date, value));
    }

    let (points, metrics) = compute(&navs, &flows);
    let insufficient_data = points.is_empty();

    Ok(PerformanceView {
        base: base.to_string(),
        points: points
            .into_iter()
            .map(|p| PerfPointOut {
                date: p.date.format("%Y-%m-%d").to_string(),
                cum_return: p.cum_return,
                nav: p.nav,
            })
            .collect(),
        metrics,
        insufficient_data,
    })
}
```

- [ ] **Step 5: Make `PerfMetrics` serializable**

In `backend/src/domain/performance.rs`, change the `PerfMetrics` derive to include `Serialize`:

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct PerfMetrics {
```

(Leave `PerfPoint` as-is — the service maps it to `PerfPointOut`.)

- [ ] **Step 6: Run tests to verify they pass**

Run: `cd backend && cargo test service::performance`
Expected: PASS (2 tests).

- [ ] **Step 7: Commit**

```bash
git add backend/src/service/performance.rs backend/src/service/mod.rs backend/src/domain/performance.rs
git commit -m "feat(backend): performance service (load snapshots+flows, TWR view)"
```

---

## Task 3: API endpoint + route

**Files:**
- Modify: `backend/src/api/portfolio.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Add the handler**

In `backend/src/api/portfolio.rs`, add imports + handler:

```rust
use crate::service::performance::{build_performance, PerformanceView};
use axum::extract::Query;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct PerfQuery {
    pub base: Option<String>,
    pub period: Option<String>,
}

pub async fn performance(
    State(s): State<AppState>,
    Query(q): Query<PerfQuery>,
) -> Result<Json<PerformanceView>, AppError> {
    let base = q.base.as_deref().unwrap_or("idr");
    if base != "idr" && base != "usd" {
        return Err(AppError::BadRequest("base must be idr or usd".into()));
    }
    let period = q.period.as_deref().unwrap_or("1y");
    if !["1m", "3m", "6m", "ytd", "1y", "all"].contains(&period) {
        return Err(AppError::BadRequest("invalid period".into()));
    }
    Ok(Json(build_performance(&s.db, base, period).await.map_err(AppError::Other)?))
}
```

(Keep the existing `use axum::{extract::State, Json};` — merge the `extract::Query` into it if you prefer a single use statement.)

- [ ] **Step 2: Add the route (JWT-protected group)**

In `backend/src/api/mod.rs`, inside the `protected` router (next to the other `/portfolio/*` routes), add:

```rust
        .route("/portfolio/performance", get(portfolio::performance))
```

- [ ] **Step 3: Verify build + full backend suite**

Run: `cd backend && cargo build && cargo test`
Expected: compiles; all tests pass (existing + new performance tests).

- [ ] **Step 4: Manual smoke (optional, with a running DB)**

Not required for CI. Logic is covered by Task 2 service tests.

- [ ] **Step 5: Format, lint, commit**

```bash
cd backend && cargo fmt && cargo clippy --all-targets
cd .. && git add backend/src/api/portfolio.rs backend/src/api/mod.rs
git commit -m "feat(backend): GET /portfolio/performance endpoint"
```

---

## Task 4: Frontend schema + hook

**Files:**
- Modify: `frontend/src/api/schemas.ts`
- Modify: `frontend/src/api/hooks.ts`

- [ ] **Step 1: Add the schema**

Append to `frontend/src/api/schemas.ts` (`z` is already imported):

```ts
export const PerformanceSchema = z.object({
  base: z.string(),
  points: z.array(
    z.object({ date: z.string(), cum_return: z.number(), nav: z.number() }),
  ),
  metrics: z.object({
    total_return: z.number(),
    annualized: z.number(),
    max_drawdown: z.number(),
    volatility: z.number(),
  }),
  insufficient_data: z.boolean(),
});
export type Performance = z.infer<typeof PerformanceSchema>;
```

- [ ] **Step 2: Add the hook**

In `frontend/src/api/hooks.ts`, add the `PerformanceSchema` import to the existing `./schemas` import, then add the hook near `useHistory`:

```ts
export const usePerformance = (base: "idr" | "usd", period: string) =>
  useQuery({
    queryKey: ["performance", base, period],
    queryFn: () => api.get(`/portfolio/performance?base=${base}&period=${period}`, PerformanceSchema),
  });
```

- [ ] **Step 3: Typecheck**

Run: `cd frontend && npx tsc -b`
Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts
git commit -m "feat(frontend): performance schema + usePerformance hook"
```

---

## Task 5: Performance page + route + nav

**Files:**
- Create: `frontend/src/pages/PerformancePage.tsx`
- Create: `frontend/src/pages/PerformancePage.test.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Write the failing test**

Create `frontend/src/pages/PerformancePage.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { render, screen, waitFor } from "@testing-library/react";
import { server } from "../test/server";
import PerformancePage from "./PerformancePage";

function wrap() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter><PerformancePage /></MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders metric cards from the API", async () => {
  server.use(
    http.get("/api/portfolio/performance", () =>
      HttpResponse.json({
        base: "idr",
        points: [
          { date: "2026-01-01", cum_return: 0, nav: 1000000 },
          { date: "2026-02-01", cum_return: 0.1, nav: 1100000 },
        ],
        metrics: { total_return: 0.1, annualized: 0.12, max_drawdown: -0.05, volatility: 0.2 },
        insufficient_data: false,
      }),
    ),
  );
  wrap();
  await waitFor(() => expect(screen.getByText(/Total return/i)).toBeInTheDocument());
  expect(screen.getByText(/Max drawdown/i)).toBeInTheDocument();
});

test("shows empty state when insufficient data", async () => {
  server.use(
    http.get("/api/portfolio/performance", () =>
      HttpResponse.json({
        base: "idr", points: [],
        metrics: { total_return: 0, annualized: 0, max_drawdown: 0, volatility: 0 },
        insufficient_data: true,
      }),
    ),
  );
  wrap();
  await waitFor(() => expect(screen.getByText(/belum cukup data/i)).toBeInTheDocument());
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd frontend && npx vitest run src/pages/PerformancePage.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the page**

Create `frontend/src/pages/PerformancePage.tsx`:

```tsx
import { useState } from "react";
import { AreaChart, Area, XAxis, YAxis, Tooltip, ResponsiveContainer } from "recharts";
import { usePerformance } from "../api/hooks";
import { QueryState } from "../components/QueryState";

const PERIODS: { value: string; label: string }[] = [
  { value: "1m", label: "1B" }, { value: "3m", label: "3B" }, { value: "6m", label: "6B" },
  { value: "ytd", label: "YTD" }, { value: "1y", label: "1Th" }, { value: "all", label: "Semua" },
];

const pct = (x: number) => `${(x * 100).toFixed(2)}%`;

function MetricCard({ label, value, loss }: { label: string; value: string; loss?: boolean }) {
  return (
    <div className="card" style={{ padding: 16 }}>
      <div className="t-sm t-muted">{label}</div>
      <div className="t-h2" style={{ marginTop: 4, color: loss ? "var(--loss)" : undefined }}>{value}</div>
    </div>
  );
}

export default function PerformancePage() {
  const [base, setBase] = useState<"idr" | "usd">("idr");
  const [period, setPeriod] = useState("1y");
  const q = usePerformance(base, period);
  const data = q.data;

  return (
    <div className="col gap-4">
      <div className="flex items-center justify-between" style={{ flexWrap: "wrap", gap: 8 }}>
        <div>
          <h1 className="t-h1">Performa</h1>
          <div className="t-sm t-muted">Return tertimbang waktu (TWR)</div>
        </div>
        <div style={{ display: "flex", gap: 8 }}>
          <div className="seg">
            {(["idr", "usd"] as const).map((b) => (
              <button key={b} type="button" className={base === b ? "seg-on" : ""}
                onClick={() => setBase(b)} aria-label={`Basis ${b.toUpperCase()}`}>
                {b.toUpperCase()}
              </button>
            ))}
          </div>
          <div className="seg">
            {PERIODS.map((p) => (
              <button key={p.value} type="button" className={period === p.value ? "seg-on" : ""}
                onClick={() => setPeriod(p.value)} aria-label={`Periode ${p.label}`}>
                {p.label}
              </button>
            ))}
          </div>
        </div>
      </div>

      <QueryState isLoading={q.isLoading} error={q.error}>
        {data?.insufficient_data ? (
          <div className="card" style={{ padding: 40, textAlign: "center" }}>
            <p className="t-muted">Belum cukup data — snapshot harian terkumpul seiring waktu.</p>
          </div>
        ) : (
          <>
            <div style={{ display: "grid", gridTemplateColumns: "repeat(auto-fit, minmax(150px, 1fr))", gap: 12 }}>
              <MetricCard label="Total return" value={pct(data?.metrics.total_return ?? 0)} loss={(data?.metrics.total_return ?? 0) < 0} />
              <MetricCard label="Annualized" value={pct(data?.metrics.annualized ?? 0)} loss={(data?.metrics.annualized ?? 0) < 0} />
              <MetricCard label="Max drawdown" value={pct(data?.metrics.max_drawdown ?? 0)} loss />
              <MetricCard label="Volatility" value={pct(data?.metrics.volatility ?? 0)} />
            </div>
            <div className="card" style={{ padding: 16, height: 340 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={(data?.points ?? []).map((p) => ({ date: p.date, ret: p.cum_return * 100 }))}>
                  <XAxis dataKey="date" tick={{ fontSize: 11 }} minTickGap={32} />
                  <YAxis tickFormatter={(v) => `${v.toFixed(0)}%`} width={44} tick={{ fontSize: 11 }} />
                  <Tooltip formatter={(v: number) => `${v.toFixed(2)}%`} />
                  <Area type="monotone" dataKey="ret" stroke="hsl(var(--primary))" fill="hsl(var(--primary) / 0.15)" />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </>
        )}
      </QueryState>
    </div>
  );
}
```

> Check `frontend/src/components/QueryState.tsx` for its real prop names (used across pages). If they differ from `isLoading`/`error`, match them. The `.seg`/`.seg-on` classes are illustrative — reuse whatever segmented-control class the app already has (see `AppShell`'s `Segmented`) or add minimal styles in `index.css`; the test only asserts text, not styling.

- [ ] **Step 4: Add the route**

In `frontend/src/App.tsx`: add the import and a route inside the `AppShell` route block:

```tsx
import PerformancePage from "./pages/PerformancePage";
```
```tsx
        <Route path="performance" element={<PerformancePage />} />
```

- [ ] **Step 5: Add the nav item**

In `frontend/src/components/AppShell.tsx`: import an icon and add a nav entry. Add `LineChart` to the existing `lucide-react` import, then add to the `NAV`/nav array (after Portofolio):

```tsx
  { to: "/performance", label: "Performa", icon: LineChart },
```

- [ ] **Step 6: Run the page tests**

Run: `cd frontend && npx vitest run src/pages/PerformancePage.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 7: Commit**

```bash
git add frontend/src/pages/PerformancePage.tsx frontend/src/pages/PerformancePage.test.tsx frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(frontend): Performa page (TWR curve + risk cards)"
```

---

## Task 6: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Backend**

Run: `cd backend && cargo test && cargo clippy --all-targets`
Expected: tests pass; no new clippy errors.

- [ ] **Step 2: Frontend typecheck + tests + build**

Run: `cd frontend && npx tsc -b && npx vitest run && npm run build`
Expected: no type errors; all tests pass; build succeeds.

- [ ] **Step 3: Commit any fixups**

```bash
git add -A && git commit -m "chore: fixups for performance page" || echo "nothing to commit"
```

---

## Self-Review Notes

- **Spec coverage:** TWR series (Task 1), IDR+USD (service `base` param, Task 2), period windowing (Task 2 `period_start`), metrics total/annualized/maxDD/volatility (Task 1), endpoint (Task 3), schema+hook (Task 4), Performa page with toggles + empty state (Task 5). ✓
- **Cashflow definition:** external = `Deposit`/`Withdrawal` only, isolated in the `build_performance` loop (Task 2) — adjustable in one place per the spec. ✓
- **Casing:** response is snake_case (`cum_return`, `total_return`, `insufficient_data`); the frontend schema (Task 4) matches. ✓
- **Type consistency:** `compute` / `PerfPoint` / `PerfMetrics` (Task 1) reused verbatim by the service (Task 2); `PerformanceView` / `PerfPointOut` (Task 2) match the endpoint return (Task 3) and the zod schema (Task 4).
- **Known checks for the implementer:** (a) confirm `repo::accounts::create` / `instruments::create` / `transactions::create` signatures before writing Task 2 test helpers; (b) confirm `QueryState` prop names; (c) reuse the existing segmented-control styling rather than inventing `.seg`.
