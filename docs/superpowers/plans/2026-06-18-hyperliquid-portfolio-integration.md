# Hyperliquid Portfolio Integration (Bot-API Consumer) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Depends on:** the bot-side API plan (`agent-hyperliquid/docs/plans/2026-06-18-hyperliquid-bot-api.md`). Phases 2–3 here consume `/positions`, `/trades`, `/flows`; Phase 1 consumes `/balance`. The endpoints can be stubbed/mocked for tests, so this plan is independently testable, but live data needs the bot API running.

**Goal:** Consume the `agent-hyperliquid` read-only API so Hyperliquid equity flows into portfolio-tracker's net worth / TWR / movers, and add a dedicated perp section (open positions, closed trades with PnL + strategy metadata) plus drawdown alerts and briefing/recap lines.

**Architecture:** Account equity is the price of a synthetic 1-unit `HL-EQUITY` instrument, refreshed by the pricing loop from the bot's `/balance` (replacing the on-chain pull). Perp positions/trades are pulled by a dedicated sync routine into new `hl_position`/`hl_trade` tables (they don't fit the spot `txn` model). USDC deposits/withdrawals come via a `Connector` over `/flows` so TWR excludes fund transfers. All three are read-only pulls on the existing scheduler.

**Tech Stack:** Rust (Axum, sqlx/SQLite, reqwest, rust_decimal, tokio, async-trait, chrono, serde, tracing); React + TypeScript (Vite, TanStack Query, Zod, Recharts, Vitest + Testing Library).

## Global Constraints

- Backend money values are `rust_decimal::Decimal`, stored in SQLite as TEXT strings.
- Provider/connector/sync errors are logged (`tracing::warn!`) and non-fatal — never abort `refresh_all` or a scheduler tick.
- Synthetic instrument symbol is exactly `HL-EQUITY`; account name is exactly `Hyperliquid`; `price_source` is exactly `hyperliquid:bot` (the real target is the API URL, not a wallet).
- Bot API: base from `HYPERLIQUID_API_URL` (e.g. `http://127.0.0.1:8088`); every request sends `Authorization: Bearer <HYPERLIQUID_API_TOKEN>`. `/balance` → `{ equity_usd: number, as_of_ms: number }`. `/positions`, `/trades?since=<ms>`, `/flows?since=<ms>` per the bot plan's shapes.
- Env vars: `HYPERLIQUID_API_URL`, `HYPERLIQUID_API_TOKEN` (both enable the integration), `HL_DRAWDOWN_PCT` (default `15.0`).
- Backend tests run from `backend/` with `cargo test`; frontend from `frontend/` with `npm test` / `npm run build`.
- Pure parse functions are unit-tested without network; DB code is tested with `crate::db::connect("sqlite::memory:")`. Do not run `cargo fmt`.
- **Migration numbering:** the new migration must take the next free number checked against `origin/main` (local tip is `0024`; `0023` is currently skipped — confirm before naming to avoid a collision).

---

## Phase 1 — Equity feed (net worth / TWR / movers)

### Task 1: Hyperliquid balance API client (pricing provider)

**Files:**
- Create: `backend/src/pricing/hyperliquid.rs`
- Modify: `backend/src/pricing/mod.rs` (add `pub mod hyperliquid;`)

**Interfaces:**
- Consumes: `crate::pricing::{PriceError, Quote}` (`Quote { price: Decimal, currency: String }`).
- Produces: `pricing::hyperliquid::BotClient::from_env() -> Option<Self>`; `async fn account_equity(&self) -> Result<Quote, PriceError>`; `fn parse_balance(body: &serde_json::Value) -> Result<Quote, PriceError>`.

- [ ] **Step 1: Write the failing test** — append to `backend/src/pricing/hyperliquid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_equity_from_balance_response() {
        let body = serde_json::json!({ "equity_usd": 1234.56, "as_of_ms": 1700000000000_i64 });
        let q = parse_balance(&body).unwrap();
        assert_eq!(q.price, dec!(1234.56));
        assert_eq!(q.currency, "USD");
    }

    #[test]
    fn missing_equity_is_parse_error() {
        let body = serde_json::json!({ "as_of_ms": 1 });
        assert!(matches!(parse_balance(&body).unwrap_err(), PriceError::Parse(_)));
    }
}
```

- [ ] **Step 2: Write the implementation** — prepend to the same file:

```rust
use crate::pricing::{PriceError, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Read-only client for the agent-hyperliquid bot API.
pub struct BotClient {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl BotClient {
    /// Built only when both env vars are set; `None` disables the integration.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("HYPERLIQUID_API_URL").ok().filter(|s| !s.is_empty())?;
        let token = std::env::var("HYPERLIQUID_API_TOKEN").ok().filter(|s| !s.is_empty())?;
        Some(Self { base_url, token, client: reqwest::Client::new() })
    }

    /// Total account equity (USD) from `GET /balance`.
    pub async fn account_equity(&self) -> Result<Quote, PriceError> {
        let url = format!("{}/balance", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            return Err(PriceError::Http(format!("hyperliquid bot /balance status {status}")));
        }
        let json: serde_json::Value =
            resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_balance(&json)
    }
}

/// Pull `equity_usd` out of a `/balance` response.
pub fn parse_balance(body: &serde_json::Value) -> Result<Quote, PriceError> {
    let raw = body
        .get("equity_usd")
        .ok_or_else(|| PriceError::Parse("missing equity_usd".into()))?;
    // Accept either a JSON number or a numeric string.
    let price = match raw {
        serde_json::Value::Number(n) => Decimal::from_str(&n.to_string()),
        serde_json::Value::String(s) => Decimal::from_str(s),
        _ => return Err(PriceError::Parse("equity_usd not numeric".into())),
    }
    .map_err(|e| PriceError::Parse(format!("bad equity_usd: {e}")))?;
    Ok(Quote { price, currency: "USD".into() })
}
```

Add `pub mod hyperliquid;` to `backend/src/pricing/mod.rs`.

- [ ] **Step 3: Run the tests**

Run: `cd backend && cargo test parses_equity_from_balance_response missing_equity_is_parse_error`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/src/pricing/hyperliquid.rs backend/src/pricing/mod.rs
git commit -m "feat(pricing): Hyperliquid bot-API balance client"
```

---

### Task 2: Wire the `hyperliquid:` price source into refresh_all

**Files:**
- Modify: `backend/src/pricing/service.rs` (inside the instrument loop in `refresh_all`, after the existing source arms)

**Interfaces:**
- Consumes: `pricing::hyperliquid::BotClient`, `prices::upsert_latest`.
- Produces: a `price_quote` row (source `"hyperliquid"`) for the instrument whose `price_source` starts with `hyperliquid:`.

- [ ] **Step 1: Add the dispatch arm** — inside the `for ins in ...` loop in `refresh_all`, add:

```rust
        // Hyperliquid account equity: price of the synthetic 1-unit instrument
        // equals the account's USD equity, pulled read-only from the bot API.
        if ins.price_source.starts_with("hyperliquid:") {
            if let Some(client) = crate::pricing::hyperliquid::BotClient::from_env() {
                match client.account_equity().await {
                    Ok(q) => {
                        let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "hyperliquid", &today).await;
                    }
                    Err(e) => tracing::warn!("hyperliquid equity refresh failed for {}: {e}", ins.symbol),
                }
            }
        }
