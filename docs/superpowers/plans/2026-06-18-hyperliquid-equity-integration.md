# Hyperliquid Equity Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface the Hyperliquid trading account's equity inside portfolio-tracker's existing analytics, monitoring, and reporting by pulling account equity read-only from the Hyperliquid info API.

**Architecture:** Hyperliquid is modeled as one `exchange` account holding a single synthetic instrument (`HL-EQUITY`) whose 1-unit price equals the account's USD equity. A new pricing provider refreshes that price on the existing scheduler, so equity flows automatically into net worth, the TWR curve, daily movers, milestone alerts, and the briefing. On top of that automatic inclusion we add a drawdown alert, briefing/recap lines, a dedicated equity-curve endpoint + frontend section, and (last) a connector that records USDC deposits/withdrawals so TWR is not distorted by fund transfers.

**Tech Stack:** Rust (Axum, sqlx/SQLite, reqwest, rust_decimal, tokio, async-trait, chrono, serde, tracing); React + TypeScript (Vite, TanStack Query, Zod, Recharts, Vitest + Testing Library).

## Global Constraints

- All changes live in `portfolio-tracker`. **`agent-hyperliquid` is not modified.**
- Backend money values are `rust_decimal::Decimal`, stored in SQLite as TEXT strings.
- Provider/connector errors are logged (`tracing::warn!`) and non-fatal — never abort `refresh_all` or `evaluate`.
- Synthetic instrument symbol is exactly `HL-EQUITY`; account name is exactly `Hyperliquid`; `price_source` is exactly `hyperliquid:<wallet>`.
- Hyperliquid info API: `POST {base}/info`, JSON body `{"type":"clearinghouseState","user":"<wallet>"}`. Mainnet base `https://api.hyperliquid.xyz`, testnet `https://api.hyperliquid-testnet.xyz`. Equity is `marginSummary.accountValue` (a decimal string), currency USD.
- Env vars: `HYPERLIQUID_WALLET` (enables setup), `HYPERLIQUID_NETWORK` (`mainnet`|`testnet`, default `mainnet`), `HL_DRAWDOWN_PCT` (default `15.0`).
- Backend tests run from `backend/` with `cargo test`; frontend from `frontend/` with `npm test` / `npm run build`.
- Follow existing patterns: pure parse functions are unit-tested without network; DB code is tested with `crate::db::connect("sqlite::memory:")`.

---

## Phase 1 — Core equity feed

Deliverable: Hyperliquid equity appears in net worth, movers, and the global TWR curve.

### Task 1: Hyperliquid pricing provider

**Files:**
- Create: `backend/src/pricing/hyperliquid.rs`
- Modify: `backend/src/pricing/mod.rs` (add `pub mod hyperliquid;`)

**Interfaces:**
- Consumes: `crate::pricing::{PriceError, Quote}` (`Quote { price: Decimal, currency: String }`).
- Produces: `pricing::hyperliquid::Hyperliquid::new(network: &str) -> Self`; `async fn account_equity(&self, wallet: &str) -> Result<Quote, PriceError>`; `fn parse_account_equity(body: &serde_json::Value) -> Result<Quote, PriceError>`.

- [ ] **Step 1: Write the failing test** — append to `backend/src/pricing/hyperliquid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_account_value_from_margin_summary() {
        let body = serde_json::json!({
            "marginSummary": { "accountValue": "1234.56", "totalNtlPos": "0.0" },
            "assetPositions": []
        });
        let q = parse_account_equity(&body).unwrap();
        assert_eq!(q.price, dec!(1234.56));
        assert_eq!(q.currency, "USD");
    }

    #[test]
    fn missing_account_value_is_parse_error() {
        let body = serde_json::json!({ "marginSummary": {} });
        let err = parse_account_equity(&body).unwrap_err();
        matches!(err, PriceError::Parse(_));
    }
}
```

- [ ] **Step 2: Write the implementation** — prepend to the same file (above the test module):

```rust
use crate::pricing::{PriceError, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct Hyperliquid {
    base: String,
    client: reqwest::Client,
}

impl Hyperliquid {
    pub fn new(network: &str) -> Self {
        let base = match network {
            "testnet" => "https://api.hyperliquid-testnet.xyz",
            _ => "https://api.hyperliquid.xyz",
        }
        .to_string();
        Self { base, client: reqwest::Client::new() }
    }

    /// Total account equity (USD) for `wallet` via clearinghouseState.
    pub async fn account_equity(&self, wallet: &str) -> Result<Quote, PriceError> {
        let url = format!("{}/info", self.base);
        let body = serde_json::json!({ "type": "clearinghouseState", "user": wallet });
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            return Err(PriceError::Http(format!("hyperliquid info status {status}")));
        }
        let json: serde_json::Value =
            resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_account_equity(&json)
    }
}

/// Pull `marginSummary.accountValue` out of a clearinghouseState response.
pub fn parse_account_equity(body: &serde_json::Value) -> Result<Quote, PriceError> {
    let raw = body
        .get("marginSummary")
        .and_then(|m| m.get("accountValue"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| PriceError::Parse("missing marginSummary.accountValue".into()))?;
    let price = Decimal::from_str(raw)
        .map_err(|e| PriceError::Parse(format!("bad accountValue '{raw}': {e}")))?;
    Ok(Quote { price, currency: "USD".into() })
}
```

Then add the module declaration to `backend/src/pricing/mod.rs` alongside the others:

```rust
pub mod hyperliquid;
```

- [ ] **Step 3: Run the tests**

