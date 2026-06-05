# Bibit NAV Provider + Unit Derivation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Daily NAV quotes for Indonesian mutual funds scraped from Bibit's public product pages, and real-unit derivation (`quantity = amount / NAV`) when confirming amount-only fund buys.

**Architecture:** New standalone pricing client `pricing/bibit.rs` (pure `parse_nav()` + thin HTTP wrapper, mirroring the FxClient/Yahoo split); a `bibit:` dispatch branch in `refresh_all` (gold-provider template) storing quotes under the page's NAV date; source-aware staleness (96h for `bibit`, 24h otherwise); and a derivation step inside `confirm()`'s existing amount-only gate that reads the stored quote — never the network — and falls back to the established `quantity = amount, price = 1` convention.

**Tech Stack:** Rust (reqwest, serde_json, rust_decimal, thiserror), React/TS (text change only).

**Spec:** `docs/superpowers/specs/2026-06-05-bibit-nav-provider-design.md`

**Branch/worktree:** `feat/bibit-nav-provider` at `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/bibit-nav`

**Implementer context:**
- All cargo commands run from `<worktree>/backend`; frontend from `<worktree>/frontend`; git from the worktree root.
- Do NOT run `cargo fmt` — this repo intentionally uses compact style. Gate on `cargo clippy --all-targets -- -D warnings` and `cargo test`.
- The real page structure (verified live 2026-06-05): `https://bibit.id/reksadana/RD1436/<slug>` embeds `<script id="__NEXT_DATA__" type="application/json">{...}</script>` whose JSON has `props.pageProps.productDetail.nav = {"date":"2026-06-04","first_date":"2016-12-08","value":1697.22}`.
- No network calls in tests — everything tests pure functions or sqlite::memory:.

---

### Task 1: `pricing/bibit.rs` — `parse_nav()` + `BibitNav` client

**Files:**
- Create: `backend/src/pricing/bibit.rs`
- Modify: `backend/src/pricing/mod.rs:1-4` (add `pub mod bibit;`)

- [ ] **Step 1: Register the module and create the file with failing tests**

In `backend/src/pricing/mod.rs`, add `pub mod bibit;` as the first module line (alphabetical: bibit, coingecko, fx, service, yahoo).

Create `backend/src/pricing/bibit.rs`:

```rust
use super::PriceError;
use rust_decimal::Decimal;
use std::str::FromStr;

/// A fund NAV quote: price per unit in IDR and the NAV date printed on the page.
#[derive(Debug, Clone)]
pub struct NavQuote {
    pub price: Decimal,
    pub as_of: String, // "YYYY-MM-DD" from productDetail.nav.date
}

/// Extract the NAV from a Bibit product page. The page is Next.js
/// server-rendered: full fund data sits in a `<script id="__NEXT_DATA__">`
/// JSON blob at `props.pageProps.productDetail.nav.{value,date}`.
pub fn parse_nav(html: &str) -> Result<NavQuote, PriceError> {
    let marker = "<script id=\"__NEXT_DATA__\"";
    let start = html.find(marker).ok_or_else(|| PriceError::Parse("__NEXT_DATA__ script not found".into()))?;
    let body = &html[start..];
    let open = body.find('>').ok_or_else(|| PriceError::Parse("__NEXT_DATA__ tag malformed".into()))?;
    let body = &body[open + 1..];
    let end = body.find("</script>").ok_or_else(|| PriceError::Parse("__NEXT_DATA__ not closed".into()))?;
    let v: serde_json::Value = serde_json::from_str(&body[..end])
        .map_err(|e| PriceError::Parse(format!("__NEXT_DATA__ not valid JSON: {e}")))?;
    let nav = v.pointer("/props/pageProps/productDetail/nav")
        .ok_or_else(|| PriceError::Parse("productDetail.nav missing".into()))?;
    let as_of = nav.get("date").and_then(|d| d.as_str())
        .ok_or_else(|| PriceError::Parse("nav.date missing".into()))?
        .to_string();
    // Go through the raw JSON number's string form, not f64, to avoid float artifacts.
    let raw = nav.get("value").ok_or_else(|| PriceError::Parse("nav.value missing".into()))?;
    let price = Decimal::from_str(&raw.to_string())
        .map_err(|e| PriceError::Parse(format!("nav.value not a decimal: {e}")))?;
    if price <= Decimal::ZERO {
        return Err(PriceError::Parse(format!("nav.value not positive: {price}")));
    }
    Ok(NavQuote { price, as_of })
}

/// Fetches NAV from Bibit's public product page (no auth; desktop UA).
pub struct BibitNav {
    base: String,
    client: reqwest::Client,
}

impl BibitNav {
    pub fn new() -> Self {
        Self { base: "https://bibit.id/reksadana".into(), client: reqwest::Client::new() }
    }

    /// `code` is the fund's RDCODE, e.g. "RD1436". The slug segment after the
    /// code is cosmetic — any value routes.
    pub async fn latest(&self, code: &str) -> Result<NavQuote, PriceError> {
        let url = format!("{}/{}/x", self.base, code);
        let resp = self.client.get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send().await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(PriceError::Http(format!("status {} for {code}", resp.status())));
        }
        let html = resp.text().await.map_err(|e| PriceError::Http(e.to_string()))?;
        parse_nav(&html)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    /// Trimmed to the real shape of bibit.id/reksadana/RD1436 (captured 2026-06-05).
    fn page(nav_json: &str) -> String {
        format!(
            "<html><head><title>x</title></head><body><div id=\"app\">...</div>\
             <script id=\"__NEXT_DATA__\" type=\"application/json\">\
             {{\"props\":{{\"pageProps\":{{\"productDetail\":{{\"symbol\":\"RD1436\",\
             \"name\":\"Sucorinvest Bond Fund\",\"type\":\"Obligasi\",{nav_json}}}}}}},\
             \"page\":\"/reksadana/[id]/[slug]\"}}</script></body></html>"
        )
    }

    #[test]
    fn parses_nav_value_and_date() {
        let html = page("\"nav\":{\"date\":\"2026-06-04\",\"first_date\":\"2016-12-08\",\"value\":1697.22}");
        let q = parse_nav(&html).unwrap();
        assert_eq!(q.price, dec!(1697.22));
        assert_eq!(q.as_of, "2026-06-04");
    }

    #[test]
    fn missing_nav_is_parse_error() {
        let html = page("\"aum\":{\"value\":1.0}");
        assert!(matches!(parse_nav(&html), Err(PriceError::Parse(_))));
    }

    #[test]
    fn garbage_html_is_parse_error() {
        assert!(matches!(parse_nav("<html>nope</html>"), Err(PriceError::Parse(_))));
        assert!(matches!(parse_nav(""), Err(PriceError::Parse(_))));
    }

    #[test]
    fn non_positive_nav_is_parse_error() {
        let html = page("\"nav\":{\"date\":\"2026-06-04\",\"value\":0}");
        assert!(matches!(parse_nav(&html), Err(PriceError::Parse(_))));
    }

    #[test]
    fn missing_date_is_parse_error() {
        let html = page("\"nav\":{\"value\":1697.22}");
        assert!(matches!(parse_nav(&html), Err(PriceError::Parse(_))));
    }
}
```

- [ ] **Step 2: Run tests to verify they pass (pure function, written together with impl)**

Run: `cargo test pricing::bibit`
Expected: 5 passed. (parse_nav and its tests land together; the failing-first checkpoint for this task is the compile error before `pub mod bibit;` + file exist — if you want the strict TDD beat, add the mod line and an empty file first and watch `cargo test pricing::bibit` find zero tests.)

- [ ] **Step 3: Clippy**

Run: `cargo clippy --all-targets -- -D warnings`
Expected: clean. (If clippy flags `BibitNav::new` for `new_without_default`, add `impl Default for BibitNav { fn default() -> Self { Self::new() } }` — Yahoo has the same shape; mirror whatever it does. Check: `grep -n "Default" backend/src/pricing/yahoo.rs` — if Yahoo has no Default impl and clippy passes, BibitNav needs none either.)

- [ ] **Step 4: Commit**

```bash
git add backend/src/pricing/bibit.rs backend/src/pricing/mod.rs
git commit -m "feat(pricing): bibit NAV client parsing __NEXT_DATA__ fund pages"
```

---

### Task 2: Dispatch `bibit:` in `refresh_all`

**Files:**
- Modify: `backend/src/pricing/service.rs:28-75` (`refresh_all`)

- [ ] **Step 1: Add the branch**

In `refresh_all`, after `let fx = FxClient::new();` add:

```rust
    let bibit = crate::pricing::bibit::BibitNav::new();
```