```

- [ ] **Step 2: Verify build**

Run: `cd backend && cargo build`
Expected: clean (wiring covered end-to-end by Task 3 setup + the unit-tested client in Task 1).

- [ ] **Step 3: Commit**

```bash
git add backend/src/pricing/service.rs
git commit -m "feat(pricing): refresh Hyperliquid equity from bot API in refresh_all"
```

---

### Task 3: Account/instrument setup on startup

**Files:**
- Modify: `backend/src/repo/accounts.rs` (add `find_by_name` if absent)
- Create: `backend/src/setup.rs`
- Modify: `backend/src/main.rs` (`mod setup;`; call setup when `HYPERLIQUID_API_URL` is set, before `scheduler::spawn`)

**Interfaces:**
- Produces: `accounts::find_by_name(db, name) -> anyhow::Result<Option<AccountRow>>`; `setup::ensure_hyperliquid_account(db) -> anyhow::Result<()>`; `setup::{HL_SYMBOL, HL_ACCOUNT_NAME}`.

- [ ] **Step 1: Write the failing test for find_by_name** — add to the `tests` module in `backend/src/repo/accounts.rs` (create one mirroring other repo test modules if absent):

```rust
    #[tokio::test]
    async fn find_by_name_returns_created_account() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(find_by_name(&db, "Hyperliquid").await.unwrap().is_none());
        create(&db, &NewAccount {
            name: "Hyperliquid".into(), account_type: "exchange".into(),
            institution: None, native_currency: "USD".into(), note: None,
        }).await.unwrap();
        let found = find_by_name(&db, "Hyperliquid").await.unwrap().expect("found");
        assert_eq!(found.name, "Hyperliquid");
    }
```

- [ ] **Step 2: Implement find_by_name** — add to `backend/src/repo/accounts.rs` (model the column list on the existing `create`/list queries):

```rust
pub async fn find_by_name(db: &Db, name: &str) -> anyhow::Result<Option<AccountRow>> {
    Ok(sqlx::query_as::<_, AccountRow>(
        "SELECT id, name, account_type, institution, native_currency, note, created_at
         FROM account WHERE name = ? LIMIT 1",
    )
    .bind(name)
    .fetch_optional(db)
    .await?)
}
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test find_by_name_returns_created_account`
Expected: PASS.

- [ ] **Step 4: Write the failing setup test** — create `backend/src/setup.rs`:

```rust
//! One-time, idempotent setup for the Hyperliquid equity account.

use crate::db::Db;
use crate::repo::{accounts, instruments, transactions};

pub const HL_SYMBOL: &str = "HL-EQUITY";
pub const HL_ACCOUNT_NAME: &str = "Hyperliquid";

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn ensure_is_idempotent_and_creates_synthetic_holding() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        ensure_hyperliquid_account(&db).await.unwrap();
        ensure_hyperliquid_account(&db).await.unwrap(); // no-op second time

        let ins = instruments::find_by_symbol(&db, HL_SYMBOL).await.unwrap().expect("instrument");
        assert_eq!(ins.price_source, "hyperliquid:bot");
        assert_eq!(ins.native_currency, "USD");
        let acct = accounts::find_by_name(&db, HL_ACCOUNT_NAME).await.unwrap().expect("account");
        assert_eq!(acct.account_type, "exchange");
        let all = instruments::list(&db).await.unwrap();
        assert_eq!(all.iter().filter(|i| i.symbol == HL_SYMBOL).count(), 1);
    }
}
```

- [ ] **Step 5: Run to confirm failure**

Run: `cd backend && cargo test ensure_is_idempotent_and_creates_synthetic_holding`
Expected: FAIL — `ensure_hyperliquid_account` not found.

- [ ] **Step 6: Implement ensure_hyperliquid_account** — add to `backend/src/setup.rs` (above the test module; match the real `NewAccount`/`NewInstrument`/`NewTransaction` field sets):

```rust
/// Create the Hyperliquid account, the synthetic `HL-EQUITY` instrument, and a
/// single quantity-1 opening-balance holding. Idempotent: gated on the
/// instrument's existence, so re-running on every startup is safe.
pub async fn ensure_hyperliquid_account(db: &Db) -> anyhow::Result<()> {
    if instruments::find_by_symbol(db, HL_SYMBOL).await?.is_some() {
        return Ok(());
    }
    let account = match accounts::find_by_name(db, HL_ACCOUNT_NAME).await? {
        Some(a) => a,
        None => {
            accounts::create(db, &accounts::NewAccount {
                name: HL_ACCOUNT_NAME.into(),
                account_type: "exchange".into(),
                institution: Some("Hyperliquid".into()),
                native_currency: "USD".into(),
                note: Some("Auto-created for Hyperliquid equity tracking".into()),
            })
            .await?
        }
    };
    let instrument = instruments::create(db, &instruments::NewInstrument {
        symbol: HL_SYMBOL.into(),
        name: "Hyperliquid Account Equity".into(),
        instrument_type: "other".into(),
        native_currency: "USD".into(),
        category_id: None,
        price_source: "hyperliquid:bot".into(),
        decimals: Some(2),
        note: None,
    })
    .await?;
    // Synthetic 1-unit holding; market value comes entirely from the equity
    // price quote × the live USD/IDR fx.
    transactions::create(db, &transactions::NewTransaction {
        account_id: account.id,
        instrument_id: instrument.id,
        txn_type: "opening_balance".into(),
        executed_at: chrono::Utc::now(),
        quantity: "1".into(),
        price_native: "0".into(),
        fee_native: None,
        currency: "USD".into(),
        fx_to_idr: "1".into(),
        fx_to_usd: "1".into(),
        note: Some("Synthetic 1-unit holding; value = account equity".into()),
        source: Some("hyperliquid-setup".into()),
        external_id: Some("hl-equity-opening".into()),
    })
    .await?;
    Ok(())
}
```

- [ ] **Step 7: Run the setup test**

Run: `cd backend && cargo test ensure_is_idempotent_and_creates_synthetic_holding`
Expected: PASS.

- [ ] **Step 8: Wire into startup** — in `backend/src/main.rs` add `mod setup;`, then before `scheduler::spawn(...)`:

```rust
    if std::env::var("HYPERLIQUID_API_URL").is_ok() {
        if let Err(e) = setup::ensure_hyperliquid_account(&db).await {
            tracing::warn!("hyperliquid setup failed: {e:#}");
        }
    }
```

- [ ] **Step 9: Build + commit**

Run: `cd backend && cargo build`
Expected: clean.

```bash
git add backend/src/repo/accounts.rs backend/src/setup.rs backend/src/main.rs
git commit -m "feat(setup): provision Hyperliquid equity account on startup"
```

---

### Task 4: prices::series time-series read

**Files:**
- Modify: `backend/src/repo/prices.rs` (add `series`)

**Interfaces:**
- Produces: `prices::series(db, instrument_id) -> anyhow::Result<Vec<(String, Decimal)>>` — `(as_of, price)` ascending.

- [ ] **Step 1: Write the failing test** — add to the `tests` module in `backend/src/repo/prices.rs`:

```rust
    #[tokio::test]
    async fn series_returns_quotes_ascending_by_date() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "HL-EQUITY".into(), name: "HL".into(), instrument_type: "other".into(),
            native_currency: "USD".into(), category_id: None,
            price_source: "hyperliquid:bot".into(), decimals: Some(2), note: None,
        }).await.unwrap();
        upsert_latest(&db, ins.id, d!(100), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        upsert_latest(&db, ins.id, d!(120), "USD", "hyperliquid", "2026-06-03").await.unwrap();
        upsert_latest(&db, ins.id, d!(90), "USD", "hyperliquid", "2026-06-02").await.unwrap();
        let s = series(&db, ins.id).await.unwrap();
        assert_eq!(s.len(), 3);
        assert_eq!(s[0], ("2026-06-01".to_string(), d!(100)));
        assert_eq!(s[2], ("2026-06-03".to_string(), d!(120)));
    }
