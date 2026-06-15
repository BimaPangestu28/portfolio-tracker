# Money Proactivity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface cashflow + insights in chat, add a monthly recap, and add per-instrument price alerts.

**Architecture:** Two read-only chat tools over existing services (cashflow, insights, portfolio); a monthly proactive recap mirroring `recap.rs`; a `price_alerts` table + repo + tools + tick evaluation reusing the existing alert claim-then-send loop.

**Tech Stack:** Rust, sqlx (SQLite), rust_decimal, chrono, serde_json. Tests: `cargo test <filter>` from `backend/` (BIN crate — never `cargo test --lib`, never `cargo fmt`).

---

## Conventions
- Paths relative to `backend/`. Run cargo from `backend/`. Commit from repo root.
- End commit bodies with: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- IDR formatting: reuse `crate::service::chat::group_id(&decimal)` (used in briefing).
- Resolve instruments by symbol/name via `crate::repo::instruments::list(db)` (inspect the row fields — likely `id`, `symbol`/`name`).

---

## Task 1: `cashflow_summary` tool

**Files:** Modify `src/assistant/tools.rs` (schema + registration test), `src/assistant/dispatcher.rs` (arm + handler + test).

- [ ] **Step 1: Inspect inputs.** Read `src/repo/cashflow.rs` (`CashflowRow` fields, `list_for_month(db, "YYYY-MM")`), `src/repo/categories.rs` (`list`), `src/repo/invoices.rs` (`InvoiceRow.issue_date`, `.total` string), and `src/service/cashflow.rs` (`CfRow{direction,amount,category_id}`, `CatRow{id,name,kind,budget}`, `month_summary(month, &[CfRow], &[CatRow]) -> MonthSummary{month,total_in,total_out,net,categories:Vec<CategoryLine{name,kind,actual,...}>}`).

- [ ] **Step 2: Write the failing test** (in `dispatcher.rs` tests, with `mem_db()`):
```rust
    #[tokio::test]
    async fn cashflow_summary_reports_in_out_net() {
        let db = mem_db().await;
        // Insert a couple cashflow rows for the current WIB month via the repo.
        let month = chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m").to_string();
        // Use repo::cashflow::create with NewCashflow (inspect its fields) to add
        // one 'in' 1_000_000 and one 'out' 400_000 dated in `month`.
        // ... construct NewCashflow rows ...
        let out = dispatch(&db, "cashflow_summary", &serde_json::json!({})).await.unwrap();
        assert!(out.to_lowercase().contains("masuk"), "{out}");
        assert!(out.to_lowercase().contains("net"), "{out}");
    }
```
(Construct the `NewCashflow` rows by reading `repo/cashflow.rs`; date them within the current WIB month so the default-month path includes them.)

- [ ] **Step 3: Handler + arm.** Add the dispatch arm `"cashflow_summary" => cashflow_summary(db, input).await,` and:
```rust
async fn cashflow_summary(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let month = match str_arg(input, "month") {
        Some(m) => m.to_string(),
        None => chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m").to_string(),
    };
    // Map repo rows → service CfRow/CatRow (inspect field names), then summarize.
    let rows = crate::repo::cashflow::list_for_month(db, &month).await.map_err(|e| format!("db error: {e}"))?;
    let cats = crate::repo::categories::list(db).await.map_err(|e| format!("db error: {e}"))?;
    let cf_rows: Vec<crate::service::cashflow::CfRow> = rows.iter().map(|r| /* map fields */).collect();
    let cat_rows: Vec<crate::service::cashflow::CatRow> = cats.iter().map(|c| /* map fields */).collect();
    let summary = crate::service::cashflow::month_summary(&month, &cf_rows, &cat_rows);
    // Freelance invoiced this month (separate line; not summed into income).
    let invoiced = crate::repo::invoices::list_all(db).await.unwrap_or_default()
        .iter()
        .filter(|i| i.issue_date.starts_with(&month))
        .filter_map(|i| i.total.parse::<rust_decimal::Decimal>().ok())
        .sum::<rust_decimal::Decimal>();
    use crate::service::chat::group_id;
    let mut out = format!(
        "Bulan {}: masuk Rp {}, kepake Rp {}, net Rp {}\n",
        summary.month,
        group_id(&summary.total_in.round_dp(0)),
        group_id(&summary.total_out.round_dp(0)),
        group_id(&summary.net.round_dp(0)),
    );
    let mut top: Vec<_> = summary.categories.iter().filter(|c| c.kind == "expense").collect();
    top.sort_by(|a, b| b.actual.cmp(&a.actual));
    for c in top.into_iter().take(3) {
        out.push_str(&format!("- {}: Rp {}\n", c.name, group_id(&c.actual.round_dp(0))));
    }
    if invoiced > rust_decimal::Decimal::ZERO {
        out.push_str(&format!("Freelance diinvoice: Rp {}\n", group_id(&invoiced.round_dp(0))));
    }
    Ok(out)
}
```
(`repo::invoices::list_all` may not exist — if only `max_seq_for_prefix`/`insert` exist, add a `list_all(db) -> Vec<InvoiceRow>` to `repo/invoices.rs` mirroring `cashflow::list_all`, or a `list_for_month`. Inspect and add the minimal query needed.)