Run: `cd backend && cargo test parses_account_value_from_margin_summary missing_account_value_is_parse_error`
Expected: PASS (2 tests).

- [ ] **Step 4: Commit**

```bash
git add backend/src/pricing/hyperliquid.rs backend/src/pricing/mod.rs
git commit -m "feat(pricing): add Hyperliquid equity provider"
```

---

### Task 2: Wire the `hyperliquid:` price source into refresh_all

**Files:**
- Modify: `backend/src/pricing/service.rs` (inside the `for ins in instruments::list(db).await?` loop in `refresh_all`, after the `gold:idr_gram` block, before the loop closes ~line 112)

**Interfaces:**
- Consumes: `pricing::hyperliquid::Hyperliquid`, `prices::upsert_latest`.
- Produces: a `price_quote` row (source `"hyperliquid"`) for any instrument whose `price_source` starts with `hyperliquid:`.

- [ ] **Step 1: Add the dispatch arm** — inside the instrument loop in `refresh_all`, add:

```rust
        // Hyperliquid account equity: price of the synthetic 1-unit instrument
        // equals the account's USD equity, pulled read-only by wallet address.
        if let Some(wallet) = ins.price_source.strip_prefix("hyperliquid:") {
            let network =
                std::env::var("HYPERLIQUID_NETWORK").unwrap_or_else(|_| "mainnet".into());
            match crate::pricing::hyperliquid::Hyperliquid::new(&network)
                .account_equity(wallet)
                .await
            {
                Ok(q) => {
                    let _ = prices::upsert_latest(db, ins.id, q.price, &q.currency, "hyperliquid", &today).await;
                }
                Err(e) => tracing::warn!("hyperliquid equity refresh failed for {}: {e}", ins.symbol),
            }
        }
```

- [ ] **Step 2: Verify it compiles**

Run: `cd backend && cargo build`
Expected: builds with no errors (no new test — this is a wiring change covered end-to-end by Task 3's setup + manual run; the provider itself is tested in Task 1).

- [ ] **Step 3: Commit**

```bash
git add backend/src/pricing/service.rs
git commit -m "feat(pricing): refresh Hyperliquid equity in refresh_all"
```

---

### Task 3: Account/instrument setup + accounts::find_by_name

**Files:**
- Modify: `backend/src/repo/accounts.rs` (add `find_by_name`)
- Create: `backend/src/setup.rs`
- Modify: `backend/src/main.rs` (declare `mod setup;`; call setup when `HYPERLIQUID_WALLET` is set, before `scheduler::spawn`)

**Interfaces:**
- Consumes: `accounts::{NewAccount, AccountRow, create}`, `instruments::{NewInstrument, find_by_symbol, create}`, `transactions::{NewTransaction, create}`.
- Produces: `accounts::find_by_name(db, name) -> anyhow::Result<Option<AccountRow>>`; `setup::ensure_hyperliquid_account(db, wallet) -> anyhow::Result<()>`; `setup::HL_SYMBOL: &str = "HL-EQUITY"`; `setup::HL_ACCOUNT_NAME: &str = "Hyperliquid"`.

- [ ] **Step 1: Write the failing test for find_by_name** — add to the `tests` module in `backend/src/repo/accounts.rs` (create one mirroring other repo test modules if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn find_by_name_returns_created_account() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(find_by_name(&db, "Hyperliquid").await.unwrap().is_none());
        create(&db, &NewAccount {
            name: "Hyperliquid".into(),
            account_type: "exchange".into(),
            institution: None,
            native_currency: "USD".into(),
            note: None,
        }).await.unwrap();
        let found = find_by_name(&db, "Hyperliquid").await.unwrap().expect("found");
        assert_eq!(found.name, "Hyperliquid");
        assert_eq!(found.native_currency, "USD");
    }
}
```

- [ ] **Step 2: Implement find_by_name** — add to `backend/src/repo/accounts.rs` (model the SQL on the existing `create`/list queries for exact column list):

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

- [ ] **Step 4: Write the failing test for setup** — create `backend/src/setup.rs` with:

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
        ensure_hyperliquid_account(&db, "0xWALLET").await.unwrap();
        // Second call is a no-op (no duplicate instrument).
        ensure_hyperliquid_account(&db, "0xWALLET").await.unwrap();

        let ins = instruments::find_by_symbol(&db, HL_SYMBOL).await.unwrap().expect("instrument");
        assert_eq!(ins.price_source, "hyperliquid:0xWALLET");
        assert_eq!(ins.native_currency, "USD");

        let acct = accounts::find_by_name(&db, HL_ACCOUNT_NAME).await.unwrap().expect("account");
        assert_eq!(acct.account_type, "exchange");

        let all = instruments::list(&db).await.unwrap();
        assert_eq!(all.iter().filter(|i| i.symbol == HL_SYMBOL).count(), 1);
    }
}
```

- [ ] **Step 5: Run it to confirm it fails**

Run: `cd backend && cargo test ensure_is_idempotent_and_creates_synthetic_holding`
Expected: FAIL — `ensure_hyperliquid_account` not found.

- [ ] **Step 6: Implement ensure_hyperliquid_account** — add to `backend/src/setup.rs` (above the test module):