```

(Use whatever decimal macro alias the existing tests in this file use — shown here as `d!`.)

- [ ] **Step 2: Implement series** — add to `backend/src/repo/prices.rs` (match the price column name used by the existing `last_two`/`latest` SELECTs):

```rust
pub async fn series(db: &Db, instrument_id: i64) -> anyhow::Result<Vec<(String, rust_decimal::Decimal)>> {
    use std::str::FromStr;
    let rows = sqlx::query_as::<_, (String, String)>(
        "SELECT as_of, price_native FROM price_quote
         WHERE instrument_id = ? AND kind = 'latest' ORDER BY as_of ASC",
    )
    .bind(instrument_id)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(as_of, p)| rust_decimal::Decimal::from_str(&p).ok().map(|d| (as_of, d)))
        .collect())
}
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test series_returns_quotes_ascending_by_date`
Expected: PASS. (If it fails on the column name, copy the exact price column from `last_two` in the same file.)

- [ ] **Step 4: Commit**

```bash
git add backend/src/repo/prices.rs
git commit -m "feat(repo): add price-quote series read"
```

---

## Phase 2 — Perp storage + sync

### Task 5: Perp tables migration + repo

**Files:**
- Create: `backend/migrations/00NN_hyperliquid_perp.sql` (NN = next free number vs `origin/main`)
- Create: `backend/src/repo/hl.rs`
- Modify: `backend/src/repo/mod.rs` (add `pub mod hl;`)

**Interfaces:**
- Produces:
  - `repo::hl::{HlPosition, HlTrade}` row structs (sqlx `FromRow`)
  - `repo::hl::replace_positions(db, &[HlPosition]) -> anyhow::Result<()>`
  - `repo::hl::insert_trade_if_new(db, &HlTrade) -> anyhow::Result<bool>` (false if `external_id` already present)
  - `repo::hl::list_positions(db) -> anyhow::Result<Vec<HlPosition>>`
  - `repo::hl::list_trades(db, limit: i64) -> anyhow::Result<Vec<HlTrade>>` (newest first by `closed_at`)

- [ ] **Step 1: Write the migration** — create `backend/migrations/00NN_hyperliquid_perp.sql`:

```sql
CREATE TABLE IF NOT EXISTS hl_position (
    coin            TEXT PRIMARY KEY,
    direction       TEXT NOT NULL,
    size            TEXT NOT NULL,
    entry_px        TEXT NOT NULL,
    mark_px         TEXT NOT NULL,
    unrealized_pnl  TEXT NOT NULL,
    leverage        TEXT NOT NULL,
    notional        TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS hl_trade (
    external_id   TEXT PRIMARY KEY,
    coin          TEXT NOT NULL,
    direction     TEXT NOT NULL,
    size          TEXT NOT NULL,
    entry_px      TEXT NOT NULL,
    exit_px       TEXT NOT NULL,
    realized_pnl  TEXT NOT NULL,
    fee           TEXT NOT NULL,
    opened_at     TEXT NOT NULL,
    closed_at     TEXT NOT NULL,
    leverage      INTEGER,
    confidence    INTEGER,
    timeframe     TEXT,
    profile       TEXT
);

CREATE INDEX IF NOT EXISTS idx_hl_trade_closed_at ON hl_trade(closed_at DESC);
```

- [ ] **Step 2: Write the failing repo test** — create `backend/src/repo/hl.rs` with a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trade(id: &str) -> HlTrade {
        HlTrade {
            external_id: id.into(), coin: "ETH".into(), direction: "long".into(),
            size: "1".into(), entry_px: "2000".into(), exit_px: "2100".into(),
            realized_pnl: "100".into(), fee: "2".into(),
            opened_at: "2026-06-01T00:00:00Z".into(), closed_at: "2026-06-02T00:00:00Z".into(),
            leverage: Some(5), confidence: Some(80), timeframe: Some("4h".into()), profile: Some("moderate".into()),
        }
    }

    #[tokio::test]
    async fn insert_trade_dedups_by_external_id() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(insert_trade_if_new(&db, &sample_trade("ETH:1:2000")).await.unwrap());
        assert!(!insert_trade_if_new(&db, &sample_trade("ETH:1:2000")).await.unwrap());
        assert_eq!(list_trades(&db, 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn replace_positions_swaps_snapshot() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let p = |coin: &str| HlPosition {
            coin: coin.into(), direction: "long".into(), size: "1".into(),
            entry_px: "100".into(), mark_px: "110".into(), unrealized_pnl: "10".into(),
            leverage: "5".into(), notional: "110".into(), updated_at: "2026-06-02T00:00:00Z".into(),
        };
        replace_positions(&db, &[p("ETH"), p("BTC")]).await.unwrap();
        replace_positions(&db, &[p("ETH")]).await.unwrap(); // BTC closed
        let rows = list_positions(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].coin, "ETH");
    }
}
```

- [ ] **Step 3: Implement the repo** — prepend to `backend/src/repo/hl.rs`:

```rust
//! Storage for Hyperliquid perp positions (open snapshot) and closed trades.

use crate::db::Db;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct HlPosition {
    pub coin: String,
    pub direction: String,
    pub size: String,
    pub entry_px: String,
    pub mark_px: String,
    pub unrealized_pnl: String,
    pub leverage: String,
    pub notional: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct HlTrade {
    pub external_id: String,
    pub coin: String,
    pub direction: String,
    pub size: String,
    pub entry_px: String,
    pub exit_px: String,
    pub realized_pnl: String,
    pub fee: String,
    pub opened_at: String,
    pub closed_at: String,
    pub leverage: Option<i64>,
    pub confidence: Option<i64>,
    pub timeframe: Option<String>,
    pub profile: Option<String>,
}

/// Replace the entire open-position snapshot in one transaction (positions that
/// have since closed simply disappear).
pub async fn replace_positions(db: &Db, positions: &[HlPosition]) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM hl_position").execute(&mut *tx).await?;
    for p in positions {
        sqlx::query(
            "INSERT INTO hl_position
             (coin, direction, size, entry_px, mark_px, unrealized_pnl, leverage, notional, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(&p.coin).bind(&p.direction).bind(&p.size).bind(&p.entry_px)
        .bind(&p.mark_px).bind(&p.unrealized_pnl).bind(&p.leverage).bind(&p.notional)
        .bind(&p.updated_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Insert a closed trade; returns false (no-op) when its `external_id` exists.
pub async fn insert_trade_if_new(db: &Db, t: &HlTrade) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO hl_trade
         (external_id, coin, direction, size, entry_px, exit_px, realized_pnl, fee,
          opened_at, closed_at, leverage, confidence, timeframe, profile)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&t.external_id).bind(&t.coin).bind(&t.direction).bind(&t.size)
    .bind(&t.entry_px).bind(&t.exit_px).bind(&t.realized_pnl).bind(&t.fee)
    .bind(&t.opened_at).bind(&t.closed_at).bind(t.leverage).bind(t.confidence)
    .bind(&t.timeframe).bind(&t.profile)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn list_positions(db: &Db) -> anyhow::Result<Vec<HlPosition>> {
    Ok(sqlx::query_as::<_, HlPosition>("SELECT * FROM hl_position ORDER BY coin ASC")
        .fetch_all(db)
        .await?)
}

pub async fn list_trades(db: &Db, limit: i64) -> anyhow::Result<Vec<HlTrade>> {
    Ok(sqlx::query_as::<_, HlTrade>(
        "SELECT * FROM hl_trade ORDER BY closed_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?)
}
```

Add `pub mod hl;` to `backend/src/repo/mod.rs`.

- [ ] **Step 4: Run the tests**

Run: `cd backend && cargo test --lib repo::hl`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/migrations/00NN_hyperliquid_perp.sql backend/src/repo/hl.rs backend/src/repo/mod.rs
git commit -m "feat(repo): hl_position/hl_trade tables and reads"
```

---

### Task 6: Perp sync service + scheduler wiring

**Files:**
- Create: `backend/src/service/hyperliquid_sync.rs`
- Modify: `backend/src/service/mod.rs` (add `pub mod hyperliquid_sync;`)
- Modify: `backend/src/scheduler.rs` (call the sync each tick)

**Interfaces:**
- Consumes: `repo::hl::{HlPosition, HlTrade, replace_positions, insert_trade_if_new}`.
- Produces:
  - `service::hyperliquid_sync::parse_positions(body: &serde_json::Value, now: &str) -> Vec<HlPosition>`
  - `service::hyperliquid_sync::parse_trades(body: &serde_json::Value) -> Vec<HlTrade>`
  - `service::hyperliquid_sync::run(db) -> anyhow::Result<()>` (no-op when env unset)

- [ ] **Step 1: Write the failing parse tests** — create `backend/src/service/hyperliquid_sync.rs` with a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positions_response() {
        let body = serde_json::json!([
            { "coin": "ETH", "direction": "long", "size": 1.0, "entry_px": 2000.0,
              "mark_px": 2100.0, "unrealized_pnl": 100.0, "leverage": 5.0, "notional": 2100.0 }
        ]);
        let rows = parse_positions(&body, "2026-06-18T00:00:00Z");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].coin, "ETH");
        assert_eq!(rows[0].mark_px, "2100");
        assert_eq!(rows[0].updated_at, "2026-06-18T00:00:00Z");
    }

    #[test]
    fn parses_trades_response_with_metadata() {
        let body = serde_json::json!([
            { "external_id": "ETH:1:2000", "coin": "ETH", "direction": "long",
              "size": 1.0, "entry_px": 2000.0, "exit_px": 2100.0, "realized_pnl": 100.0,
              "fee": 2.0, "opened_at_ms": 1700000000000_i64, "closed_at_ms": 1700000100000_i64,
              "confidence": 80, "timeframe": "4h", "profile": "moderate", "leverage": 5 }
        ]);
        let rows = parse_trades(&body);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].external_id, "ETH:1:2000");
        assert_eq!(rows[0].realized_pnl, "100");
        assert_eq!(rows[0].confidence, Some(80));
        assert!(rows[0].closed_at.starts_with("2023-11-14")); // ms epoch → rfc3339
    }
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cd backend && cargo test --lib service::hyperliquid_sync`
Expected: FAIL — functions not defined.

- [ ] **Step 3: Implement** — prepend to `backend/src/service/hyperliquid_sync.rs`:

```rust
//! Pull Hyperliquid perp positions/trades from the bot API into local tables.

use crate::db::Db;
use crate::repo::hl::{insert_trade_if_new, replace_positions, HlPosition, HlTrade};
use chrono::{TimeZone, Utc};

/// Trim a JSON number to a plain decimal string ("2100.0" -> "2100").
fn num_str(v: &serde_json::Value, key: &str) -> String {
    match v.get(key) {
        Some(serde_json::Value::Number(n)) => {
            let s = n.to_string();
            if let Ok(d) = rust_decimal::Decimal::from_str_exact(&s) {
                return d.normalize().to_string();
            }
            s
        }
        Some(serde_json::Value::String(s)) => s.clone(),
        _ => "0".into(),
    }
}

fn ms_to_rfc3339(v: &serde_json::Value, key: &str) -> String {
    let ms = v.get(key).and_then(|x| x.as_i64()).unwrap_or(0);
    Utc.timestamp_millis_opt(ms).single().unwrap_or_else(Utc::now).to_rfc3339()
}

pub fn parse_positions(body: &serde_json::Value, now: &str) -> Vec<HlPosition> {
    body.as_array().map(|rows| {
        rows.iter().map(|r| HlPosition {
            coin: r.get("coin").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            direction: r.get("direction").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            size: num_str(r, "size"),
            entry_px: num_str(r, "entry_px"),
            mark_px: num_str(r, "mark_px"),
            unrealized_pnl: num_str(r, "unrealized_pnl"),
            leverage: num_str(r, "leverage"),
            notional: num_str(r, "notional"),
            updated_at: now.to_string(),
        }).collect()
    }).unwrap_or_default()
}

pub fn parse_trades(body: &serde_json::Value) -> Vec<HlTrade> {
    body.as_array().map(|rows| {
        rows.iter().map(|r| HlTrade {
            external_id: r.get("external_id").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            coin: r.get("coin").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            direction: r.get("direction").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            size: num_str(r, "size"),
            entry_px: num_str(r, "entry_px"),
            exit_px: num_str(r, "exit_px"),
            realized_pnl: num_str(r, "realized_pnl"),
            fee: num_str(r, "fee"),
            opened_at: ms_to_rfc3339(r, "opened_at_ms"),
            closed_at: ms_to_rfc3339(r, "closed_at_ms"),
            leverage: r.get("leverage").and_then(|v| v.as_i64()),
            confidence: r.get("confidence").and_then(|v| v.as_i64()),
            timeframe: r.get("timeframe").and_then(|v| v.as_str()).map(String::from),
            profile: r.get("profile").and_then(|v| v.as_str()).map(String::from),
        }).collect()
    }).unwrap_or_default()
}

/// Pull positions + trades and persist. No-op (Ok) when the API env is unset.
pub async fn run(db: &Db) -> anyhow::Result<()> {
    let (base, token) = match (
        std::env::var("HYPERLIQUID_API_URL").ok().filter(|s| !s.is_empty()),
        std::env::var("HYPERLIQUID_API_TOKEN").ok().filter(|s| !s.is_empty()),
    ) {
        (Some(b), Some(t)) => (b, t),
        _ => return Ok(()),
    };
    let base = base.trim_end_matches('/');
    let client = reqwest::Client::new();
    let now = Utc::now().to_rfc3339();

    let positions: serde_json::Value = client
        .get(format!("{base}/positions"))
        .bearer_auth(&token)
        .send().await?.json().await?;
    replace_positions(db, &parse_positions(&positions, &now)).await?;

    let trades: serde_json::Value = client
        .get(format!("{base}/trades"))
        .bearer_auth(&token)
        .send().await?.json().await?;
    for t in parse_trades(&trades) {
        insert_trade_if_new(db, &t).await?;
    }
    Ok(())
}
```

Add `pub mod hyperliquid_sync;` to `backend/src/service/mod.rs`.

- [ ] **Step 4: Run the parse tests**

Run: `cd backend && cargo test --lib service::hyperliquid_sync`
Expected: PASS (2 tests).

- [ ] **Step 5: Wire into the scheduler** — in `backend/src/scheduler.rs`, inside the loop after the connectors block and before `tokio::time::sleep(interval)`:

```rust
            if let Err(e) = crate::service::hyperliquid_sync::run(&db).await {
                tracing::warn!("hyperliquid perp sync failed: {e:#}");
            }
```

- [ ] **Step 6: Build + commit**

Run: `cd backend && cargo build`
Expected: clean.

```bash
git add backend/src/service/hyperliquid_sync.rs backend/src/service/mod.rs backend/src/scheduler.rs
git commit -m "feat(service): sync Hyperliquid perp positions/trades from bot API"
```

---

## Phase 3 — USDC flow connector

### Task 7: HyperliquidConnector over /flows

**Files:**
- Create: `backend/src/connectors/hyperliquid.rs`
- Modify: `backend/src/connectors/mod.rs` (add `pub mod hyperliquid;`)
- Modify: `backend/src/connectors/factory.rs` (add the `"hyperliquid"` arm)

**Interfaces:**
- Consumes: `connectors::{Connector, ExternalTxn, SyncBatch, ConnectorError}`.
- Produces: `connectors::hyperliquid::HyperliquidConnector::new(base_url, token) -> Self`; `Connector` impl; `fn parse_flows(body: &serde_json::Value) -> Result<Vec<ExternalTxn>, ConnectorError>`.

- [ ] **Step 1: Write the failing test** — append to `backend/src/connectors/hyperliquid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_deposit_and_withdrawal_flows() {
        let body = serde_json::json!([
            { "external_id": "0xa:deposit", "kind": "deposit", "usdc": 500.0, "time_ms": 1700000000000_i64 },
            { "external_id": "0xb:withdrawal", "kind": "withdrawal", "usdc": 200.0, "time_ms": 1700000100000_i64 }
        ]);
        let out = parse_flows(&body).unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].symbol, "USDC");
        assert_eq!(out[0].currency, "USD");
        assert_eq!(out[0].quantity, "500");
        assert_eq!(out[1].kind, "withdrawal");
    }
}
```

- [ ] **Step 2: Implement** — prepend to `backend/src/connectors/hyperliquid.rs`:

```rust
use crate::connectors::{Connector, ConnectorError, ExternalTxn, SyncBatch};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};

pub struct HyperliquidConnector {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl HyperliquidConnector {
    pub fn new(base_url: String, token: String) -> Self {
        Self { base_url, token, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl Connector for HyperliquidConnector {
    async fn fetch_new(&self, _cursor: Option<&str>) -> Result<SyncBatch, ConnectorError> {
        let url = format!("{}/flows", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| ConnectorError::Http(e.to_string()))?;
        let json: serde_json::Value =
            resp.json().await.map_err(|e| ConnectorError::Parse(e.to_string()))?;
        Ok(SyncBatch { txns: parse_flows(&json)?, next_cursor: None })
    }
}

/// Map `/flows` rows to deposit/withdrawal ExternalTxns (USDC).
pub fn parse_flows(body: &serde_json::Value) -> Result<Vec<ExternalTxn>, ConnectorError> {
    let rows = body
        .as_array()
        .ok_or_else(|| ConnectorError::Parse("expected flows array".into()))?;
    let mut out = Vec::new();
    for r in rows {
        let kind = match r.get("kind").and_then(|v| v.as_str()) {
            Some(k @ ("deposit" | "withdrawal")) => k.to_string(),
            _ => continue,
        };
        let quantity = match r.get("usdc") {
            Some(serde_json::Value::Number(n)) => rust_decimal::Decimal::from_str_exact(&n.to_string())
                .map(|d| d.normalize().to_string())
                .unwrap_or_else(|_| n.to_string()),
            Some(serde_json::Value::String(s)) => s.clone(),
            _ => continue,
        };
        let time_ms = r.get("time_ms").and_then(|v| v.as_i64()).unwrap_or(0);
        let occurred_at = Utc
            .timestamp_millis_opt(time_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339();
        let external_id = r
            .get("external_id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        out.push(ExternalTxn {
            external_id,
            occurred_at,
            kind,
            symbol: "USDC".into(),
            quantity,
            fee: None,
            currency: "USD".into(),
        });
    }
    Ok(out)
}
```

Add `pub mod hyperliquid;` to `backend/src/connectors/mod.rs`.

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test parses_deposit_and_withdrawal_flows`
Expected: PASS.

- [ ] **Step 4: Register in the factory** — in `backend/src/connectors/factory.rs`, in `match row.kind.as_str()` before the `other =>` arm (parse `config_json` the same way the `evm_wallet` arm does):

```rust
        "hyperliquid" => {
            let cfg: serde_json::Value = serde_json::from_str(&row.config_json)
                .map_err(|e| ConnectorError::Config(e.to_string()))?;
            let base_url = cfg.get("base_url").and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing base_url".into()))?.to_string();
            let token = cfg.get("token").and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing token".into()))?.to_string();
            Ok(Box::new(crate::connectors::hyperliquid::HyperliquidConnector::new(base_url, token)))
        }
```

(Match the existing arm's exact `cfg` parsing approach and `ConnectorRow` field name for the config JSON.)

- [ ] **Step 5: Build + commit**

Run: `cd backend && cargo build && cargo test --lib connectors::hyperliquid`
Expected: clean + PASS.

```bash
git add backend/src/connectors/hyperliquid.rs backend/src/connectors/mod.rs backend/src/connectors/factory.rs
git commit -m "feat(connectors): Hyperliquid USDC flow connector over bot API"
```

---

## Phase 4 — Monitoring + reporting

### Task 8: Equity summary helpers + drawdown alert

**Files:**
- Create: `backend/src/service/hyperliquid.rs`
- Modify: `backend/src/service/mod.rs` (add `pub mod hyperliquid;`)
- Modify: `backend/src/assistant/proactive/alerts.rs` (add helper + wire into `evaluate`)
- Modify: `backend/src/assistant/proactive/tick.rs` (`ProactiveConfig.hl_drawdown_pct` + call site)

**Interfaces:**
- Produces:
  - `service::hyperliquid::HlEquitySummary { equity_usd: Decimal, change_pct: Option<f64> }`
  - `service::hyperliquid::equity_and_change(db, since_date: &str) -> anyhow::Result<Option<HlEquitySummary>>`
  - `service::hyperliquid::format_hyperliquid_line(&HlEquitySummary) -> String`
  - `alerts::hyperliquid_drawdown_alert(current: Decimal, peak: Decimal, threshold_pct: f64, today_wib: &str) -> Option<Alert>`

- [ ] **Step 1: Write the failing summary tests** — create `backend/src/service/hyperliquid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn formats_line_with_and_without_pct() {
        let with = HlEquitySummary { equity_usd: dec!(1234.5), change_pct: Some(2.34) };
        assert_eq!(format_hyperliquid_line(&with), "Hyperliquid: $1234.50 (+2.3%)");
        let without = HlEquitySummary { equity_usd: dec!(1000), change_pct: None };
        assert_eq!(format_hyperliquid_line(&without), "Hyperliquid: $1000.00");
    }

    #[tokio::test]
    async fn equity_and_change_computes_pct_since_baseline() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db).await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL).await.unwrap().unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(100), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(110), "USD", "hyperliquid", "2026-06-05").await.unwrap();
        let s = equity_and_change(&db, "2026-06-01").await.unwrap().expect("summary");
        assert_eq!(s.equity_usd, dec!(110));
        assert!((s.change_pct.unwrap() - 10.0).abs() < 1e-9);
    }
}
```

- [ ] **Step 2: Implement the helpers** — prepend to `backend/src/service/hyperliquid.rs`:

```rust
//! Read-side helpers over the synthetic Hyperliquid equity instrument.