Inside the `for ins in instruments::list(db).await?` loop, after the `yahoo:` block and before the gold block, add:

```rust
        // Indonesian mutual fund NAV scraped from Bibit's public product page.
        // Store under the page's NAV date (T-1), not today — keeps staleness honest
        // and avoids duplicating the same NAV under multiple dates.
        if let Some(code) = ins.price_source.strip_prefix("bibit:") {
            match bibit.latest(code).await {
                Ok(q) => { let _ = prices::upsert_latest(db, ins.id, q.price, "IDR", "bibit", &q.as_of).await; }
                Err(e) => tracing::warn!("bibit nav refresh failed for {}: {e}", ins.symbol),
            }
        }
```

- [ ] **Step 2: Build + full test suite**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass, clippy clean. (No network test for the dispatch branch — same policy as the yahoo/coingecko branches; correctness of parsing is covered by Task 1, of storage by `repo::prices` tests.)

- [ ] **Step 3: Commit**

```bash
git add backend/src/pricing/service.rs
git commit -m "feat(pricing): refresh bibit NAV quotes under their page NAV date"
```

---

### Task 3: Source-aware staleness (96h for `bibit`)

**Files:**
- Modify: `backend/src/pricing/service.rs` (new fn + tests)
- Modify: `backend/src/service/portfolio.rs:42`
- Modify: `backend/src/repo/prices.rs:6` (drop `#[allow(dead_code)]` on `source`)

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `backend/src/pricing/service.rs`:

```rust
    #[test]
    fn bibit_quotes_get_a_four_day_stale_window() {
        assert_eq!(stale_window_hours("bibit"), 96);
        assert_eq!(stale_window_hours("yahoo"), 24);
        assert_eq!(stale_window_hours("coingecko"), 24);
        assert_eq!(stale_window_hours("manual"), 24);
    }

    #[test]
    fn fund_nav_three_days_old_is_fresh_five_days_is_stale() {
        let now = Utc.with_ymd_and_hms(2026, 6, 8, 12, 0, 0).unwrap(); // Monday
        // NAV dated Friday-ish (3 days back) — fine under the 96h fund window.
        assert!(!is_stale("2026-06-05", now, stale_window_hours("bibit")));
        // 5 days back — stale even for funds.
        assert!(is_stale("2026-06-03", now, stale_window_hours("bibit")));
        // Same age under the default window — stale.
        assert!(is_stale("2026-06-05", now, stale_window_hours("yahoo")));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test pricing::service`
Expected: compile error — `stale_window_hours` not found.

- [ ] **Step 3: Implement**

In `backend/src/pricing/service.rs`, right after `is_stale` (line 9), add:

```rust
/// Staleness window per quote source. Fund NAV (bibit) is T-1 and pauses over
/// weekends/market holidays, so it gets 4 days; everything else 24h.
pub fn stale_window_hours(source: &str) -> i64 {
    if source == "bibit" { 96 } else { 24 }
}
```

In `backend/src/service/portfolio.rs` line 42, change:

```rust
            Some(lp) => (lp.price, crate::pricing::service::is_stale(&lp.as_of, chrono::Utc::now(), 24)),
```

to:

```rust
            Some(lp) => (lp.price, crate::pricing::service::is_stale(&lp.as_of, chrono::Utc::now(), crate::pricing::service::stale_window_hours(&lp.source))),
```

In `backend/src/repo/prices.rs` line 6, the `source` field is now read in non-test code — remove the `#[allow(dead_code)]`:

```rust
pub struct LatestPrice { pub price: Decimal, pub as_of: String, pub source: String }
```

- [ ] **Step 4: Run tests**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass (including `service::portfolio` tests — their fixture quote uses source "test" → 24h window, unchanged behavior), clippy clean.

- [ ] **Step 5: Commit**

```bash
git add backend/src/pricing/service.rs backend/src/service/portfolio.rs backend/src/repo/prices.rs
git commit -m "feat(pricing): 4-day staleness window for bibit fund NAV quotes"
```

---

### Task 4: Unit derivation in `confirm()`

**Files:**
- Modify: `backend/src/ingestion/review.rs` (confirm + helper + tests)

- [ ] **Step 1: Write the failing tests**

Add to the tests module in `backend/src/ingestion/review.rs` (alongside the existing `seed`):