```rust
/// Create the Hyperliquid account, the synthetic `HL-EQUITY` instrument, and a
/// single quantity-1 opening-balance holding. Idempotent: gated on the
/// instrument's existence, so re-running on every startup is safe.
pub async fn ensure_hyperliquid_account(db: &Db, wallet: &str) -> anyhow::Result<()> {
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
        price_source: format!("hyperliquid:{wallet}"),
        decimals: Some(2),
        note: None,
    })
    .await?;
    // Synthetic 1-unit holding. price_native = 0, so cost basis is 0; market
    // value comes entirely from the equity price quote × the live USD/IDR fx.
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

- [ ] **Step 8: Wire into startup** — in `backend/src/main.rs` add `mod setup;` with the other `mod` lines, then insert before `scheduler::spawn(db, ...)` (line ~53):

```rust
    if let Ok(wallet) = std::env::var("HYPERLIQUID_WALLET") {
        if let Err(e) = setup::ensure_hyperliquid_account(&db, &wallet).await {
            tracing::warn!("hyperliquid setup failed: {e:#}");
        }
    }
```

- [ ] **Step 9: Verify build + commit**

Run: `cd backend && cargo build`
Expected: builds clean.

```bash
git add backend/src/repo/accounts.rs backend/src/setup.rs backend/src/main.rs
git commit -m "feat(setup): provision Hyperliquid equity account on startup"
```

---

## Phase 2 — Shared Hyperliquid service helpers

Deliverable: a reusable price-series read and equity-summary helpers used by monitoring, reporting, and the UI.

### Task 4: prices::series time-series read

**Files:**
- Modify: `backend/src/repo/prices.rs` (add `series`)

**Interfaces:**
- Produces: `prices::series(db, instrument_id) -> anyhow::Result<Vec<(String, Decimal)>>` — `(as_of, price)` ascending by `as_of`.

- [ ] **Step 1: Write the failing test** — add to the `tests` module in `backend/src/repo/prices.rs`:

```rust
    #[tokio::test]
    async fn series_returns_quotes_ascending_by_date() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol: "HL-EQUITY".into(), name: "HL".into(), instrument_type: "other".into(),
            native_currency: "USD".into(), category_id: None,
            price_source: "hyperliquid:0x".into(), decimals: Some(2), note: None,
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

- [ ] **Step 2: Implement series** — add to `backend/src/repo/prices.rs` (match the exact column names used by the existing `last_two`/`latest` queries — the price column is the one those select):

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
Expected: PASS. (If it fails on the column name, open `last_two` in the same file and copy its exact `SELECT` column for the price.)

- [ ] **Step 4: Commit**

```bash
git add backend/src/repo/prices.rs
git commit -m "feat(repo): add price-quote series read"
```

---

### Task 5: service/hyperliquid.rs equity-summary helpers

**Files:**
- Create: `backend/src/service/hyperliquid.rs`
- Modify: `backend/src/service/mod.rs` (add `pub mod hyperliquid;`)

**Interfaces:**
- Consumes: `instruments::find_by_symbol`, `prices::series`, `setup::HL_SYMBOL`.
- Produces:
  - `service::hyperliquid::HlEquitySummary { equity_usd: Decimal, change_pct: Option<f64> }`
  - `async fn equity_and_change(db, since_date: &str) -> anyhow::Result<Option<HlEquitySummary>>`
  - `fn format_hyperliquid_line(s: &HlEquitySummary) -> String`

- [ ] **Step 1: Write the failing tests** — create `backend/src/service/hyperliquid.rs` with a test module:

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
        crate::setup::ensure_hyperliquid_account(&db, "0x").await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await.unwrap().unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(100), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(110), "USD", "hyperliquid", "2026-06-05").await.unwrap();
        // Baseline = latest quote on/before 2026-06-01 → 100; current → 110 → +10%.
        let s = equity_and_change(&db, "2026-06-01").await.unwrap().expect("summary");
        assert_eq!(s.equity_usd, dec!(110));
        assert!((s.change_pct.unwrap() - 10.0).abs() < 1e-9);
    }

    #[tokio::test]
    async fn equity_and_change_none_when_no_instrument() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(equity_and_change(&db, "2026-06-01").await.unwrap().is_none());
    }
}
```

- [ ] **Step 2: Run to confirm failure**

Run: `cd backend && cargo test --lib service::hyperliquid`
Expected: FAIL — types/functions not defined.

- [ ] **Step 3: Implement the helpers** — prepend to `backend/src/service/hyperliquid.rs`:

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

/// Current equity and its percent change since the latest quote on or before
/// `since_date` (falls back to the earliest quote). `None` when the Hyperliquid
/// instrument or any price quote is absent.
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
    let baseline = series
        .iter()
        .rev()
        .find(|(date, _)| date.as_str() <= since_date)
        .or_else(|| series.first())
        .map(|(_, price)| *price);
    let change_pct = baseline.and_then(|b| {
        if b.is_zero() {
            None
        } else {
            ((current - b) / b * Decimal::from(100)).to_f64()
        }
    });
    Ok(Some(HlEquitySummary { equity_usd: current, change_pct }))
}

/// "Hyperliquid: $1234.50 (+2.3%)" — pct omitted when unknown.
pub fn format_hyperliquid_line(s: &HlEquitySummary) -> String {
    let pct = s
        .change_pct
        .map(|p| format!(" ({p:+.1}%)"))
        .unwrap_or_default();
    format!("Hyperliquid: ${}{}", s.equity_usd.round_dp(2), pct)
}
```

Add to `backend/src/service/mod.rs`:

```rust
pub mod hyperliquid;
```

- [ ] **Step 4: Run the tests**