use crate::db::Db;
use crate::setup::HL_SYMBOL;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

#[derive(Debug, Clone)]
pub struct HlEquitySummary {
    pub equity_usd: Decimal,
    pub change_pct: Option<f64>,
}

/// Current equity and percent change since the latest quote on or before
/// `since_date`. `None` when the instrument or any quote is absent.
pub async fn equity_and_change(db: &Db, since_date: &str) -> anyhow::Result<Option<HlEquitySummary>> {
    let instrument = match crate::repo::instruments::find_by_symbol(db, HL_SYMBOL).await? {
        Some(i) => i,
        None => return Ok(None),
    };
    let series = crate::repo::prices::series(db, instrument.id).await?;
    let current = match series.last() {
        Some((_, price)) => *price,
        None => return Ok(None),
    };
    let baseline = series.iter().rev()
        .find(|(date, _)| date.as_str() <= since_date)
        .or_else(|| series.first())
        .map(|(_, price)| *price);
    let change_pct = baseline.and_then(|b| {
        if b.is_zero() { None } else { ((current - b) / b * Decimal::from(100)).to_f64() }
    });
    Ok(Some(HlEquitySummary { equity_usd: current, change_pct }))
}

/// "Hyperliquid: $1234.50 (+2.3%)" — pct omitted when unknown.
pub fn format_hyperliquid_line(s: &HlEquitySummary) -> String {
    let pct = s.change_pct.map(|p| format!(" ({p:+.1}%)")).unwrap_or_default();
    format!("Hyperliquid: ${}{}", s.equity_usd.round_dp(2), pct)
}
```

Add `pub mod hyperliquid;` to `backend/src/service/mod.rs`.

- [ ] **Step 3: Run the summary tests**

Run: `cd backend && cargo test --lib service::hyperliquid`
Expected: PASS (2 tests).

- [ ] **Step 4: Write the failing drawdown test** — add to the `tests` module in `backend/src/assistant/proactive/alerts.rs`:

```rust
    #[test]
    fn drawdown_alerts_only_at_or_beyond_threshold() {
        let a = hyperliquid_drawdown_alert(dec!(800), dec!(1000), 15.0, "2026-06-18").expect("alert");
        assert_eq!(a.dedup_key, "hl-drawdown:2026-06-18");
        assert!(a.message.contains("Hyperliquid"));
        assert!(a.message.contains("20"));
        assert!(hyperliquid_drawdown_alert(dec!(950), dec!(1000), 15.0, "2026-06-18").is_none());
        assert!(hyperliquid_drawdown_alert(dec!(0), dec!(0), 15.0, "2026-06-18").is_none());
    }
