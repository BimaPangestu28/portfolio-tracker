# Performance Analytics (TWR + Risk) — Design

Date: 2026-06-03

Sub-project **A** of the analytics effort (#6/#7). Build order: **A → B (benchmark) → C (rebalance exec) → D (dividend view)**. B (benchmark comparison) depends on the return curve built here.

## Problem

The app stores net-worth history (`valuation_snapshot`) and XIRR (money-weighted, single number), but has **no time-weighted return (TWR) curve, no return-% over time, and no risk metrics**. Users can't see portfolio performance independent of when they added money, and there's no return curve for a benchmark to be compared against later.

## Goals

- A **TWR return curve** over time, in **IDR and USD** (toggle).
- Summary/risk metrics: **total return, annualized return, max drawdown, volatility**.
- A dedicated **"Performa"** page.
- Methodology compatible with a later benchmark overlay (sub-project B).

## Non-goals

- Benchmark comparison (sub-project B).
- Sharpe ratio / risk-free rate (YAGNI).
- Per-asset / per-category return attribution (future).
- Intraday accuracy — we use the existing daily-cadence snapshots.

## Methodology (TWR)

Daily-link sub-period returns between consecutive snapshots, excluding external cashflows:

- For consecutive snapshots at `t-1`, `t` with end-of-period NAVs `V_{t-1}`, `V_t` and net external flow `F_t` occurring in the interval `(t-1, t]`:
  - `r_t = (V_t − F_t) / V_{t-1} − 1`
- Cumulative TWR through `T`: `C_T = ∏_{t ≤ T} (1 + r_t) − 1`. Plotted as `cumReturn = C_t` (a fraction; UI shows ×100 %).
- Computed **separately for IDR** (`total_idr`) and **USD** (`total_usd`).

**External cashflows** (confirmed: the user records Deposits/Withdrawals):

- `F_t` = signed sum within the interval of **`Deposit` (+)** and **`Withdrawal` (−)** transactions, valued in the base currency (`quantity × price_native × fx_to_idr` for IDR, `× fx_to_usd` for USD).
- `Buy` / `Sell` / `Dividend` / `Interest` / `Fee` are **internal** — part of the return, not flows.
- The flow-extraction lives in one function so it can be adjusted (e.g. to include Buy/Sell) if real data shows deposits aren't recorded consistently.

**Edge handling:**

- Start the series at the first snapshot with `V > 0`; skip any interval where `V_{t-1} ≤ 0`.
- Snapshots are not assumed strictly daily — link between *available* consecutive snapshots (interval-based).
- `< 2` usable snapshots → `insufficientData: true`, empty response.

**Metrics** (over the selected period; `r_t` are the interval returns, `W_t = ∏(1+r)` the wealth index):

- **Total return** = `C_last`.
- **Annualized** = `(1 + C_last)^(365 / spanDays) − 1`, `spanDays` = days between first and last snapshot (guard `spanDays ≥ 1`).
- **Max drawdown** = `min_t ( W_t / runningMax(W)_t − 1 )` (most negative peak-to-trough decline; ≤ 0).
- **Volatility** = stdev of the interval returns `r_t`, annualized by `√(periodsPerYear)`, `periodsPerYear ≈ 365 / avgIntervalDays`. (Documented approximation; cadence-aware.)

## Architecture

### Backend

- **`backend/src/domain/performance.rs`** (pure, unit-tested):
  - `struct PerfPoint { date: NaiveDate, cum_return: f64, nav: Decimal }`
  - `struct PerfMetrics { total_return: f64, annualized: f64, max_drawdown: f64, volatility: f64 }`
  - `fn compute(navs: &[(NaiveDate, Decimal)], flows: &[(NaiveDate, Decimal)]) -> (Vec<PerfPoint>, PerfMetrics)` — aligns flows to intervals, builds the series + metrics.
  - helpers: `max_drawdown(&[f64]) -> f64`, `annualized_volatility(returns: &[f64], span_days: i64) -> f64`.
- **`backend/src/service/performance.rs`**:
  - `async fn build_performance(db, base: Base, period: Period) -> PerformanceView`
  - Loads `valuation_snapshot` rows (`date`, total for `base`) within the period window; loads `Deposit`/`Withdrawal` txns within the window valued in `base`; calls `domain::performance::compute`; assembles the view.
- **`backend/src/repo`**: a query for snapshots (`date, total_idr, total_usd` ordered by date) and one for `Deposit`/`Withdrawal` txns (date + base value). Reuse the existing `valuation_snapshot` access used by the history endpoint where possible.
- **`backend/src/api/portfolio.rs`**: add a `performance` handler. Route `GET /portfolio/performance` in the **JWT-protected** group, query params `base` and `period`.
  - `Base { Idr, Usd }`; `Period { M1, M3, M6, Ytd, Y1, All }` → resolved to a start date (`All` = first snapshot). Invalid value → 400.
- **Response JSON:**
  ```json
  {
    "base": "idr",
    "points": [{ "date": "2026-01-01", "cumReturn": 0.0, "nav": "100000000" }],
    "metrics": { "totalReturn": 0.12, "annualized": 0.13, "maxDrawdown": -0.08, "volatility": 0.18 },
    "insufficientData": false
  }
  ```

### Frontend — new "Performa" page

- **`frontend/src/pages/PerformancePage.tsx`** + route in `App.tsx` + nav item "Performa" in `AppShell.tsx`.
- **`frontend/src/api/schemas.ts`**: `PerformanceSchema` (matches the response above).
- **`frontend/src/api/hooks.ts`**: `usePerformance(base, period)` → `GET /portfolio/performance?base=&period=`.
- **Chart:** recharts area/line of `cumReturn %` over `date`, styled to match existing charts (e.g. `HistoryChart` / `StackedAreaChart`). **IDR/USD toggle** and **period selector** (1B / 3B / 6B / YTD / 1Th / Semua) each refetch.
- **Risk cards (4):** Total return, Annualized, Max drawdown, Volatility — formatted as %, gain/loss colored.
- **Empty state** when `insufficientData`: "Belum cukup data — snapshot harian terkumpul seiring waktu."

### Data flow

1. Page loads → `usePerformance('idr','1y')` → `GET /portfolio/performance`.
2. Backend windows snapshots + flows, computes TWR series + metrics for the base, returns.
3. Chart + cards render; toggling base or period refetches.

## Error handling

- `insufficientData: true` when `< 2` usable snapshots → page shows the empty state, no chart.
- Invalid `base`/`period` → `400`.
- Endpoint is under `/portfolio` → JWT-protected (auth already enforced).

## Testing (TDD)

- **domain** (pure, deterministic): a deposit-only history → **0 % TWR** (proves cashflow exclusion); a pure +10 % asset move with no flows → **10 % TWR**; a withdrawal interval; `max_drawdown` and `annualized_volatility` on known series; `< 2` snapshots → empty.
- **service**: fixture DB (snapshots + a deposit) → expected `points` + `metrics`.
- **frontend**: `usePerformance` fetch via MSW; page renders chart + 4 cards; base/period toggle refetches; `insufficientData` empty state.