- [ ] **Step 4: Schema + registration.** Append to `definitions()`:
```rust
        {
            "name": "cashflow_summary",
            "description": "Monthly cashflow: money in, money out, net, top expense categories, and freelance invoiced that month. Use for 'bulan ini masuk/kepake/net berapa?'.",
            "input_schema": { "type": "object", "properties": { "month": { "type": "string", "description": "YYYY-MM; default current month" } } }
        }
```
Append `"cashflow_summary"` to the `defines_all_tools_with_schemas` vector (after the actual last name).

- [ ] **Step 5:** `cargo test cashflow_summary tools::tests` + `cargo build`. Commit:
```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs backend/src/repo/invoices.rs
git commit -m "feat(assistant): cashflow_summary tool (income/expense/net + freelance)"
```

---

## Task 2: `portfolio_insights` tool

**Files:** Modify `src/assistant/tools.rs`, `src/assistant/dispatcher.rs`.

- [ ] **Step 1: Inspect `src/service/insights.rs`** signatures: `savings_rate(income, expense)`, `concentration(&[(String,Decimal)], net_worth) -> Option<Concentration{symbol?,pct?}>` (inspect `Concentration` fields), `dividend_ttm(&[Decimal])`, `yield_pct(dividend_ttm, net_worth)`. And `service/portfolio::build_summary` (`net_worth_idr`, `positions` with symbol + market value — inspect `Position` fields).

- [ ] **Step 2: Failing test:**
```rust
    #[tokio::test]
    async fn portfolio_insights_runs_on_empty_db() {
        let db = mem_db().await;
        let out = dispatch(&db, "portfolio_insights", &serde_json::json!({})).await.unwrap();
        // On an empty portfolio it still returns a non-error string (lines omitted as needed).
        assert!(!out.is_empty(), "{out}");
    }
```

- [ ] **Step 3: Handler + arm** `"portfolio_insights" => portfolio_insights(db).await,`:
```rust
async fn portfolio_insights(db: &Db) -> Result<String, String> {
    let summary = crate::service::portfolio::build_summary(db).await.map_err(|e| format!("db error: {e}"))?;
    let net = summary.net_worth_idr;
    use crate::service::chat::group_id;
    let mut out = format!("Net worth: Rp {}\n", group_id(&net.round_dp(0)));
    // Concentration (top position % of net worth).
    let positions: Vec<(String, rust_decimal::Decimal)> = summary.positions.iter()
        .map(|p| (/* symbol */, /* market value idr */)).collect();
    if let Some(c) = crate::service::insights::concentration(&positions, net) {
        out.push_str(&format!("Konsentrasi terbesar: {} ({})\n", /* c.symbol */, /* c.pct formatted */));
    }
    // Savings rate (current WIB month).
    let month = chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m").to_string();
    if let Ok(rows) = crate::repo::cashflow::list_for_month(db, &month).await {
        let (income, expense) = /* sum in/out from rows */;
        if income > rust_decimal::Decimal::ZERO {
            let rate = crate::service::insights::savings_rate(income, expense);
            out.push_str(&format!("Savings rate bulan ini: {}%\n", rate.round_dp(0)));
        }
    }
    Ok(out)
}
```
Fill the `/* ... */` by inspecting the structs. Omit the dividend-yield and runway lines in v1 unless the inputs are cleanly available (note in the commit if omitted). Keep each line guarded so an empty portfolio yields a short but valid reply.