```

- [ ] **Step 5: Implement the alert helper** — add to `alerts.rs` (near `mover_alerts`):

```rust
/// Drawdown of current equity from its peak. One alert per day when the decline
/// meets `threshold_pct`. `current`/`peak` are USD equity.
pub fn hyperliquid_drawdown_alert(
    current: rust_decimal::Decimal,
    peak: rust_decimal::Decimal,
    threshold_pct: f64,
    today_wib: &str,
) -> Option<Alert> {
    use rust_decimal::prelude::ToPrimitive;
    if peak <= rust_decimal::Decimal::ZERO || current >= peak {
        return None;
    }
    let dd_pct = ((peak - current) / peak * rust_decimal::Decimal::from(100)).to_f64().unwrap_or(0.0);
    if dd_pct < threshold_pct {
        return None;
    }
    Some(Alert {
        dedup_key: format!("hl-drawdown:{today_wib}"),
        message: format!("📉 Hyperliquid drawdown {:.1}% dari puncak (equity ${})", dd_pct, current.round_dp(2)),
    })
}
```

- [ ] **Step 6: Run the alert test**

Run: `cd backend && cargo test drawdown_alerts_only_at_or_beyond_threshold`
Expected: PASS.

- [ ] **Step 7: Wire into evaluate + config** — in `alerts.rs`, add `hl_drawdown_pct: f64` to `evaluate`'s signature and this independently-degrading section before the existing price-alert line:

```rust
    match crate::repo::instruments::find_by_symbol(db, crate::setup::HL_SYMBOL).await {
        Ok(Some(ins)) => match crate::repo::prices::series(db, ins.id).await {
            Ok(series) if !series.is_empty() => {
                let current = series.last().map(|(_, p)| *p).unwrap_or_default();
                let peak = series.iter().map(|(_, p)| *p).max().unwrap_or(current);
                if let Some(a) = hyperliquid_drawdown_alert(current, peak, hl_drawdown_pct, today_wib) {
                    alerts.push(a);
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("alerts: hl prices unavailable: {e:#}"),
        },
        Ok(None) => {}
        Err(e) => tracing::warn!("alerts: hl instrument lookup failed: {e:#}"),
    }
```

In `tick.rs`, add `pub hl_drawdown_pct: f64` to `ProactiveConfig`, parse it in `from_env`:

```rust
            hl_drawdown_pct: std::env::var("HL_DRAWDOWN_PCT").ok().and_then(|v| v.parse().ok()).unwrap_or(15.0),
```

and pass `config.hl_drawdown_pct` at the `evaluate(...)` call site.

- [ ] **Step 8: Build + tests + commit**

Run: `cd backend && cargo test --lib assistant::proactive && cargo build`
Expected: PASS + clean.

```bash
git add backend/src/service/hyperliquid.rs backend/src/service/mod.rs backend/src/assistant/proactive/alerts.rs backend/src/assistant/proactive/tick.rs
git commit -m "feat(monitoring): Hyperliquid equity summary + drawdown alert"
```

---

### Task 9: Briefing + weekly + monthly recap lines

**Files:**
- Modify: `backend/src/assistant/proactive/briefing.rs` (`BriefingData.hyperliquid` + gather + render)
- Modify: `backend/src/assistant/proactive/recap.rs` (`RecapData.hyperliquid` + gather + render)
- Modify: `backend/src/assistant/proactive/monthly_recap.rs` (`MonthlyRecapData.hyperliquid` + gather + render)

**Interfaces:**
- Consumes: `service::hyperliquid::{HlEquitySummary, equity_and_change, format_hyperliquid_line}`.

- [ ] **Step 1: Briefing** — add `pub hyperliquid: Option<crate::service::hyperliquid::HlEquitySummary>` to `BriefingData`; in `gather` (where `yesterday` is in scope) add `let hyperliquid = crate::service::hyperliquid::equity_and_change(db, &yesterday).await.unwrap_or(None);` and set it in the returned literal; in `render_data_block`, after the net-worth delta block, add:

```rust
    if let Some(hl) = &d.hyperliquid {
        out.push_str(&crate::service::hyperliquid::format_hyperliquid_line(hl));
        out.push('\n');
    }
```

- [ ] **Step 2: Add a briefing render test** — add to the `tests` module in `briefing.rs`:

```rust
    #[test]
    fn render_includes_hyperliquid_line_when_present() {
        use crate::service::hyperliquid::{format_hyperliquid_line, HlEquitySummary};
        let line = format_hyperliquid_line(&HlEquitySummary {
            equity_usd: rust_decimal_macros::dec!(2500), change_pct: Some(-3.2),
        });
        assert_eq!(line, "Hyperliquid: $2500.00 (-3.2%)");
    }
```

- [ ] **Step 3: Weekly recap** — same three edits in `recap.rs` using `week_ago_date` as the baseline; render after the net-worth/week-delta line (match the function's existing output accumulator name).

- [ ] **Step 4: Monthly recap** — same in `monthly_recap.rs` using `&format!("{month_label}-01")` as the baseline; render after the net-worth-change line.

- [ ] **Step 5: Build + test + commit**

Run: `cd backend && cargo test --lib assistant::proactive && cargo build`
Expected: PASS + clean (each `*Data` literal now sets `hyperliquid` — the build catches omissions).

```bash
git add backend/src/assistant/proactive/briefing.rs backend/src/assistant/proactive/recap.rs backend/src/assistant/proactive/monthly_recap.rs
git commit -m "feat(reporting): Hyperliquid equity line in briefing and recaps"
```

---

## Phase 5 — Endpoint + frontend

### Task 10: GET /portfolio/hyperliquid endpoint

**Files:**
- Modify: `backend/src/service/hyperliquid.rs` (add `HyperliquidView` + `build_hyperliquid_view`)
- Modify: `backend/src/api/portfolio.rs` (add handler)
- Modify: `backend/src/api/mod.rs` (register the route in the `protected` router)

**Interfaces:**
- Consumes: `domain::performance::{compute, PerfMetrics}`, `repo::prices::series`, `repo::hl::{list_positions, list_trades, HlPosition, HlTrade}`.
- Produces: `service::hyperliquid::HyperliquidView { points: Vec<HlPoint>, metrics: PerfMetrics, current_value_usd: String, positions: Vec<HlPosition>, trades: Vec<HlTrade>, realized_pnl_total: String, win_rate: Option<f64>, insufficient_data: bool }`; `HlPoint { date, cum_return, nav }`; `build_hyperliquid_view(db) -> anyhow::Result<HyperliquidView>`; route `GET /portfolio/hyperliquid`.

- [ ] **Step 1: Write the failing test** — add to the `tests` module in `service/hyperliquid.rs`:

```rust
    #[tokio::test]
    async fn build_view_produces_curve_positions_and_stats() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db).await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL).await.unwrap().unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1000), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1100), "USD", "hyperliquid", "2026-06-02").await.unwrap();
        let view = build_hyperliquid_view(&db).await.unwrap();
        assert!(!view.insufficient_data);
        assert_eq!(view.current_value_usd, "1100");
        assert!((view.metrics.total_return - 0.10).abs() < 1e-9);
    }
