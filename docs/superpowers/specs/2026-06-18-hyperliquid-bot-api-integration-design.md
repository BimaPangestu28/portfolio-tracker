# Hyperliquid ↔ Portfolio-Tracker Integration (Bot-API Model)

**Date:** 2026-06-18
**Status:** Design approved, pending spec review
**Supersedes:** `2026-06-18-hyperliquid-equity-integration-design.md` (on-chain pull model)
**Scope:** Changes span **two repos** — `agent-hyperliquid` gains a read-only HTTP
API; `portfolio-tracker` consumes that API instead of reading Hyperliquid
on-chain.

## Overview

`agent-hyperliquid` is a Telegram bot (Rust, `teloxide` + `rusqlite` +
`hyperliquid_rust_sdk`) that executes leveraged perpetual trades on Hyperliquid
and keeps a local trade journal. We want its activity to appear inside
`portfolio-tracker`'s analytics, monitoring, and reporting.

The earlier design had portfolio-tracker observe the account on-chain by wallet
address. This design replaces that: **the bot exposes its own balance, positions,
trades, and fund flows over a small authenticated HTTP API, and
portfolio-tracker pulls from that API.** The bot already owns the SDK session and
the journal, so it is the natural source of truth for its own positions — it can
serve enriched data (round-trip trades with realized PnL + strategy metadata)
that an on-chain reader could not assemble cleanly.

Two layers of integration result:

1. **Account equity → existing analytics.** The bot's `/balance` feeds the
   synthetic 1-unit `HL-EQUITY` instrument, so Hyperliquid equity flows into net
   worth, the global TWR curve, daily movers, milestone alerts, the morning
   briefing, and the stale-price liveness check — unchanged from the prior design
   except for the data source.
2. **Perp detail → a dedicated Hyperliquid section.** The bot's `/positions` and
   `/trades` populate new perp tables that drive a Hyperliquid-specific UI
   (open positions with unrealized PnL; closed round-trip trades with realized
   PnL, fees, and strategy metadata; aggregate stats such as win rate).

## Goals

- Bot exposes read-only `/balance`, `/positions`, `/trades`, `/flows` behind a
  bearer token. No bot trading logic changes; the API only reads existing state.
- Portfolio-tracker stops reading Hyperliquid on-chain. All Hyperliquid data
  arrives via the bot API, pulled on the existing scheduler.
- Account equity continues to contribute to net worth and the global TWR curve
  via the synthetic `HL-EQUITY` instrument (data source swapped to `/balance`).
- A dedicated Hyperliquid perp section: open positions, closed trades with PnL
  and strategy metadata, and aggregate stats.
- Deposits/withdrawals recorded as external flows so TWR is not distorted by
  fund transfers — sourced from the bot's `/flows`.

## Non-Goals

- Modeling perp PnL inside the existing spot `txn` / average-cost machinery. Perp
  positions and trades live in their own tables, separate from `txn`.
- Real-time/intraday updates. Cadence matches the existing pricing refresh loop
  (pull, not push/webhook).
- Re-deriving positions inside portfolio-tracker from raw fills. The bot
  aggregates; portfolio-tracker stores and displays.
- Any change to the bot's trading behavior. The API is read-only.

## Architecture

```
agent-hyperliquid (Rust, + axum HTTP server alongside teloxide loop)
  GET /balance    -> { equity_usd, as_of }
  GET /positions  -> [{ coin, direction, size, entry_px, mark_px,
                        unrealized_pnl, leverage, notional }]
  GET /trades?since=<ts> -> [{ external_id, coin, direction, size,
                        entry_px, exit_px, realized_pnl, fee,
                        opened_at, closed_at, leverage, confidence,
                        timeframe, profile }]
  GET /flows?since=<ts>  -> [{ external_id, kind: deposit|withdrawal,
                        usdc, time }]
        |  (all routes require Authorization: Bearer <token>, read-only)
        v   pull on portfolio's existing scheduler
portfolio-tracker
  /balance   -> pricing provider arm  -> HL-EQUITY price -> net worth + TWR + movers + stale-alert
  /flows     -> Connector trait        -> ExternalTxn deposit/withdrawal -> service/sync dedup
  /positions
  /trades    -> dedicated perp sync    -> hl_position / hl_trade tables -> Hyperliquid UI section
```

### Why three ingestion paths, not one

Each datum goes through the extension point whose shape it already fits:

- **Equity is a price fact.** Routing `/balance` through the pricing provider
  (`pricing/service.rs::refresh_all` dispatch by `price_source` prefix) means net
  worth, TWR, movers, milestones, and the stale-price alert pick it up with no
  new wiring — exactly as in the prior design.
- **Flows are transactions.** The existing `Connector` trait returns
  `ExternalTxn { kind: deposit|withdrawal, ... }` and `service/sync.rs` dedups by
  `external_id`. `/flows` maps onto this directly.
- **Perp positions/trades do not fit `ExternalTxn`** (they carry entry/exit
  price, leverage, realized PnL, strategy metadata). They get their own tables
  and a dedicated sync routine rather than being forced into the connector or the
  spot `txn` model.

All three are read-only pulls on the same scheduler.

## Components and Changes

### A. `agent-hyperliquid` — read-only HTTP API

- Add `axum` (+ tower) and spawn an HTTP server task alongside the existing
  teloxide loop in `main.rs`. Bind address from `HTTP_BIND_ADDR`.
- Bearer-token middleware comparing `Authorization: Bearer <token>` against
  `PORTFOLIO_API_TOKEN` in constant time; reject with 401 otherwise.