- [ ] **Step 4: Schema + registration:**
```rust
        {
            "name": "portfolio_insights",
            "description": "Portfolio health: net worth, biggest-position concentration, and this month's savings rate. Use for insight questions about the portfolio.",
            "input_schema": { "type": "object", "properties": {} }
        }
```
Append `"portfolio_insights"` to the registration vector.

- [ ] **Step 5:** `cargo test portfolio_insights tools::tests` + build. Commit:
```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): portfolio_insights tool"
```

---

## Task 3: Monthly recap (proactive)

**Files:** Create `src/assistant/proactive/monthly_recap.rs`; modify `compose.rs`, `proactive/mod.rs`, `tick.rs`.

- [ ] **Step 1: Inspect `src/assistant/proactive/recap.rs`** (its `gather`/`render_data_block`/`run` shape and `RecapData`) and `tick.rs` (`recap_due`, `ProactiveConfig`, `run_once`, `GRACE_HOURS`, the `wib()` test helper).

- [ ] **Step 2: `MONTHLY_RECAP_SYSTEM` in `compose.rs`** (after `REVIEW_SYSTEM`):
```rust
pub const MONTHLY_RECAP_SYSTEM: &str = "You write a short monthly recap in Indonesian for the \
app owner, delivered over Telegram on the 1st. Use ONLY the data block provided — copy every \
number exactly, never invent anything. Plain text only: no Markdown, no headers, no **bold**, no \
tables. At most 15 short lines; use emoji sparingly. Structure: one opening line naming the month; \
productivity (todos done); the month's finances (net worth change, money in/out, freelance \
invoiced); one short grounded closing line. Skip any section whose data is empty.";
```
Add `MONTHLY_RECAP_SYSTEM` to the `prompts_demand_exact_numbers_and_plain_text` test loop.

- [ ] **Step 3: `monthly_recap.rs`** — mirror `recap.rs`: `gather(db, now_utc)` building a data block for the **prior** month (todos done last month via `todos::completed_since`(prior-month start), `cashflow::list_for_month(prior_month)` → `month_summary`, freelance invoiced that month, net-worth change from `snapshots`), `render_data_block`, and `run(db, client, chat_id)` calling `compose(MONTHLY_RECAP_SYSTEM, &block, "📅 Recap bulanan (mode ringkas)")` then `client.send_message`. Add `pub mod monthly_recap;` to `proactive/mod.rs`. Add focused unit tests for the render (sections present) and a pure prior-month helper.

- [ ] **Step 4: Schedule in `tick.rs`:**
  - `ProactiveConfig` += `monthly_recap_hour: Option<u32>`; `from_env` += `parse_hour(std::env::var("MONTHLY_RECAP_HOUR_WIB").ok(), 8)`; add the field to the test `ProactiveConfig{..}` literal.
  - `monthly_recap_due(now_wib, hour) -> Option<String>`: due when `now_wib.day() == 1` and `hour <= h < hour+GRACE_HOURS`; dedup key = the prior month: `format!("monthly_recap:{}", (first_of_this_month - 1 day).format("%Y-%m"))`. Test: due on the 1st in-window only; off disables.
  - `run_once`: claim-then-send block (kind `"monthly_recap"`) calling `super::monthly_recap::run`.
  - Extend `config_defaults_are_sane` with `assert_eq!(config.monthly_recap_hour, Some(8))`.

- [ ] **Step 5:** `cargo test monthly_recap tick::tests compose::tests` + build. Commit:
```bash
git add backend/src/assistant/proactive/monthly_recap.rs backend/src/assistant/proactive/compose.rs backend/src/assistant/proactive/mod.rs backend/src/assistant/proactive/tick.rs
git commit -m "feat(proactive): monthly recap"
```

---

## Task 4: price_alerts table + repo + trigger predicate

**Files:** Create `migrations/0020_price_alerts.sql`, `src/repo/price_alerts.rs`; modify `src/repo/mod.rs`.

- [ ] **Step 1: Verify `migrations/0020_*.sql` is free** (`ls`); if taken, STOP + report. Create:
```sql
-- Fase 6: user-defined per-instrument price alerts (fire once).
CREATE TABLE price_alerts (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  instrument_id INTEGER NOT NULL REFERENCES instruments(id),
  target_price TEXT NOT NULL,
  direction TEXT NOT NULL CHECK (direction IN ('above', 'below')),
  status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'triggered', 'cancelled')),
  created_at TEXT NOT NULL,
  triggered_at TEXT
);
CREATE INDEX idx_price_alerts_active ON price_alerts (status, instrument_id);
```