```

- [ ] **Step 2: Implement the view builder** — add to `service/hyperliquid.rs` (confirm `PerfMetrics` field set and `compute`'s signature against `domain/performance.rs`):

```rust
use crate::domain::performance::{compute, PerfMetrics};
use crate::repo::hl::{list_positions, list_trades, HlPosition, HlTrade};
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

#[derive(Serialize)]
pub struct HlPoint {
    pub date: String,
    pub cum_return: f64,
    pub nav: f64,
}

#[derive(Serialize)]
pub struct HyperliquidView {
    pub points: Vec<HlPoint>,
    pub metrics: PerfMetrics,
    pub current_value_usd: String,
    pub positions: Vec<HlPosition>,
    pub trades: Vec<HlTrade>,
    pub realized_pnl_total: String,
    pub win_rate: Option<f64>,
    pub insufficient_data: bool,
}

/// Equity curve (TWR) for the Hyperliquid account plus current open positions,
/// recent closed trades, and aggregate realized stats.
pub async fn build_hyperliquid_view(db: &Db) -> anyhow::Result<HyperliquidView> {
    let trades = list_trades(db, 200).await?;
    let positions = list_positions(db).await?;

    // Realized PnL total + win rate from stored closed trades.
    let mut realized = Decimal::ZERO;
    let mut wins = 0i64;
    for t in &trades {
        if let Ok(p) = Decimal::from_str(&t.realized_pnl) {
            realized += p;
            if p > Decimal::ZERO { wins += 1; }
        }
    }
    let win_rate = if trades.is_empty() { None } else { Some(wins as f64 / trades.len() as f64) };

    let instrument = crate::repo::instruments::find_by_symbol(db, HL_SYMBOL).await?;
    let series = match &instrument {
        Some(i) => crate::repo::prices::series(db, i.id).await?,
        None => Vec::new(),
    };
    let mut navs: Vec<(NaiveDate, f64)> = Vec::new();
    for (as_of, price) in &series {
        if let (Ok(date), Some(v)) = (NaiveDate::parse_from_str(as_of, "%Y-%m-%d"), price.to_f64()) {
            navs.push((date, v));
        }
    }
    // Equity-curve TWR with no external flows here (flows live in txn analytics).
    let flows: Vec<(NaiveDate, f64)> = Vec::new();
    let (points, metrics) = compute(&navs, &flows);
    let current_value_usd = series.last().map(|(_, p)| p.to_string()).unwrap_or_else(|| "0".into());

    Ok(HyperliquidView {
        points: points.into_iter().map(|p| HlPoint {
            date: p.date.format("%Y-%m-%d").to_string(),
            cum_return: p.cum_return,
            nav: p.nav,
        }).collect(),
        metrics,
        current_value_usd,
        positions,
        trades,
        realized_pnl_total: realized.normalize().to_string(),
        win_rate,
        insufficient_data: navs.len() < 2,
    })
}
```

(`use rust_decimal::prelude::ToPrimitive;` is already imported at the top of the file from Task 8.)

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test build_view_produces_curve_positions_and_stats`
Expected: PASS.