```rust
    /// A Bibit-sourced mutual fund instrument + goal account.
    async fn seed_fund(db: &Db) -> (i64, i64) {
        let a = accounts::create(db, &accounts::NewAccount { name:"Pendidikan Noah".into(), account_type:"manual".into(), institution:None, native_currency:"IDR".into(), note:None }).await.unwrap();
        let i = instruments::create(db, &instruments::NewInstrument { symbol:"RD1436".into(), name:"Sucorinvest Bond Fund".into(), instrument_type:"mutual_fund".into(), native_currency:"IDR".into(), category_id:None, price_source:"bibit:RD1436".into(), decimals:Some(4), note:None }).await.unwrap();
        (a.id, i.id)
    }

    fn amount_only_payload(account_id: i64, instrument_id: i64, entry_type: &str, amount: &str) -> ConfirmPayload {
        ConfirmPayload {
            account_id, instrument_id, entry_type: entry_type.into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some(amount.into()),
        }
    }

    #[tokio::test]
    async fn amount_only_buy_with_stored_nav_derives_units() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await;
        crate::repo::prices::upsert_latest(&db, instrument_id, dec!(1697.22), "IDR", "bibit", "2026-06-04").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "13000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, (dec!(13000000) / dec!(1697.22)).round_dp(4));
        assert_eq!(txns[0].price_native, dec!(1697.22));
        let note = txns[0].note.clone().unwrap_or_default();
        assert!(note.contains("NAV 1697.22"), "note should record the NAV used: {note}");
        assert!(note.contains("2026-06-04"), "note should record the NAV date: {note}");
    }

    #[tokio::test]
    async fn amount_only_buy_without_nav_falls_back_to_price_one() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await; // no quote stored
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "13000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(13000000));
        assert_eq!(txns[0].price_native, dec!(1));
        let note = txns[0].note.clone().unwrap_or_default();
        assert!(note.contains("NAV belum tersedia"), "fallback should be noted: {note}");
    }

    #[tokio::test]
    async fn amount_only_sell_with_nav_derives_units_too() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed_fund(&db).await;
        crate::repo::prices::upsert_latest(&db, instrument_id, dec!(1617.0896), "IDR", "bibit", "2026-06-04").await.unwrap();
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "sell", "5000000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, (dec!(5000000) / dec!(1617.0896)).round_dp(4));
        assert_eq!(txns[0].price_native, dec!(1617.0896));
    }

    #[tokio::test]
    async fn amount_only_on_non_bibit_instrument_keeps_price_one_without_note() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await; // crypto, price_source "manual"
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        confirm(&db, item.id, &amount_only_payload(account_id, instrument_id, "buy", "750000")).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(750000));
        assert_eq!(txns[0].price_native, dec!(1));
        assert_eq!(txns[0].note, None, "non-bibit fallback must not add a note");
    }
```

Note: the existing amount-only tests construct `ConfirmPayload` inline; leave them as they are (they cover the same fallback path via the crypto instrument).

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test ingestion::review`
Expected: the three NAV tests FAIL (quantity equals the amount instead of derived units / missing notes); `amount_only_on_non_bibit_instrument_keeps_price_one_without_note` passes already (pins current behavior). Existing tests pass.

- [ ] **Step 3: Implement**

In `backend/src/ingestion/review.rs`:

3a. Bind the instrument lookup (currently discarded). Change:

```rust
    crate::repo::instruments::get(db, p.instrument_id).await
        .map_err(|_| anyhow::anyhow!("unknown instrument_id {}", p.instrument_id))?;
```

to:

```rust
    let ins = crate::repo::instruments::get(db, p.instrument_id).await
        .map_err(|_| anyhow::anyhow!("unknown instrument_id {}", p.instrument_id))?;
```

3b. Replace the amount-only block (the `let (quantity, price_native) = ...` expression and the comment above it) with:

```rust
    // Amount-only mutual fund trades (e.g. Bibit "Order" screenshots) carry a
    // total IDR amount but no units/NAV. With a stored NAV quote (bibit:* price
    // source, refreshed daily) we derive real units: quantity = amount / NAV at
    // price = NAV. Without one we fall back to quantity = amount at price 1,
    // which values the position at cost via the avg_cost fallback
    // (service/portfolio.rs).
    let mut note = p.note.clone();
    let (quantity, price_native) = if p.quantity.trim().is_empty() && p.price_native.trim().is_empty() {
        let amount = p.amount_native.as_deref().map(str::trim).filter(|a| !a.is_empty());
        match (amount, p.entry_type.as_str()) {
            (Some(a), "buy" | "sell") => amount_only_qty_price(db, &ins, a, &mut note).await?,
            _ => return Err(anyhow::anyhow!(
                "quantity/price or amount_native is required for a {} entry",
                p.entry_type
            )),
        }
    } else {
        (p.quantity.clone(), p.price_native.clone())
    };
