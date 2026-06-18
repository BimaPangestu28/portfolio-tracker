# Hyperliquid → Portfolio-Tracker Integration (Account-Equity Model)

**Date:** 2026-06-18
**Status:** Design approved, pending spec review
**Scope:** Changes live entirely in `portfolio-tracker`. `agent-hyperliquid` is untouched.

## Overview

`agent-hyperliquid` is a Telegram bot that executes leveraged perpetual trades on
Hyperliquid. We want its trading activity to show up inside `portfolio-tracker`'s
existing analytics, monitoring, and reporting — using the **same analytics
approach** rather than building a second, divergent stack.

Instead of forcing leveraged perps (longs *and* shorts, funding, liquidation)
into portfolio-tracker's spot, long-only, transaction-journal model, we observe
the Hyperliquid account at the **account-equity** level: Hyperliquid becomes one
account whose value equals its USDC account equity, pulled read-only from the
Hyperliquid info API by wallet address. Once equity flows in as a daily-tracked
value, the existing net-worth, time-weighted-return (TWR), movers, milestone
alerts, and morning briefing pick it up automatically. On top of that automatic
inclusion we add three Hyperliquid-specific touches: a drawdown alert, a
briefing/recap line, and a frontend section.

## Goals

- Hyperliquid account equity appears in portfolio-tracker net worth, performance
  (TWR) curve, and daily movers — through the existing snapshot pipeline.
- Read-only pull on portfolio-tracker's existing scheduler. No private key, no
  cross-app authentication, no changes to `agent-hyperliquid`.
- Hyperliquid-specific drawdown alert reusing the proactive-alert + dedup-log
  machinery.
- Hyperliquid line in the morning briefing and weekly/monthly recap.
- Hyperliquid section in the React frontend (equity card + per-account TWR view).
- Deposits/withdrawals to the Hyperliquid account are recorded as external flows
  so TWR is not distorted by fund transfers.

## Non-Goals

- Per-position or per-trade representation of perps inside portfolio-tracker
  (rejected: conflicts with the spot cost-basis model). Per-trade detail stays in
  `agent-hyperliquid`'s own SQLite journal.
- Modeling leverage, funding payments, mark price, or liquidation risk inside
  portfolio-tracker.
- Any change to `agent-hyperliquid`. The bot continues trading; portfolio-tracker
  independently observes the same on-chain account.
- Intraday/real-time equity tracking. Cadence matches the existing pricing
  refresh loop.

## Architecture

### Reframing

Because the integration uses a **read-only pull** keyed on the Hyperliquid
**wallet address**, portfolio-tracker reads the *same* account the bot trades —
directly from Hyperliquid. The two apps stay fully decoupled: the bot executes,
portfolio-tracker observes. This is why no bot change is required.

### Data representation in the spot-only model

portfolio-tracker has no positions table; positions are derived by summing `txn`
rows with average-cost FIFO (`domain/cost_basis.rs`). We represent the
Hyperliquid account as a single synthetic, fixed-quantity instrument priced at
the account's equity:

- **Account:** `name = "Hyperliquid"`, `account_type = "exchange"`,
  `native_currency = "USD"`. Created once.
- **Instrument:** symbol `HL-EQUITY`, `native_currency = "USD"`,
  `instrument_type = "other"`, `price_source = "hyperliquid:<wallet>"`,
  `decimals = 2`. Held quantity fixed at **1 unit** (seeded via a single
  `opening_balance` txn of quantity 1).
- **Price of the 1 unit = current account equity in USD**, refreshed by the
  pricing loop.

Consequences (all verified against the code):

- `market_value = quantity(1) × price(equity) = equity` → contributes to net
  worth (`service/insights.rs`) and the valuation snapshot.
- `service/movers.rs` computes `(price_latest − price_prev) × quantity × fx`,
  which with `quantity = 1` equals the equity change in IDR → correct daily
  mover.
- `service/performance.rs` builds TWR from the snapshot NAV series minus external
  flows → correct once deposits/withdrawals are recorded as flows (Phase 5).
- The synthetic line's per-instrument *cost-basis* PnL fields are not meaningful
  (cost basis of a 1-unit synthetic is arbitrary). They are suppressed in any
  Hyperliquid-specific display; the meaningful metric is the equity/TWR curve and
  net-worth contribution.

### Two read-only pulls, each on the right extension point

1. **Equity value → pricing provider** (primary, Phase 1).
   `pricing/service.rs::refresh_all()` already dispatches by `price_source`
   prefix (`coingecko:`, `yahoo:`, `bibit:`, `gold:idr_gram`). We add a
   `hyperliquid:` arm: parse the wallet from the suffix, call the Hyperliquid
   info API for account equity, and `prices::upsert_latest(db, ins.id, equity,
   "USD", "hyperliquid", today)`. Failures are logged, non-fatal — matching the
   existing providers. Bonus: because the source is not `"manual"`,
   `stale_price_alerts` (in `assistant/proactive/alerts.rs`) will flag the
   Hyperliquid line if the pull stops working — free liveness monitoring.