- [ ] **Step 2: `repo/price_alerts.rs`** — `pub mod price_alerts;` in `repo/mod.rs`. Implement:
```rust
//! Per-instrument price alerts (migration 0020).
use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct PriceAlertRow {
    pub id: i64,
    pub instrument_id: i64,
    pub target_price: String,
    pub direction: String,
    pub status: String,
    pub created_at: String,
    pub triggered_at: Option<String>,
}

pub async fn create(db: &Db, instrument_id: i64, target_price: &str, direction: &str) -> anyhow::Result<PriceAlertRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO price_alerts (instrument_id, target_price, direction, status, created_at) VALUES (?, ?, ?, 'active', ?)",
    ).bind(instrument_id).bind(target_price).bind(direction).bind(&now)
        .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<PriceAlertRow> {
    Ok(sqlx::query_as::<_, PriceAlertRow>("SELECT * FROM price_alerts WHERE id = ?").bind(id).fetch_one(db).await?)
}

pub async fn list_active(db: &Db) -> anyhow::Result<Vec<PriceAlertRow>> {
    Ok(sqlx::query_as::<_, PriceAlertRow>("SELECT * FROM price_alerts WHERE status = 'active' ORDER BY id").fetch_all(db).await?)
}

pub async fn mark_triggered(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE price_alerts SET status = 'triggered', triggered_at = ? WHERE id = ? AND status = 'active'")
        .bind(&now).bind(id).execute(db).await?;
    Ok(())
}

pub async fn cancel(db: &Db, id: i64) -> anyhow::Result<bool> {
    let r = sqlx::query("UPDATE price_alerts SET status = 'cancelled' WHERE id = ? AND status = 'active'")
        .bind(id).execute(db).await?;
    Ok(r.rows_affected() > 0)
}

/// True when `price` has reached the target in the alert's direction.
pub fn is_triggered(direction: &str, target: rust_decimal::Decimal, price: rust_decimal::Decimal) -> bool {
    match direction {
        "below" => price <= target,
        "above" => price >= target,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[test]
    fn trigger_predicate() {
        assert!(is_triggered("below", dec!(9000), dec!(8999)));
        assert!(!is_triggered("below", dec!(9000), dec!(9001)));
        assert!(is_triggered("above", dec!(11000), dec!(11000)));
        assert!(!is_triggered("above", dec!(11000), dec!(10999)));
    }

    #[tokio::test]
    async fn create_list_trigger_cancel() {
        let db = mem_db().await;
        // instruments has a FK; insert a minimal instrument row first if the FK is enforced.
        // (Inspect repo/instruments.rs for a create/insert helper, or insert directly.)
        let a = create(&db, 1, "9000", "below").await.unwrap();
        assert_eq!(list_active(&db).await.unwrap().len(), 1);
        mark_triggered(&db, a.id).await.unwrap();
        assert!(list_active(&db).await.unwrap().is_empty());
        let b = create(&db, 1, "11000", "above").await.unwrap();
        assert!(cancel(&db, b.id).await.unwrap());
        assert!(list_active(&db).await.unwrap().is_empty());
    }
}
```
> If SQLite FK enforcement makes `create(&db, 1, …)` fail (no instrument id 1), either insert a minimal instruments row in the test first (inspect `repo/instruments.rs`), or note FKs aren't enforced in the test DB. Make the test pass.

- [ ] **Step 3:** `cargo test price_alerts::tests` + build. Commit:
```bash
git add backend/migrations/0020_price_alerts.sql backend/src/repo/price_alerts.rs backend/src/repo/mod.rs
git commit -m "feat(price-alerts): table + repo + trigger predicate (migration 0020)"
```

---

## Task 5: price-alert tools + tick evaluation

**Files:** Modify `src/assistant/tools.rs`, `src/assistant/dispatcher.rs`, `src/assistant/proactive/alerts.rs` (+ wherever `evaluate` is called in `tick.rs` if needed).