```

3c. In the `NewTransaction` literal, change `note: p.note.clone(),` to `note,`.

3d. Add the helper above `confirm()` (after the `ConfirmPayload` struct):

```rust
use crate::repo::instruments::InstrumentRow;

/// Resolve an amount-only trade into (quantity, price). For bibit-sourced
/// funds with a stored NAV quote, derive real units (amount / NAV, 4 dp —
/// Bibit's unit precision). Otherwise: quantity = amount at price 1. Never
/// touches the network; reads only the quote table.
async fn amount_only_qty_price(
    db: &Db,
    ins: &InstrumentRow,
    amount: &str,
    note: &mut Option<String>,
) -> anyhow::Result<(String, String)> {
    let amount_dec = crate::repo::dec(amount)?;
    if ins.price_source.starts_with("bibit:") {
        if let Some(lp) = prices::latest(db, ins.id).await? {
            if lp.price > Decimal::ZERO {
                let qty = (amount_dec / lp.price).round_dp(4);
                append_note(note, &format!("unit dihitung dari NAV {} per {}", lp.price.normalize(), lp.as_of));
                return Ok((qty.normalize().to_string(), lp.price.normalize().to_string()));
            }
        }
        append_note(note, "NAV belum tersedia; dicatat nominal di harga 1");
    }
    Ok((amount_dec.normalize().to_string(), "1".to_string()))
}

fn append_note(note: &mut Option<String>, msg: &str) {
    match note {
        Some(n) => {
            n.push_str(" (");
            n.push_str(msg);
            n.push(')');
        }
        None => *note = Some(format!("({msg})")),
    }
}
```

(`Decimal` is already imported in this file; `prices` is already imported via `use crate::repo::{prices, review_items, transactions};`.)

- [ ] **Step 4: Run tests**

Run: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all pass, clippy clean. Existing amount-only tests stay green: the crypto instrument's `price_source` is `"manual"`, so they take the unchanged fallback path. The derived strings still flow through `repo::dec()` validation inside `transactions::create`.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/review.rs
git commit -m "feat(ingest): derive fund units from stored NAV on amount-only confirm"
```

---

### Task 5: Frontend hint text

**Files:**
- Modify: `frontend/src/pages/ImportPage.tsx:228`

- [ ] **Step 1: Update the hint**

Change:

```tsx
            <span className="t-xs t-muted">amount-only — dicatat sebagai nominal di harga 1</span>
```

to:

```tsx
            <span className="t-xs t-muted">amount-only — unit dihitung otomatis dari NAV (tanpa NAV: nominal di harga 1)</span>
```

- [ ] **Step 2: Run the frontend suite**

Run: `npx vitest run && npm run build`
Expected: all pass (the existing test asserts `/amount-only/i`, which still matches), build clean.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/ImportPage.tsx
git commit -m "feat(frontend): hint that fund units derive from NAV when available"
```

---

### Task 6: Full verification

**Files:** none

- [ ] **Step 1: Backend**

Run from `backend/`: `cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all tests pass (≈225+), clippy clean. Do NOT run cargo fmt.

- [ ] **Step 2: Frontend**

Run from `frontend/`: `npx vitest run && npm run build`
Expected: 100 tests pass, build clean.

- [ ] **Step 3: One live smoke check (manual, outside tests)**

```bash
curl -s -A "Mozilla/5.0" https://bibit.id/reksadana/RD1436/x | grep -o '"nav":{[^}]*}' | head -1
```
Expected: a JSON fragment with `"value":<number>` and `"date":"YYYY-MM-DD"`. This confirms the page structure still matches `parse_nav` before shipping. If it doesn't match, STOP and report — the parser needs adjusting to the live structure.

- [ ] **Step 4: Review the branch**

```bash
git log --oneline origin/main..HEAD
git status --short   # clean except intended files
```
Expected: spec/plan docs + 5 feature commits. Do NOT push or open a PR — integration goes through finishing-a-development-branch (pushing main auto-deploys prod).