2. **Deposits/withdrawals → connector** (Phase 5).
   Implement the `Connector` trait (`connectors/mod.rs`,
   `fetch_new(cursor) -> SyncBatch`) in `connectors/hyperliquid.rs`, modeled on
   `connectors/evm.rs`. It pulls the account's USDC transfer ledger and emits
   `ExternalTxn { kind: "deposit" | "withdrawal", symbol: "USDC",
   currency: "USD", external_id: <ledger event id>, ... }`. Registered in
   `connectors/factory.rs` under kind `"hyperliquid"`. The existing
   `service/sync.rs` dedups by `external_id` and writes the flow txns that
   `service/performance.rs` reads as external cash flows.

   *Why the equity value goes through pricing, not the connector:* the `Connector`
   trait is transaction-oriented (it returns `ExternalTxn`s). "Account value =
   price of 1 unit" is a price fact, so it belongs in the pricing provider. Both
   mechanisms are read-only pulls on the same scheduler — we use the right one for
   each data type.

## Components and Changes

### Phase 1 — Core: equity in net worth / TWR / movers
- `backend/src/pricing/hyperliquid.rs` (new): Hyperliquid info-API client
  fetching account equity for a wallet on a given network. Returns a `Decimal`
  USD value. Pure HTTP + parse; mirrors `pricing/coingecko.rs` shape.
- `backend/src/pricing/service.rs`: add the `strip_prefix("hyperliquid:")` arm in
  `refresh_all()`.
- `backend/src/pricing/mod.rs`: register the new module.
- One-time data setup: create the Hyperliquid account, the `HL-EQUITY`
  instrument, and the quantity-1 `opening_balance` txn. Document as a setup step
  (API calls or a small seed routine); no schema migration required — existing
  `account`, `instrument`, `txn` tables suffice.

### Phase 2 — Monitoring: Hyperliquid drawdown alert
- `backend/src/assistant/proactive/alerts.rs`: add pure helper
  `hyperliquid_drawdown_alert(quotes, threshold_pct, today_wib) -> Option<Alert>`
  comparing latest equity against the recent peak from the `HL-EQUITY` price-quote
  history; emits an `Alert { dedup_key: "hl-drawdown:<bucket>", message }` when the
  drawdown exceeds the threshold. Wire it into `evaluate()` as an
  independently-degrading section (same pattern as the other alert sources).
- Config: `HL_DRAWDOWN_PCT` added to `ProactiveConfig`.

### Phase 3 — Reporting: briefing & recap line
- `backend/src/assistant/proactive/briefing.rs`: gather Hyperliquid equity + day
  delta and render `"Hyperliquid: $X (±Y% hari ini)"`. Degrades gracefully if the
  source is unavailable (matches existing signal handling).
- `backend/src/assistant/proactive/recap.rs` and `monthly_recap.rs`: add the
  Hyperliquid equity change for the period.

### Phase 4 — Analytics UI: frontend Hyperliquid section
- React: a Hyperliquid card on the dashboard (current equity, day delta, mini
  equity curve) and an account filter on `PerformancePage` to view the
  Hyperliquid TWR curve alone. Reuse existing Recharts wrappers and the
  `/portfolio/performance` + `/portfolio/history` endpoints (filtering by the
  Hyperliquid account).

### Phase 5 — TWR accuracy: deposit/withdrawal flow connector
- `backend/src/connectors/hyperliquid.rs` (new): `Connector` impl pulling USDC
  transfer ledger events → `ExternalTxn` deposits/withdrawals.
- `backend/src/connectors/factory.rs`: register kind `"hyperliquid"`.
- Connector config_json: `{ "wallet": "0x...", "network": "mainnet|testnet" }`.

## Configuration

- `HYPERLIQUID_WALLET` — account wallet address (also encoded in the instrument's
  `price_source` suffix and the connector `config_json`).
- `HYPERLIQUID_NETWORK` — `mainnet` | `testnet`.
- `HL_DRAWDOWN_PCT` — drawdown alert threshold (in `ProactiveConfig`).

## Error Handling

- Pricing pull failure: log a warning and skip (non-fatal), exactly like the
  existing providers in `refresh_all()`. The stale-price alert then surfaces the
  outage.
- Alert evaluation: the Hyperliquid drawdown section degrades independently in
  `evaluate()` — a failure must not silence other alerts.
- Briefing/recap: missing Hyperliquid data omits the line rather than failing the
  report.
- Connector (Phase 5): `ConnectorError` variants already cover HTTP/parse/config;
  `service/sync.rs` dedups, so retries are idempotent.

## Testing

- `pricing/hyperliquid.rs`: unit test parsing a sample info-API response into the
  correct equity `Decimal` (mock the HTTP layer, per existing provider tests).
- `alerts.rs`: unit tests for `hyperliquid_drawdown_alert` — below threshold is
  silent, at/above fires once with the right `dedup_key`, peak/trough math is
  correct (mirrors the existing `mover_alerts` / `milestones_crossed` tests).
- `connectors/hyperliquid.rs` (Phase 5): map a sample ledger response to the
  expected `ExternalTxn` deposit/withdrawal set (mirrors the EVM connector tests).
- Briefing/recap: assert the Hyperliquid line renders when data is present and is
  omitted when absent.

## Build Sequencing

Each phase is independently shippable and adds value on its own:

1. Core equity feed (net worth / TWR / movers).
2. Drawdown alert (monitoring).
3. Briefing/recap line (reporting).
4. Frontend Hyperliquid section (analytics UI).
5. Deposit/withdrawal flow connector (TWR accuracy).

## Open Questions

None outstanding. The account-equity model, pull mechanism, and per-phase scope
are confirmed.