- [ ] **Step 4: Add the handler + route** — in `backend/src/api/portfolio.rs`:

```rust
pub async fn hyperliquid(
    State(s): State<AppState>,
) -> Result<Json<crate::service::hyperliquid::HyperliquidView>, AppError> {
    Ok(Json(
        crate::service::hyperliquid::build_hyperliquid_view(&s.db).await.map_err(AppError::Other)?,
    ))
}
```

In `backend/src/api/mod.rs`, in the `protected` router with the other `/portfolio/*` routes:

```rust
        .route("/portfolio/hyperliquid", get(portfolio::hyperliquid))
```

(Match the real `AppState`/`AppError` names and the existing handler signatures in `portfolio.rs`.)

- [ ] **Step 5: Build + commit**

Run: `cd backend && cargo build`
Expected: clean.

```bash
git add backend/src/service/hyperliquid.rs backend/src/api/portfolio.rs backend/src/api/mod.rs
git commit -m "feat(api): GET /portfolio/hyperliquid equity, positions, trades, stats"
```

---

### Task 11: Frontend schema + query hook

**Files:**
- Modify: `frontend/src/api/schemas.ts` (add `HyperliquidViewSchema`)
- Modify: `frontend/src/api/hooks.ts` (add `useHyperliquid`)

**Interfaces:**
- Produces: `HyperliquidView` type; `useHyperliquid()` hitting `/portfolio/hyperliquid`.

- [ ] **Step 1: Add the schema** — in `frontend/src/api/schemas.ts`:

```typescript
const HlPositionSchema = z.object({
  coin: z.string(), direction: z.string(), size: z.string(), entry_px: z.string(),
  mark_px: z.string(), unrealized_pnl: z.string(), leverage: z.string(),
  notional: z.string(), updated_at: z.string(),
});
const HlTradeSchema = z.object({
  external_id: z.string(), coin: z.string(), direction: z.string(), size: z.string(),
  entry_px: z.string(), exit_px: z.string(), realized_pnl: z.string(), fee: z.string(),
  opened_at: z.string(), closed_at: z.string(), leverage: z.number().nullable(),
  confidence: z.number().nullable(), timeframe: z.string().nullable(), profile: z.string().nullable(),
});
export const HyperliquidViewSchema = z.object({
  points: z.array(z.object({ date: z.string(), cum_return: z.number(), nav: z.number() })),
  metrics: z.object({
    total_return: z.number(), annualized: z.number().nullable(),
    max_drawdown: z.number(), volatility: z.number(),
  }),
  current_value_usd: z.string(),
  positions: z.array(HlPositionSchema),
  trades: z.array(HlTradeSchema),
  realized_pnl_total: z.string(),
  win_rate: z.number().nullable(),
  insufficient_data: z.boolean(),
});
export type HyperliquidView = z.infer<typeof HyperliquidViewSchema>;
```

(Match the `metrics` object to the real `PerfMetrics` field set serialized by Task 10.)

- [ ] **Step 2: Add the hook** — in `frontend/src/api/hooks.ts`:

```typescript
export const useHyperliquid = () =>
  useQuery({
    queryKey: ["hyperliquid"],
    queryFn: () => api.get("/portfolio/hyperliquid", HyperliquidViewSchema),
  });
```

(Import `HyperliquidViewSchema` following the file's existing import style.)

- [ ] **Step 3: Typecheck + commit**

Run: `cd frontend && npm run build`
Expected: type-checks and builds.

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts
git commit -m "feat(web): Hyperliquid view schema + query hook"
```

---

### Task 12: Dashboard equity card

**Files:**
- Create: `frontend/src/components/HyperliquidCard.tsx`
- Modify: `frontend/src/pages/DashboardPage.tsx` (render the card)

**Interfaces:**
- Consumes: `useHyperliquid`, Recharts `AreaChart`.
- Produces: `<HyperliquidCard />`.

- [ ] **Step 1: Create the card** — `frontend/src/components/HyperliquidCard.tsx` (mirror `MoversCard` structure + PerformancePage chart styling):

```tsx
import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useHyperliquid } from "@/api/hooks";