Run: `cd backend && cargo test --lib service::hyperliquid`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add backend/src/service/hyperliquid.rs backend/src/service/mod.rs
git commit -m "feat(service): Hyperliquid equity summary helpers"
```

---

## Phase 3 — Monitoring: drawdown alert

Deliverable: a proactive alert fires once per day when equity falls a configurable percent below its peak.

### Task 6: hyperliquid_drawdown_alert helper

**Files:**
- Modify: `backend/src/assistant/proactive/alerts.rs` (add helper + test)

**Interfaces:**
- Consumes: `Alert`, `rust_decimal::Decimal`.
- Produces: `fn hyperliquid_drawdown_alert(current: Decimal, peak: Decimal, threshold_pct: f64, today_wib: &str) -> Option<Alert>`.

- [ ] **Step 1: Write the failing test** — add to the `tests` module in `alerts.rs`:

```rust
    #[test]
    fn drawdown_alerts_only_at_or_beyond_threshold() {
        // Peak 1000, current 800 → 20% drawdown.
        let a = hyperliquid_drawdown_alert(dec!(800), dec!(1000), 15.0, "2026-06-18");
        let a = a.expect("alert");
        assert_eq!(a.dedup_key, "hl-drawdown:2026-06-18");
        assert!(a.message.contains("Hyperliquid"), "{}", a.message);
        assert!(a.message.contains("20"), "{}", a.message);
        // 5% drawdown under a 15% threshold → silent.
        assert!(hyperliquid_drawdown_alert(dec!(950), dec!(1000), 15.0, "2026-06-18").is_none());
        // No peak → silent.
        assert!(hyperliquid_drawdown_alert(dec!(0), dec!(0), 15.0, "2026-06-18").is_none());
    }
```

- [ ] **Step 2: Implement the helper** — add to `alerts.rs` (near `mover_alerts`):

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
    let dd_pct = ((peak - current) / peak * rust_decimal::Decimal::from(100))
        .to_f64()
        .unwrap_or(0.0);
    if dd_pct < threshold_pct {
        return None;
    }
    Some(Alert {
        dedup_key: format!("hl-drawdown:{today_wib}"),
        message: format!(
            "📉 Hyperliquid drawdown {:.1}% dari puncak (equity ${})",
            dd_pct,
            current.round_dp(2)
        ),
    })
}
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test drawdown_alerts_only_at_or_beyond_threshold`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/src/assistant/proactive/alerts.rs
git commit -m "feat(alerts): Hyperliquid drawdown alert helper"
```

---

### Task 7: Wire drawdown into config + evaluate + tick

**Files:**
- Modify: `backend/src/assistant/proactive/tick.rs` (`ProactiveConfig` field + `from_env` + the `evaluate(...)` call)
- Modify: `backend/src/assistant/proactive/alerts.rs` (`evaluate` signature + HL section)

**Interfaces:**
- Consumes: `instruments::find_by_symbol`, `prices::series`, `hyperliquid_drawdown_alert`, `setup::HL_SYMBOL`.
- Produces: `evaluate(db, mover_threshold_pct, milestone_step_idr, hl_drawdown_pct, today_wib) -> Vec<Alert>`; `ProactiveConfig.hl_drawdown_pct: f64`.

- [ ] **Step 1: Add the config field** — in `tick.rs`, add to `ProactiveConfig`:

```rust
    pub hl_drawdown_pct: f64,