- [ ] **Step 1: Failing dispatcher tests** for `set_price_alert` (target path + percent path), `list_price_alerts`, `cancel_price_alert`. For instrument resolution + current price, insert a minimal instrument + a `prices::upsert_latest` row in the test (inspect `repo/instruments.rs` + `repo/prices.rs`). Assert set with `target` stores a row; set with `percent` computes target from the latest price; list shows it; cancel removes it.

- [ ] **Step 2: Handlers** (`dispatcher.rs`):
  - `set_price_alert`: resolve `instrument` (by symbol/name via `instruments::list`); determine target: if `target` provided use it; else if `percent` provided, read `prices::latest(instrument_id)` (error if none), compute `target = current * (1 + percent/100)` for `above` or `current * (1 - percent/100)` for `below` — infer direction from the `direction` arg, or from the sign of percent / "turun"→below "naik"→above as passed by the model. Store via `price_alerts::create`. Reply "alert dipasang: {symbol} {direction} Rp {target}".
  - `list_price_alerts`: `list_active` joined with instrument symbol + current price; "(belum ada alert)" when empty.
  - `cancel_price_alert { id }`: `cancel`; report.

- [ ] **Step 3: Dispatch arms** for the three tool names.

- [ ] **Step 4: Tick evaluation.** In `proactive/alerts.rs`, add:
```rust
pub async fn price_alert_triggers(db: &Db) -> Vec<Alert> {
    let mut out = Vec::new();
    let alerts = match crate::repo::price_alerts::list_active(db).await {
        Ok(a) => a, Err(e) => { tracing::warn!("alerts: price_alerts unavailable: {e:#}"); return out; }
    };
    for a in alerts {
        let Ok(target) = a.target_price.parse::<rust_decimal::Decimal>() else { continue };
        let Ok(Some(latest)) = crate::repo::prices::latest(db, a.instrument_id).await else { continue };
        if crate::repo::price_alerts::is_triggered(&a.direction, target, latest.price) {
            // symbol lookup best-effort
            let symbol = crate::repo::instruments::get(db, a.instrument_id).await
                .ok().map(|i| /* i.symbol */).unwrap_or_default();
            out.push(Alert {
                dedup_key: format!("price_alert:{}", a.id),
                message: format!("🔔 {symbol} {} Rp {}", a.direction, crate::service::chat::group_id(&latest.price.round_dp(0))),
            });
            let _ = crate::repo::price_alerts::mark_triggered(db, a.id).await;
        }
    }
    out
}
```
Call `price_alert_triggers(db)` inside `evaluate` (extend it to push these) so the existing `run_once` alert claim-loop sends them. (Inspect `instruments::get` / field names; adjust.)

- [ ] **Step 5: Schemas + registration** for `set_price_alert` (instrument required; target/percent/direction optional), `read`... `list_price_alerts` ({}), `cancel_price_alert` ({id required}). Append the three names to the registration vector.

- [ ] **Step 6:** `cargo test price_alert set_price_alert tools::tests alerts` + build. Commit:
```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs backend/src/assistant/proactive/alerts.rs
git commit -m "feat(assistant): price-alert tools + tick evaluation"
```

---

## Task 6: Prompt guidance + final verification

**Files:** Modify `src/assistant/agent.rs`.

- [ ] **Step 1: Append to `SYSTEM`** (before the closing `";`, `\`-joined):
```
 You can answer money questions: 'bulan ini masuk/kepake/net berapa?' → cashflow_summary; \
portfolio insight questions (konsentrasi, savings rate, net worth) → portfolio_insights. \
For price alerts ('kabarin kalau BBCA turun 5%' or 'di harga 9000'), call set_price_alert: pass \
the instrument and either target (an absolute price) or percent + direction (turun→below, naik→above); \
for a percent the alert is computed from the current price. list_price_alerts shows active alerts; \
cancel_price_alert cancels one by id.
```

- [ ] **Step 2:** `cargo build` + `cargo test`. Commit:
```bash
git add backend/src/assistant/agent.rs
git commit -m "feat(assistant): money + price-alert prompt guidance"
```

- [ ] **Step 3: Final** — `cargo test` (all pass) and `cargo build` (no new warnings).

## Spec coverage check
- Cashflow gabungan → Task 1. Insights → Task 2. Monthly recap → Task 3.
- Price alerts (table/repo → Task 4; tools+tick → Task 5; semantics: absolute target, % converted at set-time). Prompt → Task 6.
- Migration 0020; Fase 2 PR → 0021 before merge (coordination note).