- Handlers:
  - `GET /balance` — from the SDK `equity()`; returns `{ equity_usd, as_of }`.
  - `GET /positions` — from the SDK user-state; open positions with mark price
    and unrealized PnL.
  - `GET /trades?since=<ts>` — round-trip trades assembled by grouping
    `user_fills` per coin (using `closed_pnl`/`fee`), enriched with journal
    metadata joined via `entry_order_id`. **Implementation note:** the current
    `Fill` struct captures only `{ coin, closed_pnl, dir, time_ms, fee }`; serving
    entry/exit price and size requires capturing more fields from the SDK
    `user_fills` response. This is bot-side work flagged for the plan.
  - `GET /flows?since=<ts>` — USDC deposits/withdrawals from the non-funding
    ledger.
- The API only reads existing journal/SDK state; trading logic is untouched.

### B. `portfolio-tracker` — equity feed (layer 1)

- `pricing/hyperliquid.rs`: client calling the bot `/balance` with the bearer
  token; returns a `Quote { price: equity_usd, currency: "USD" }`.
- `pricing/service.rs`: the `hyperliquid:<...>` dispatch arm calls the bot API
  instead of the on-chain info API. Failures logged, non-fatal.
- Synthetic `HL-EQUITY` 1-unit instrument and `Hyperliquid` exchange account
  provisioned on startup (same idempotent setup as the prior design).
- Config: `HYPERLIQUID_API_URL`, `HYPERLIQUID_API_TOKEN`.

### C. `portfolio-tracker` — perp tables + sync (layer 2)

- New sqlx migration creating `hl_position` (open snapshot, upserted each sync,
  keyed by coin) and `hl_trade` (closed round-trips, deduped by `external_id`).
  **Migration number must be rechecked against `origin/main` before merge**
  (known collision risk; current local tip is `0024`, `0023` is skipped).
- `service/hyperliquid_sync.rs`: pulls `/positions` and `/trades`, upserts
  positions, inserts new trades (dedup by `external_id`). Wired into the
  scheduler tick.
- Read-side helpers (`service/hyperliquid.rs`): equity summary (reused by
  briefing/recap/alerts as before) plus position/trade/stat reads for the UI.

### D. `portfolio-tracker` — flows connector

- `connectors/hyperliquid.rs`: `Connector` impl pulling `/flows` → `ExternalTxn`
  deposits/withdrawals. Registered in `connectors/factory.rs` under kind
  `"hyperliquid"`. `service/sync.rs` dedups and writes the flow `txn`s that
  `service/performance.rs` reads as external cash flows.

### E. `portfolio-tracker` — monitoring, reporting, UI

- **Drawdown alert** (`assistant/proactive/alerts.rs`) and **briefing/recap
  lines** reuse the equity series — unchanged from the prior design.
- **Endpoint** `GET /portfolio/hyperliquid` returns the equity/TWR view plus
  current open positions and recent closed trades with aggregate stats.
- **Frontend**: a Hyperliquid dashboard card (equity + day delta + mini curve)
  and a dedicated section/page showing open positions (unrealized PnL), closed
  trades (realized PnL + metadata), and aggregate stats (win rate, total PnL).
  Reuse existing Recharts wrappers and the API client/Zod-schema pattern.

## Configuration

- Bot: `PORTFOLIO_API_TOKEN` (shared secret), `HTTP_BIND_ADDR`.
- Portfolio: `HYPERLIQUID_API_URL`, `HYPERLIQUID_API_TOKEN`, `HL_DRAWDOWN_PCT`
  (default `15.0`). `HYPERLIQUID_WALLET` no longer required for data; the
  synthetic instrument's `price_source` becomes `hyperliquid:<label>` and the
  real target is the API URL.

## Error Handling

- `/balance` pull failure: log a warning and skip (non-fatal), like other pricing
  providers; the stale-price alert then surfaces the outage.
- Perp sync failure: log and skip the tick; positions/trades are idempotent
  (upsert by coin / dedup by `external_id`), so retries are safe.
- Flows connector: `ConnectorError` variants cover HTTP/parse/config;
  `service/sync.rs` dedups, so retries are idempotent.
- Bot API auth failure / unreachable: portfolio logs and degrades; no Hyperliquid
  data that tick, other analytics unaffected.
- Alerts/briefing/recap: missing Hyperliquid data omits the line rather than
  failing the report.

## Testing

- **Bot:** unit-test fill→round-trip-trade grouping (entry/exit/PnL math);
  unit-test the bearer-token middleware (accept valid, 401 invalid/missing);
  handler tests returning serialized shapes from a mock SDK/journal.
- **Portfolio equity:** parse a sample `/balance` response into the correct
  `Decimal` (mock HTTP, per existing provider tests).
- **Portfolio perp sync:** parse sample `/positions` + `/trades` into rows;
  assert dedup by `external_id` is idempotent on re-pull; assert open-position
  upsert replaces stale snapshots.
- **Flows connector:** map a sample `/flows` response to the expected
  `ExternalTxn` deposit/withdrawal set (mirrors the EVM connector tests).
- **View builder + UI:** equity/TWR points correct; positions/trades render;
  briefing/recap line renders when present and is omitted when absent.

## Build Sequencing

Each phase is independently shippable:

1. **Bot API** — `/balance` + bearer auth (smallest useful slice).
2. **Equity feed** — portfolio pricing arm → net worth / TWR / movers (swaps the
   on-chain source).
3. **Bot `/positions` + `/trades`** and portfolio perp tables + sync.
4. **Hyperliquid UI section** — card + perp positions/trades view + endpoint.
5. **Monitoring + reporting** — drawdown alert, briefing/recap lines.
6. **Flows** — bot `/flows` + portfolio connector for TWR accuracy.

## Open Questions

None outstanding. Data model (equity-as-price for net worth + separate perp
tables), bot-aggregated rich API, pull-on-scheduler with bearer auth, and the
three ingestion paths are confirmed.