```

and in `from_env`, alongside the other parsed fields:

```rust
            hl_drawdown_pct: std::env::var("HL_DRAWDOWN_PCT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(15.0),
```

- [ ] **Step 2: Extend evaluate** — change the `evaluate` signature in `alerts.rs` to:

```rust
pub async fn evaluate(
    db: &Db,
    mover_threshold_pct: f64,
    milestone_step_idr: i64,
    hl_drawdown_pct: f64,
    today_wib: &str,
) -> Vec<Alert> {
```

and add this independently-degrading section before `alerts.extend(price_alert_triggers(db).await);`:

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

- [ ] **Step 3: Update the call site** — in `tick.rs`, change the `evaluate` call to pass the new argument:

```rust
    for alert in super::alerts::evaluate(
        db,
        config.mover_alert_pct,
        config.milestone_step_idr,
        config.hl_drawdown_pct,
        &today,
    )
    .await
    {
```

- [ ] **Step 4: Verify build + existing tests**

Run: `cd backend && cargo test --lib assistant::proactive`
Expected: builds and passes (existing alert tests unchanged; new logic compiles).

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/proactive/tick.rs backend/src/assistant/proactive/alerts.rs
git commit -m "feat(alerts): wire Hyperliquid drawdown into proactive tick"
```

---

## Phase 4 — Reporting: briefing & recap lines

Deliverable: morning briefing, weekly recap, and monthly recap each show a Hyperliquid equity line.

### Task 8: Hyperliquid line in the morning briefing

**Files:**
- Modify: `backend/src/assistant/proactive/briefing.rs` (`BriefingData` field, `gather`, `render_data_block`)

**Interfaces:**
- Consumes: `service::hyperliquid::{HlEquitySummary, equity_and_change, format_hyperliquid_line}`.
- Produces: `BriefingData.hyperliquid: Option<HlEquitySummary>`.

- [ ] **Step 1: Add the field** — in `BriefingData`:

```rust
    pub hyperliquid: Option<crate::service::hyperliquid::HlEquitySummary>,
```

- [ ] **Step 2: Gather it** — in `gather`, after the net-worth/delta block (where `yesterday` is in scope), add:

```rust
    let hyperliquid = crate::service::hyperliquid::equity_and_change(db, &yesterday)
        .await
        .unwrap_or(None);
```

and include `hyperliquid` in the returned `BriefingData { ... }` literal.

- [ ] **Step 3: Render it** — in `render_data_block`, immediately after the `delta_vs_yesterday_idr` block and before the movers block, add:

```rust
    if let Some(hl) = &d.hyperliquid {
        out.push_str(&crate::service::hyperliquid::format_hyperliquid_line(hl));
        out.push('\n');
    }
```

- [ ] **Step 4: Write a render test** — add to (or create) the `tests` module in `briefing.rs`:

```rust
    #[test]
    fn render_includes_hyperliquid_line_when_present() {
        use crate::service::hyperliquid::{format_hyperliquid_line, HlEquitySummary};
        let line = format_hyperliquid_line(&HlEquitySummary {
            equity_usd: rust_decimal_macros::dec!(2500),
            change_pct: Some(-3.2),
        });
        assert_eq!(line, "Hyperliquid: $2500.00 (-3.2%)");
    }
```

- [ ] **Step 5: Run tests + build**

Run: `cd backend && cargo test --lib assistant::proactive::briefing && cargo build`
Expected: PASS and clean build (the `BriefingData` literal now sets `hyperliquid`).

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/proactive/briefing.rs
git commit -m "feat(briefing): add Hyperliquid equity line"
```

---

### Task 9: Hyperliquid line in the weekly recap

**Files:**
- Modify: `backend/src/assistant/proactive/recap.rs` (`RecapData` field, `gather`, `render_data_block`)

**Interfaces:**
- Consumes: `service::hyperliquid::{HlEquitySummary, equity_and_change, format_hyperliquid_line}`.
- Produces: `RecapData.hyperliquid: Option<HlEquitySummary>`.

- [ ] **Step 1: Add the field** — in `RecapData`:

```rust
    pub hyperliquid: Option<crate::service::hyperliquid::HlEquitySummary>,
```

- [ ] **Step 2: Gather it** — in `gather`, where `week_ago_date` is in scope, add:

```rust
    let hyperliquid = crate::service::hyperliquid::equity_and_change(db, &week_ago_date)
        .await
        .unwrap_or(None);
```

and set `hyperliquid` in the returned `RecapData { ... }` literal.

- [ ] **Step 3: Render it** — in `render_data_block`, after the net-worth/week-delta line, add:

```rust
    if let Some(hl) = &d.hyperliquid {
        out.push_str(&crate::service::hyperliquid::format_hyperliquid_line(hl));
        out.push('\n');
    }
```

(Use whatever the local output accumulator is named in this function — match the existing `push_str` calls.)

- [ ] **Step 4: Build**

Run: `cd backend && cargo build`
Expected: clean (the `RecapData` literal now sets `hyperliquid`).

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/proactive/recap.rs
git commit -m "feat(recap): add Hyperliquid equity line to weekly recap"
```

---

### Task 10: Hyperliquid line in the monthly recap

**Files:**
- Modify: `backend/src/assistant/proactive/monthly_recap.rs` (`MonthlyRecapData` field, `gather`, render)

**Interfaces:**
- Consumes: `service::hyperliquid::{HlEquitySummary, equity_and_change, format_hyperliquid_line}`.
- Produces: `MonthlyRecapData.hyperliquid: Option<HlEquitySummary>`.

- [ ] **Step 1: Add the field** — in `MonthlyRecapData`:

```rust
    pub hyperliquid: Option<crate::service::hyperliquid::HlEquitySummary>,
```

- [ ] **Step 2: Gather it** — in `gather`, where `month_label` is in scope, add (baseline = day before the month starts):

```rust
    let hyperliquid = crate::service::hyperliquid::equity_and_change(
        db,
        &format!("{month_label}-01"),
    )
    .await
    .unwrap_or(None);
```

and set `hyperliquid` in the returned `MonthlyRecapData { ... }` literal.

- [ ] **Step 3: Render it** — in this module's render function, after the net-worth-change line, add:

```rust
    if let Some(hl) = &d.hyperliquid {
        out.push_str(&crate::service::hyperliquid::format_hyperliquid_line(hl));
        out.push('\n');
    }
```

- [ ] **Step 4: Build + commit**

Run: `cd backend && cargo build`
Expected: clean.

```bash
git add backend/src/assistant/proactive/monthly_recap.rs
git commit -m "feat(recap): add Hyperliquid equity line to monthly recap"
```

---

## Phase 5 — Analytics UI

Deliverable: a dedicated equity-curve endpoint and a Hyperliquid section in the frontend.

### Task 11: GET /portfolio/hyperliquid endpoint

**Files:**
- Modify: `backend/src/service/hyperliquid.rs` (add `HyperliquidView` + `build_hyperliquid_view`)
- Modify: `backend/src/api/portfolio.rs` (add `hyperliquid` handler)
- Modify: `backend/src/api/mod.rs` (register the route in the `protected` router)

**Interfaces:**
- Consumes: `domain::performance::{compute, PerfMetrics}`, `prices::series`, `accounts::find_by_name`, `transactions::list_all`, `domain::models::TxnType`.
- Produces: `service::hyperliquid::HyperliquidView { points: Vec<HlPoint>, metrics: PerfMetrics, current_value_usd: String, insufficient_data: bool }`; `HlPoint { date: String, cum_return: f64, nav: f64 }`; `build_hyperliquid_view(db) -> anyhow::Result<HyperliquidView>`; route `GET /portfolio/hyperliquid`.

- [ ] **Step 1: Write the failing test** — add to the `tests` module in `service/hyperliquid.rs`:

```rust
    #[tokio::test]
    async fn build_view_produces_twr_points_from_equity_series() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::setup::ensure_hyperliquid_account(&db, "0x").await.unwrap();
        let ins = crate::repo::instruments::find_by_symbol(&db, crate::setup::HL_SYMBOL)
            .await.unwrap().unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1000), "USD", "hyperliquid", "2026-06-01").await.unwrap();
        crate::repo::prices::upsert_latest(&db, ins.id, dec!(1100), "USD", "hyperliquid", "2026-06-02").await.unwrap();
        let view = build_hyperliquid_view(&db).await.unwrap();
        assert!(!view.insufficient_data);
        assert_eq!(view.current_value_usd, "1100");
        assert!((view.metrics.total_return - 0.10).abs() < 1e-9);
    }
```

- [ ] **Step 2: Implement the view builder** — add to `service/hyperliquid.rs`:

```rust
use crate::domain::models::TxnType;
use crate::domain::performance::{compute, PerfMetrics};
use crate::setup::HL_ACCOUNT_NAME;
use chrono::NaiveDate;
use serde::Serialize;

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
    pub insufficient_data: bool,
}

const EMPTY_METRICS: PerfMetrics = PerfMetrics {
    total_return: 0.0,
    annualized: None,
    max_drawdown: 0.0,
    volatility: 0.0,
};

/// TWR equity curve for the Hyperliquid account, in USD. NAV series is the
/// equity price quotes; flows are USD deposits/withdrawals on the HL account.
pub async fn build_hyperliquid_view(db: &Db) -> anyhow::Result<HyperliquidView> {
    let instrument = match crate::repo::instruments::find_by_symbol(db, HL_SYMBOL).await? {
        Some(i) => i,
        None => {
            return Ok(HyperliquidView {
                points: Vec::new(),
                metrics: EMPTY_METRICS,
                current_value_usd: "0".into(),
                insufficient_data: true,
            })
        }
    };
    let series = crate::repo::prices::series(db, instrument.id).await?;
    let mut navs: Vec<(NaiveDate, f64)> = Vec::new();
    for (as_of, price) in &series {
        if let (Ok(date), Some(v)) = (NaiveDate::parse_from_str(as_of, "%Y-%m-%d"), price.to_f64()) {
            navs.push((date, v));
        }
    }
    let mut flows: Vec<(NaiveDate, f64)> = Vec::new();
    if let Some(account) = crate::repo::accounts::find_by_name(db, HL_ACCOUNT_NAME).await? {
        for t in crate::repo::transactions::list_all(db).await? {
            if t.account_id != account.id {
                continue;
            }
            let sign = match t.txn_type {
                TxnType::Deposit => 1.0,
                TxnType::Withdrawal => -1.0,
                _ => continue,
            };
            let value = (t.quantity * t.price_native * t.fx_to_usd).to_f64().unwrap_or(0.0) * sign;
            flows.push((t.executed_at.date_naive(), value));
        }
    }
    let (points, metrics) = compute(&navs, &flows);
    let current_value_usd = series.last().map(|(_, p)| p.to_string()).unwrap_or_else(|| "0".into());
    Ok(HyperliquidView {
        points: points
            .into_iter()
            .map(|p| HlPoint {
                date: p.date.format("%Y-%m-%d").to_string(),
                cum_return: p.cum_return,
                nav: p.nav,
            })
            .collect(),
        metrics,
        current_value_usd,
        insufficient_data: navs.len() < 2,
    })
}
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test build_view_produces_twr_points_from_equity_series`
Expected: PASS.

- [ ] **Step 4: Add the handler** — in `backend/src/api/portfolio.rs`:

```rust
pub async fn hyperliquid(
    State(s): State<AppState>,
) -> Result<Json<crate::service::hyperliquid::HyperliquidView>, AppError> {
    Ok(Json(
        crate::service::hyperliquid::build_hyperliquid_view(&s.db)
            .await
            .map_err(AppError::Other)?,
    ))
}
```

- [ ] **Step 5: Register the route** — in `backend/src/api/mod.rs`, in the `protected` router after the other `/portfolio/*` routes:

```rust
        .route("/portfolio/hyperliquid", get(portfolio::hyperliquid))
```

- [ ] **Step 6: Build + commit**

Run: `cd backend && cargo build`
Expected: clean.

```bash
git add backend/src/service/hyperliquid.rs backend/src/api/portfolio.rs backend/src/api/mod.rs
git commit -m "feat(api): add /portfolio/hyperliquid equity-curve endpoint"
```

---

### Task 12: Frontend schema + query hook

**Files:**
- Modify: `frontend/src/api/schemas.ts` (add `HyperliquidViewSchema`)
- Modify: `frontend/src/api/hooks.ts` (add `useHyperliquid`)

**Interfaces:**
- Produces: `HyperliquidView` type; `useHyperliquid()` query hook hitting `/portfolio/hyperliquid`.

- [ ] **Step 1: Add the schema** — in `frontend/src/api/schemas.ts`:

```typescript
export const HyperliquidViewSchema = z.object({
  points: z.array(z.object({ date: z.string(), cum_return: z.number(), nav: z.number() })),
  metrics: z.object({
    total_return: z.number(),
    annualized: z.number().nullable(),
    max_drawdown: z.number(),
    volatility: z.number(),
  }),
  current_value_usd: z.string(),
  insufficient_data: z.boolean(),
});
export type HyperliquidView = z.infer<typeof HyperliquidViewSchema>;
```

- [ ] **Step 2: Add the hook** — in `frontend/src/api/hooks.ts`:

```typescript
export const useHyperliquid = () =>
  useQuery({
    queryKey: ["hyperliquid"],
    queryFn: () => api.get("/portfolio/hyperliquid", HyperliquidViewSchema),
  });
```

(Import `HyperliquidViewSchema` from `./schemas` following the file's existing import style.)

- [ ] **Step 3: Typecheck + commit**

Run: `cd frontend && npm run build`
Expected: type-checks and builds.

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts
git commit -m "feat(web): Hyperliquid view schema + query hook"
```

---

### Task 13: Dashboard Hyperliquid card

**Files:**
- Create: `frontend/src/components/HyperliquidCard.tsx`
- Modify: `frontend/src/pages/DashboardPage.tsx` (render the card)

**Interfaces:**
- Consumes: `useHyperliquid`, Recharts `AreaChart`.
- Produces: `<HyperliquidCard />`.

- [ ] **Step 1: Create the card** — `frontend/src/components/HyperliquidCard.tsx` (mirror the `MoversCard` card/loading/empty structure and the PerformancePage chart styling):

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

- [ ] **Step 2: Render it on the dashboard** — in `DashboardPage.tsx`, import `HyperliquidCard` and add it to the card grid (next to `MoversCard`):

```tsx
<HyperliquidCard />
```

- [ ] **Step 3: Typecheck + commit**

Run: `cd frontend && npm run build`
Expected: builds.

```bash
git add frontend/src/components/HyperliquidCard.tsx frontend/src/pages/DashboardPage.tsx
git commit -m "feat(web): Hyperliquid equity card on dashboard"
```

---

### Task 14: Hyperliquid panel on the Performance page

**Files:**
- Modify: `frontend/src/pages/PerformancePage.tsx` (add a Hyperliquid section reusing the same chart)
- Create: `frontend/src/pages/PerformancePage.hyperliquid.test.tsx` (component test)

**Interfaces:**
- Consumes: `useHyperliquid`.

- [ ] **Step 1: Add the panel** — in `PerformancePage.tsx`, render the `HyperliquidCard` (or an inline copy of its chart) below the main performance chart:

```tsx
import { HyperliquidCard } from "@/components/HyperliquidCard";
// ...inside the returned JSX, after the main performance chart block:
<HyperliquidCard />
```

- [ ] **Step 2: Write a component test** — `frontend/src/pages/PerformancePage.hyperliquid.test.tsx` (use the MSW server + render helper pattern from `src/App.test.tsx` / `src/test/`):

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "@/test/server";
import { HyperliquidCard } from "@/components/HyperliquidCard";

function renderCard() {
  localStorage.setItem("pt-auth-token", "test-token");
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <HyperliquidCard />
    </QueryClientProvider>,
  );
}

test("shows current equity from the API", async () => {
  server.use(
    http.get("*/portfolio/hyperliquid", () =>
      HttpResponse.json({
        points: [
          { date: "2026-06-01", cum_return: 0, nav: 1000 },
          { date: "2026-06-02", cum_return: 0.1, nav: 1100 },
        ],
        metrics: { total_return: 0.1, annualized: null, max_drawdown: -0.05, volatility: 0.2 },
        current_value_usd: "1100",
        insufficient_data: false,
      }),
    ),
  );
  renderCard();
  await waitFor(() => expect(screen.getByText("$1100")).toBeInTheDocument());
});
```

(If the MSW handler import path differs, match the existing usage in the repo's other component tests.)

- [ ] **Step 3: Run the test**

Run: `cd frontend && npm test -- PerformancePage.hyperliquid`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/pages/PerformancePage.tsx frontend/src/pages/PerformancePage.hyperliquid.test.tsx
git commit -m "feat(web): Hyperliquid panel on performance page"
```

---

## Phase 6 — Deposit/withdrawal flow connector

Deliverable: USDC transfers in/out of the Hyperliquid account are recorded so TWR excludes fund movements.

### Task 15: HyperliquidConnector

**Files:**
- Create: `backend/src/connectors/hyperliquid.rs`
- Modify: `backend/src/connectors/mod.rs` (add `pub mod hyperliquid;`)

**Interfaces:**
- Consumes: `connectors::{Connector, ExternalTxn, SyncBatch, ConnectorError}`.
- Produces: `connectors::hyperliquid::HyperliquidConnector::new(wallet: String, network: String) -> Self`; `Connector` impl; `fn parse_ledger(body: &serde_json::Value, wallet: &str) -> Result<Vec<ExternalTxn>, ConnectorError>`.

- [ ] **Step 1: Write the failing test** — append to `backend/src/connectors/hyperliquid.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usdc_deposit_and_withdrawal() {
        // Shape: userNonFundingLedgerUpdates → [{ time, hash, delta: { type, usdc } }]
        let body = serde_json::json!([
            { "time": 1700000000000_i64, "hash": "0xa",
              "delta": { "type": "deposit", "usdc": "500.0" } },
            { "time": 1700000100000_i64, "hash": "0xb",
              "delta": { "type": "withdraw", "usdc": "200.0" } },
            { "time": 1700000200000_i64, "hash": "0xc",
              "delta": { "type": "liquidation", "usdc": "1.0" } }
        ]);
        let out = parse_ledger(&body, "0xme").unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].quantity, "500.0");
        assert_eq!(out[0].symbol, "USDC");
        assert_eq!(out[0].currency, "USD");
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
    wallet: String,
    base: String,
    client: reqwest::Client,
}

impl HyperliquidConnector {
    pub fn new(wallet: String, network: String) -> Self {
        let base = if network == "testnet" {
            "https://api.hyperliquid-testnet.xyz"
        } else {
            "https://api.hyperliquid.xyz"
        }
        .to_string();
        Self { wallet, base, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl Connector for HyperliquidConnector {
    async fn fetch_new(&self, _cursor: Option<&str>) -> Result<SyncBatch, ConnectorError> {
        let url = format!("{}/info", self.base);
        let body = serde_json::json!({
            "type": "userNonFundingLedgerUpdates",
            "user": self.wallet,
        });
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ConnectorError::Http(e.to_string()))?;
        let json: serde_json::Value =
            resp.json().await.map_err(|e| ConnectorError::Parse(e.to_string()))?;
        let txns = parse_ledger(&json, &self.wallet)?;
        Ok(SyncBatch { txns, next_cursor: None })
    }
}

/// Map non-funding ledger updates to deposit/withdrawal ExternalTxns (USDC only).
pub fn parse_ledger(body: &serde_json::Value, _wallet: &str) -> Result<Vec<ExternalTxn>, ConnectorError> {
    let rows = body
        .as_array()
        .ok_or_else(|| ConnectorError::Parse("expected ledger array".into()))?;
    let mut out = Vec::new();
    for row in rows {
        let delta = match row.get("delta") {
            Some(d) => d,
            None => continue,
        };
        let kind = match delta.get("type").and_then(|v| v.as_str()) {
            Some("deposit") => "deposit",
            Some("withdraw") => "withdrawal",
            _ => continue,
        };
        let usdc = match delta.get("usdc").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => continue,
        };
        let time_ms = row.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
        let occurred_at = Utc
            .timestamp_millis_opt(time_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339();
        let hash = row.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
        out.push(ExternalTxn {
            external_id: format!("{hash}:{kind}"),
            occurred_at,
            kind: kind.to_string(),
            symbol: "USDC".into(),
            quantity: usdc,
            fee: None,
            currency: "USD".into(),
        });
    }
    Ok(out)
}
```

Add to `backend/src/connectors/mod.rs`:

```rust
pub mod hyperliquid;
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test parses_usdc_deposit_and_withdrawal`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/src/connectors/hyperliquid.rs backend/src/connectors/mod.rs
git commit -m "feat(connectors): Hyperliquid USDC ledger connector"
```

---

### Task 16: Register the connector in the factory

**Files:**
- Modify: `backend/src/connectors/factory.rs` (add the `"hyperliquid"` arm)

**Interfaces:**
- Consumes: `HyperliquidConnector::new`.

- [ ] **Step 1: Write the failing test** — add to (or create) the `tests` module in `factory.rs`:

```rust
    #[test]
    fn builds_hyperliquid_connector_from_config() {
        let row = ConnectorRow {
            id: 1,
            account_id: 1,
            kind: "hyperliquid".into(),
            label: "HL".into(),
            config_json: r#"{"wallet":"0xabc","network":"testnet"}"#.into(),
            cursor: None,
        };
        assert!(build(&row).is_ok());
    }
```

(Match the exact `ConnectorRow` field set from the existing factory/tests; omit `cursor` if the struct has none.)

- [ ] **Step 2: Add the factory arm** — in `factory.rs`, in the `match row.kind.as_str()` before the `other =>` arm:

```rust
        "hyperliquid" => {
            let wallet = cfg
                .get("wallet")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing wallet".into()))?
                .to_string();
            let network = cfg
                .get("network")
                .and_then(|v| v.as_str())
                .unwrap_or("mainnet")
                .to_string();
            Ok(Box::new(crate::connectors::hyperliquid::HyperliquidConnector::new(wallet, network)))
        }
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test builds_hyperliquid_connector_from_config`
Expected: PASS.

- [ ] **Step 4: Final full-suite check + commit**

Run: `cd backend && cargo test && cd ../frontend && npm test && npm run build`
Expected: all pass.

```bash
git add backend/src/connectors/factory.rs
git commit -m "feat(connectors): register hyperliquid connector kind"
```

---

## Self-Review

**Spec coverage:**
- Account-equity representation (synthetic 1-unit `HL-EQUITY` instrument priced at equity) → Tasks 1–3. ✓
- Read-only pull via pricing provider on existing scheduler → Tasks 1–2. ✓
- Bonus stale-price liveness monitoring → automatic (source ≠ `manual`); no code needed. ✓
- Drawdown alert reusing proactive-alert + dedup machinery → Tasks 6–7. ✓
- Briefing + weekly + monthly recap lines → Tasks 8–10. ✓
- Frontend Hyperliquid section (card + performance panel) + dedicated endpoint → Tasks 11–14. ✓
- Deposit/withdrawal flow connector for TWR accuracy → Tasks 15–16. ✓
- Config (`HYPERLIQUID_WALLET`, `HYPERLIQUID_NETWORK`, `HL_DRAWDOWN_PCT`) → Tasks 2, 3, 7. ✓
- Per-trade detail stays in the bot; no `agent-hyperliquid` changes → respected throughout. ✓

**Type consistency:** `HL_SYMBOL`/`HL_ACCOUNT_NAME` defined once in `setup.rs` and reused; `HlEquitySummary`, `HyperliquidView`, `HlPoint` consistent across service/api/web; `equity_and_change`/`format_hyperliquid_line` signatures stable across Tasks 8–10; `evaluate` signature updated at both definition and call site (Task 7).

**Notes for the implementer (verify against live code, adjust the SQL/literal, keep the test):**
- `prices::series` column name (`price_native`) — confirm against the existing `last_two`/`latest` SELECT in the same file.
- Each `*Data` struct literal (`BriefingData`, `RecapData`, `MonthlyRecapData`) must set the new `hyperliquid` field — the build step catches omissions.
- `ConnectorRow` field set in Task 16's test — match the real struct.
- The render accumulator variable name in each recap module — match the surrounding `push_str` calls.