export function HyperliquidCard() {
  const q = useHyperliquid();
  const view = q.data;
  const chartData = (view?.points ?? []).map((p) => ({ date: p.date, returnPct: p.cum_return * 100 }));
  const totalPct = view ? view.metrics.total_return * 100 : 0;

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Hyperliquid</div>
          <div className="card-sub">equity & return</div>
        </div>
      </div>
      <div className="card-pad flex col" style={{ paddingTop: 6 }}>
        {q.isLoading ? (
          <div className="skeleton" style={{ width: "100%", height: 120 }} />
        ) : !view || view.insufficient_data ? (
          <div className="empty">
            <div className="t-h3">Belum cukup data</div>
            <div className="t-sm t-muted">Kurva muncul setelah ada dua hari data equity.</div>
          </div>
        ) : (
          <>
            <div className="flex items-baseline gap-3">
              <span className="num t-h2">${view.current_value_usd}</span>
              <span className={totalPct >= 0 ? "gain" : "loss"}>
                {totalPct >= 0 ? "▲" : "▼"} {Math.abs(totalPct).toFixed(2)}%
              </span>
            </div>
            <div style={{ width: "100%", height: 140 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 10, right: 8, left: 0, bottom: 0 }}>
                  <XAxis dataKey="date" fontSize={10} tickLine={false} axisLine={false}
                    stroke="hsl(var(--muted-foreground))" minTickGap={28} />
                  <YAxis tickFormatter={(v: number) => `${v.toFixed(0)}%`} width={40} fontSize={10}
                    tickLine={false} axisLine={false} stroke="hsl(var(--muted-foreground))" />
                  <Tooltip formatter={(v: number) => `${v.toFixed(2)}%`}
                    contentStyle={{ background: "hsl(var(--popover))", border: "1px solid hsl(var(--border))",
                      borderRadius: "var(--radius)", color: "hsl(var(--popover-foreground))", fontSize: 12 }} />
                  <Area type="monotone" dataKey="returnPct" stroke="hsl(var(--chart-1))" strokeWidth={1.5}
                    fill="hsl(var(--chart-1))" fillOpacity={0.15} dot={false} />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Render it on the dashboard** — in `DashboardPage.tsx`, import and add `<HyperliquidCard />` to the card grid (next to `MoversCard`).

- [ ] **Step 3: Typecheck + commit**

Run: `cd frontend && npm run build`
Expected: builds.

```bash
git add frontend/src/components/HyperliquidCard.tsx frontend/src/pages/DashboardPage.tsx
git commit -m "feat(web): Hyperliquid equity card on dashboard"
```

---

### Task 13: Perp positions + trades section

**Files:**
- Create: `frontend/src/components/HyperliquidPositions.tsx`
- Modify: `frontend/src/pages/PerformancePage.tsx` (render the card + positions/trades section)
- Create: `frontend/src/pages/PerformancePage.hyperliquid.test.tsx` (component test)

**Interfaces:**
- Consumes: `useHyperliquid`.
- Produces: `<HyperliquidPositions />` rendering open positions (unrealized PnL) and recent closed trades (realized PnL + metadata) + aggregate stats.

- [ ] **Step 1: Create the section** — `frontend/src/components/HyperliquidPositions.tsx`:

```tsx
import { useHyperliquid } from "@/api/hooks";

export function HyperliquidPositions() {
  const q = useHyperliquid();
  const view = q.data;
  if (q.isLoading) return <div className="skeleton" style={{ width: "100%", height: 200 }} />;
  if (!view) return null;
  const pnlClass = (v: string) => (Number(v) >= 0 ? "gain" : "loss");

  return (
    <div className="card">
      <div className="card-head">
        <div className="card-title">Hyperliquid — posisi & trade</div>
        <div className="card-sub">
          Realized PnL: <span className={pnlClass(view.realized_pnl_total)}>${view.realized_pnl_total}</span>
          {view.win_rate != null && <> · win rate {(view.win_rate * 100).toFixed(0)}%</>}
        </div>
      </div>
      <div className="card-pad">
        <div className="t-h3">Posisi terbuka</div>
        {view.positions.length === 0 ? (
          <div className="t-sm t-muted">Tidak ada posisi terbuka.</div>
        ) : (
          <table className="table">
            <thead><tr><th>Coin</th><th>Arah</th><th>Size</th><th>Entry</th><th>Mark</th><th>uPnL</th><th>Lev</th></tr></thead>
            <tbody>
              {view.positions.map((p) => (
                <tr key={p.coin}>
                  <td>{p.coin}</td><td>{p.direction}</td><td className="num">{p.size}</td>
                  <td className="num">${p.entry_px}</td><td className="num">${p.mark_px}</td>
                  <td className={`num ${pnlClass(p.unrealized_pnl)}`}>${p.unrealized_pnl}</td>
                  <td className="num">{p.leverage}x</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        <div className="t-h3" style={{ marginTop: 16 }}>Trade terakhir</div>
        {view.trades.length === 0 ? (
          <div className="t-sm t-muted">Belum ada trade selesai.</div>
        ) : (
          <table className="table">
            <thead><tr><th>Coin</th><th>Arah</th><th>Entry</th><th>Exit</th><th>PnL</th><th>TF</th><th>Tutup</th></tr></thead>
            <tbody>
              {view.trades.map((t) => (
                <tr key={t.external_id}>
                  <td>{t.coin}</td><td>{t.direction}</td><td className="num">${t.entry_px}</td>
                  <td className="num">${t.exit_px}</td>
                  <td className={`num ${pnlClass(t.realized_pnl)}`}>${t.realized_pnl}</td>
                  <td>{t.timeframe ?? "—"}</td><td>{t.closed_at.slice(0, 10)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
```

(Match the repo's existing table classes; if there is no `.table` style, mirror whatever the holdings/transactions lists use.)

- [ ] **Step 2: Render on the Performance page** — in `PerformancePage.tsx`, import and render `<HyperliquidCard />` and `<HyperliquidPositions />` below the main performance chart.

- [ ] **Step 3: Write a component test** — `frontend/src/pages/PerformancePage.hyperliquid.test.tsx` (use the MSW server + render pattern from the repo's other component tests):

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "@/test/server";
import { HyperliquidPositions } from "@/components/HyperliquidPositions";

function renderSection() {
  localStorage.setItem("pt-auth-token", "test-token");
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <HyperliquidPositions />
    </QueryClientProvider>,
  );
}

test("shows an open position and a closed trade", async () => {
  server.use(
    http.get("*/portfolio/hyperliquid", () =>
      HttpResponse.json({
        points: [], metrics: { total_return: 0.1, annualized: null, max_drawdown: -0.05, volatility: 0.2 },
        current_value_usd: "1100",
        positions: [{ coin: "ETH", direction: "long", size: "1", entry_px: "2000",
          mark_px: "2100", unrealized_pnl: "100", leverage: "5", notional: "2100", updated_at: "2026-06-18T00:00:00Z" }],
        trades: [{ external_id: "ETH:1:2000", coin: "ETH", direction: "long", size: "1",
          entry_px: "2000", exit_px: "2100", realized_pnl: "100", fee: "2",
          opened_at: "2026-06-01T00:00:00Z", closed_at: "2026-06-02T00:00:00Z",
          leverage: 5, confidence: 80, timeframe: "4h", profile: "moderate" }],
        realized_pnl_total: "100", win_rate: 1.0, insufficient_data: true,
      }),
    ),
  );
  renderSection();
  await waitFor(() => expect(screen.getByText("ETH")).toBeInTheDocument());
});
```

(If the MSW handler/import path differs, match the repo's other component tests.)

- [ ] **Step 4: Run the test**

Run: `cd frontend && npm test -- PerformancePage.hyperliquid`
Expected: PASS.

- [ ] **Step 5: Final full-suite check + commit**

Run: `cd backend && cargo test && cd ../frontend && npm test && npm run build`
Expected: all pass.

```bash
git add frontend/src/components/HyperliquidPositions.tsx frontend/src/pages/PerformancePage.tsx frontend/src/pages/PerformancePage.hyperliquid.test.tsx
git commit -m "feat(web): Hyperliquid positions and trades section"
```

---

## Self-Review

**Spec coverage (portfolio-side sections of the design):**
- Equity feed via bot `/balance` → pricing provider → net worth / TWR / movers → Tasks 1–4. ✓
- Synthetic `HL-EQUITY` setup (`price_source = "hyperliquid:bot"`), no on-chain pull → Task 3. ✓
- Bonus stale-price liveness (source ≠ `manual`) → automatic; no code. ✓
- Perp tables + dedicated sync (positions snapshot upsert; trades dedup by `external_id`) → Tasks 5–6. ✓
- USDC flow connector for TWR accuracy → Task 7. ✓
- Drawdown alert reusing proactive machinery → Task 8. ✓
- Briefing + weekly + monthly recap lines → Task 9. ✓
- Endpoint + dashboard card + perp positions/trades section → Tasks 10–13. ✓
- Config (`HYPERLIQUID_API_URL`, `HYPERLIQUID_API_TOKEN`, `HL_DRAWDOWN_PCT`) → Tasks 1, 3, 8. ✓
- Perp data NOT forced into the spot `txn` model → Tasks 5–6 use separate tables. ✓

**Type consistency:** `HL_SYMBOL`/`HL_ACCOUNT_NAME` defined once in `setup.rs` and reused; `HlPosition`/`HlTrade` defined in `repo::hl` and reused by sync, endpoint, and frontend schema; `HlEquitySummary`/`format_hyperliquid_line`/`equity_and_change` stable across alerts/briefing/recap; `evaluate` signature updated at definition and call site (Task 8).

**Notes for the implementer (verify against live code, keep the test):**
- Migration number — pick the next free number checked against `origin/main` (`0023` skipped locally).
- `prices::series` price column (`price_native`) — confirm against `last_two`/`latest`.
- `NewAccount`/`NewInstrument`/`NewTransaction`/`AccountRow` field sets — match `repo/accounts.rs` and `repo/instruments.rs`.
- `domain::performance::compute` signature + `PerfMetrics` field set — confirm before Task 10; mirror them in the frontend `metrics` schema.
- `AppState`/`AppError` names and `portfolio.rs` handler signatures — match the existing handlers.
- Each `*Data` struct literal (`BriefingData`, `RecapData`, `MonthlyRecapData`) must set the new `hyperliquid` field — the build catches omissions.
- `ConnectorRow` config-JSON field name and the factory's existing `cfg` parsing — match the `evm_wallet` arm.
- Frontend table classes + MSW test helper paths — match existing components/tests.
