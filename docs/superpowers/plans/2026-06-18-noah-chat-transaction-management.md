# Noah — Kelola Transaksi via Chat — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the Noah assistant create, list, edit, and delete investment transactions from chat, capture NAV+units from fund detail screenshots, and record reksadana buys with correct units (not `quantity = rupiah, price = 1`).

**Architecture:** A shared resolver `service::txn_entry::resolve_qty_price` turns whatever value fields the caller has (units+NAV, amount+NAV, amount+units, or amount-only) into a concrete `(quantity, price_native)`. Both the existing OCR confirm path and four new assistant tools call it. New tools follow the codebase's description-driven confirmation convention (the tool description instructs the LLM to echo + confirm before calling; no `confirm` flag in code).

**Tech Stack:** Rust, sqlx (SQLite), serde_json, rust_decimal, tokio, anyhow.

## Global Constraints

- NEVER run `cargo fmt` / rustfmt (rewrites ~604 files). Hand-edit only.
- Bin-only crate: run tests with `cargo test --bins <name>` (NOT `--lib`).
- Numbers persist as strings; parse/validate via `crate::repo::dec(s) -> anyhow::Result<Decimal>`.
- Tool handler signature: `async fn name(db: &Db, input: &serde_json::Value) -> Result<String, String>`.
- Arg helpers in `dispatcher.rs`: `str_arg(input, key) -> Option<&str>`, `id_arg(input, key) -> Result<i64, String>`, `optional_id(input, key) -> Result<Option<i64>, String>`.
- Confirmation is description-driven: write "Always echo the parsed … to the user and get confirmation before calling — this writes data." in each write-tool's description. Do NOT add a `confirm` flag.
- IDR fx normalization is handled inside `transactions::create`/`update` (`fx_to_idr = "1"` for IDR). Callers pass `fx_to_idr = "1"` for IDR, else latest USD/IDR.

---

## Phase 1 — get correct data in

### Task 1: Shared `(quantity, price)` resolver

**Files:**
- Create: `backend/src/service/txn_entry.rs`
- Modify: `backend/src/service/mod.rs` (add `pub mod txn_entry;`)
- Modify: `backend/src/ingestion/review.rs:91-192` (delete `amount_only_qty_price` + `append_note`; refactor `confirm` to call the resolver)

**Interfaces:**
- Consumes: `crate::repo::instruments::InstrumentRow` (fields `id: i64`, `price_source: String`); `crate::repo::{prices, transactions, dec}`.
- Produces:
  - `enum ResolveError { NeedNavOrUnits, Other(anyhow::Error) }` with `impl From<anyhow::Error>`.
  - `async fn resolve_qty_price(db: &Db, ins: &InstrumentRow, entry_type: &str, quantity: Option<&str>, price_native: Option<&str>, amount_native: Option<&str>, allow_price_one_fallback: bool, note: &mut Option<String>) -> Result<(String, String), ResolveError>`

- [ ] **Step 1: Add the module declaration**

In `backend/src/service/mod.rs`, add alongside the other `pub mod` lines:

```rust
pub mod txn_entry;
```

- [ ] **Step 2: Write the resolver with failing tests**

Create `backend/src/service/txn_entry.rs`:

```rust
//! Resolve a trade's (quantity, price_native) from whatever value fields the
//! caller has — units+NAV, amount+NAV, amount+units, or amount-only. Shared by
//! the OCR confirm path and the chat transaction tools so both record reksadana
//! buys as real units (quantity = amount/NAV) instead of quantity = rupiah.

use crate::db::Db;
use crate::repo::instruments::InstrumentRow;
use crate::repo::{prices, transactions};
use rust_decimal::Decimal;

/// Why a trade could not be resolved into concrete units and price.
pub enum ResolveError {
    /// A fund trade carried only a rupiah amount and no NAV/units could be
    /// derived — the caller must ask the user for NAV or unit count.
    NeedNavOrUnits,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for ResolveError {
    fn from(e: anyhow::Error) -> Self {
        ResolveError::Other(e)
    }
}

fn clean(s: Option<&str>) -> Option<&str> {
    s.map(str::trim).filter(|s| !s.is_empty())
}

/// Append a parenthetical note, joining onto any existing note.
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

/// Resolve a fund amount-only trade. For a bibit-sourced fund with a stored NAV
/// quote, derive real units (amount / NAV at 4 dp). Otherwise either fall back
/// to quantity = amount at price 1 (OCR fresh-buy) or signal NeedNavOrUnits
/// (manual entry). Reads only the quote table; never touches the network.
async fn amount_only(
    db: &Db,
    ins: &InstrumentRow,
    amount: &str,
    allow_price_one_fallback: bool,
    note: &mut Option<String>,
) -> Result<(String, String), ResolveError> {
    let amount_dec = crate::repo::dec(amount)?;
    if ins.price_source.starts_with("bibit:") {
        // Once an instrument has value-based (price = 1) rows, stay on that
        // convention — mixing NAV-derived units with rupiah-as-units rows makes
        // the position unreconcilable. Edit the legacy rows to real units to
        // unlock derivation.
        if transactions::has_price_one_txn(db, ins.id).await? {
            append_note(note, "dicatat nominal di harga 1 agar konsisten dengan transaksi sebelumnya");
            return Ok((amount_dec.normalize().to_string(), "1".to_string()));
        }
        if let Some(lp) = prices::latest(db, ins.id).await? {
            if lp.source == "bibit" && lp.price > Decimal::ZERO {
                let qty = (amount_dec / lp.price).round_dp(4);
                append_note(note, &format!("unit dihitung dari NAV {} per {}", lp.price.normalize(), lp.as_of));
                return Ok((qty.normalize().to_string(), lp.price.normalize().to_string()));
            }
        }
        if !allow_price_one_fallback {
            return Err(ResolveError::NeedNavOrUnits);
        }
        append_note(note, "NAV belum tersedia; dicatat nominal di harga 1");
    }
    Ok((amount_dec.normalize().to_string(), "1".to_string()))
}

/// Resolve (quantity, price_native). See module docs for the value-field matrix.
pub async fn resolve_qty_price(
    db: &Db,
    ins: &InstrumentRow,
    entry_type: &str,
    quantity: Option<&str>,
    price_native: Option<&str>,
    amount_native: Option<&str>,
    allow_price_one_fallback: bool,
    note: &mut Option<String>,
) -> Result<(String, String), ResolveError> {
    let q = clean(quantity);
    let p = clean(price_native);
    let a = clean(amount_native);

    // units + price: use verbatim.
    if let (Some(q), Some(p)) = (q, p) {
        return Ok((q.to_string(), p.to_string()));
    }
    // amount + price (NAV): qty = amount / price (4 dp, bibit unit precision).
    if let (Some(a), Some(p)) = (a, p) {
        let price = crate::repo::dec(p)?;
        let qty = (crate::repo::dec(a)? / price).round_dp(4);
        return Ok((qty.normalize().to_string(), price.normalize().to_string()));
    }
    // amount + units: price = amount / units.
    if let (Some(a), Some(q)) = (a, q) {
        let units = crate::repo::dec(q)?;
        let price = crate::repo::dec(a)? / units;
        return Ok((units.normalize().to_string(), price.normalize().to_string()));
    }
    // amount only: fund-aware derivation (buy/sell only).
    if let Some(a) = a {
        if matches!(entry_type, "buy" | "sell") {
            return amount_only(db, ins, a, allow_price_one_fallback, note).await;
        }
    }
    // quantity only (e.g. dividend in units): price defaults to 0.
    if let Some(q) = q {
        return Ok((q.to_string(), "0".to_string()));
    }
    Err(ResolveError::Other(anyhow::anyhow!(
        "butuh quantity+price atau amount untuk entry {entry_type}"
    )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::instruments::{self, NewInstrument};

    async fn fund(db: &Db) -> InstrumentRow {
        instruments::create(db, &NewInstrument {
            symbol: "MJR".into(), name: "Majoris Pasar Uang".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None,
            price_source: "bibit:MJR02".into(), decimals: Some(4), note: None,
        }).await.unwrap();
        instruments::list(db).await.unwrap().pop().unwrap()
    }

    #[tokio::test]
    async fn units_and_nav_pass_through() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", Some("1236.7898"), Some("1617.0896"), None, false, &mut note).await.ok().unwrap();
        assert_eq!(q, "1236.7898");
        assert_eq!(p, "1617.0896");
    }

    #[tokio::test]
    async fn amount_and_nav_derives_units() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", None, Some("1617.0896"), Some("2000000"), false, &mut note).await.ok().unwrap();
        assert_eq!(q, "1236.7898"); // 2000000 / 1617.0896 = 1236.78984..., 4dp
        assert_eq!(p, "1617.0896");
    }

    #[tokio::test]
    async fn amount_and_units_derives_price() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", Some("12367.8985"), None, Some("20000000"), false, &mut note).await.ok().unwrap();
        assert_eq!(q, "12367.8985");
        assert_eq!(p, "1617.089600..".trim_end_matches('.').trim_end_matches('0')); // ~1617.0896
    }

    #[tokio::test]
    async fn fund_amount_only_without_nav_asks_when_no_fallback() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let err = resolve_qty_price(&db, &ins, "buy", None, None, Some("2000000"), false, &mut note).await.err().unwrap();
        assert!(matches!(err, ResolveError::NeedNavOrUnits));
    }

    #[tokio::test]
    async fn fund_amount_only_without_nav_falls_back_when_allowed() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = fund(&db).await;
        let mut note = None;
        let (q, p) = resolve_qty_price(&db, &ins, "buy", None, None, Some("2000000"), true, &mut note).await.ok().unwrap();
        assert_eq!(q, "2000000");
        assert_eq!(p, "1");
        assert!(note.unwrap().contains("NAV belum tersedia"));
    }
}
```

Note on the `amount_and_units_derives_price` assertion: `20000000 / 12367.8985 = 1617.08960…`. Replace the contrived `trim` expression with the exact normalized string once you run the test (Step 3 will show the actual value); assert that literal.

- [ ] **Step 3: Run resolver tests — expect FAIL then iterate to PASS**

Run: `cargo test --bins service::txn_entry`
Expected first run: compile error (module/tests new) → fix the one literal in `amount_and_units_derives_price` to the value the panic prints, then all 5 tests PASS.

- [ ] **Step 4: Refactor `review.rs::confirm` to use the resolver**

In `backend/src/ingestion/review.rs`, DELETE `amount_only_qty_price` (lines ~94-124) and `append_note` (lines ~126-135). In `confirm`, replace the qty/price block (lines ~158-170) with:

```rust
    let mut note = p.note.clone();
    let has_qp = !p.quantity.trim().is_empty() || !p.price_native.trim().is_empty();
    let (quantity, price_native) = if has_qp {
        (p.quantity.clone(), p.price_native.clone())
    } else {
        let q = (!p.quantity.trim().is_empty()).then(|| p.quantity.as_str());
        let pr = (!p.price_native.trim().is_empty()).then(|| p.price_native.as_str());
        let amt = p.amount_native.as_deref().map(str::trim).filter(|a| !a.is_empty());
        match crate::service::txn_entry::resolve_qty_price(
            db, &ins, &p.entry_type, q, pr, amt, /* allow_price_one_fallback */ true, &mut note,
        ).await {
            Ok(pair) => pair,
            Err(crate::service::txn_entry::ResolveError::NeedNavOrUnits) => {
                return Err(anyhow::anyhow!("butuh NAV atau jumlah unit untuk {}", p.entry_type));
            }
            Err(crate::service::txn_entry::ResolveError::Other(e)) => return Err(e),
        }
    };
```

(`has_qp` true keeps the existing "use given values" path verbatim; the else branch is amount-only, fallback allowed = OCR behavior unchanged.)

- [ ] **Step 5: Verify OCR path tests still pass**

Run: `cargo test --bins ingestion::review`
Expected: PASS, including `amount_only_buy_without_nav_falls_back_to_price_one` (behavior unchanged for OCR).

- [ ] **Step 6: Commit**

```bash
git add backend/src/service/txn_entry.rs backend/src/service/mod.rs backend/src/ingestion/review.rs
git commit -m "refactor(txn): extract shared (quantity, price) resolver for trades"
```

---

### Task 2: `transactions::list_recent` repo function

**Files:**
- Modify: `backend/src/repo/transactions.rs` (add after `list_for_instrument`, ~line 98)

**Interfaces:**
- Produces: `async fn list_recent(db: &Db, limit: i64, instrument_id: Option<i64>, account_id: Option<i64>) -> anyhow::Result<Vec<Transaction>>`

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `backend/src/repo/transactions.rs` (follow existing test setup that creates an instrument + account then inserts a txn):

```rust
    #[tokio::test]
    async fn list_recent_orders_newest_first_and_filters() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (acc, ins) = super::tests_support::seed(&db).await; // see note
        for (d, q) in [("2026-06-01", "1"), ("2026-06-03", "2"), ("2026-06-02", "3")] {
            create(&db, &NewTransaction {
                account_id: acc, instrument_id: ins, txn_type: "buy".into(),
                executed_at: chrono::DateTime::parse_from_rfc3339(&format!("{d}T00:00:00Z")).unwrap().with_timezone(&chrono::Utc),
                quantity: q.into(), price_native: "1000".into(), fee_native: None,
                currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
                note: None, source: None, external_id: None,
            }).await.unwrap();
        }
        let recent = list_recent(&db, 2, None, None).await.unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].executed_at.format("%Y-%m-%d").to_string(), "2026-06-03");
        let by_ins = list_recent(&db, 10, Some(ins), None).await.unwrap();
        assert_eq!(by_ins.len(), 3);
    }
```

Note: if no shared seed helper exists, inline the instrument+account creation in the test the same way other tests in this file do (create an instrument via `crate::repo::instruments::create` and an account via `crate::repo::accounts::create`, return their ids), and drop the `tests_support::seed` reference.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bins repo::transactions::tests::list_recent_orders_newest_first_and_filters`
Expected: FAIL — `list_recent` not found.

- [ ] **Step 3: Implement `list_recent`**

```rust
/// Recent transactions, newest first, optionally filtered by instrument/account.
pub async fn list_recent(
    db: &Db,
    limit: i64,
    instrument_id: Option<i64>,
    account_id: Option<i64>,
) -> anyhow::Result<Vec<Transaction>> {
    let raws = sqlx::query_as::<_, TxnRowRaw>(
        "SELECT id, account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, fx_to_idr, fx_to_usd, note \
         FROM txn \
         WHERE (?1 IS NULL OR instrument_id = ?1) AND (?2 IS NULL OR account_id = ?2) \
         ORDER BY executed_at DESC, id DESC LIMIT ?3")
        .bind(instrument_id).bind(account_id).bind(limit)
        .fetch_all(db).await?;
    raws.into_iter().map(|r| r.into_domain()).collect()
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --bins repo::transactions::tests::list_recent_orders_newest_first_and_filters`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/transactions.rs
git commit -m "feat(repo): add transactions::list_recent with instrument/account filters"
```

---

### Task 3: `create_transaction` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs` (add a tool object near `confirm_review`, ~line 186)
- Modify: `backend/src/assistant/dispatcher.rs` (add match arm in `dispatch` ~line 30; add handler near `confirm_review` ~line 918)

**Interfaces:**
- Consumes: `crate::service::txn_entry::{resolve_qty_price, ResolveError}`; `crate::ingestion::matching::suggest_instrument_for_entry`; `crate::ingestion::review::to_rfc3339`; `crate::repo::{accounts, instruments, prices, transactions}`.

- [ ] **Step 1: Add the tool schema**

In `backend/src/assistant/tools.rs`, insert after the `confirm_review` object (after line 186):

```rust
        {
            "name": "create_transaction",
            "description": "Record an investment transaction the owner dictates in chat (no photo). For mutual funds (reksadana), pass NAV as price_native and units as quantity when known; or pass quantity + price_native; or amount_native with one of NAV/units. If the owner gives only a rupiah amount for a fund and you have no NAV or unit count, ASK for the NAV or unit count first — do not guess. Always echo the parsed transaction (instrument, type, qty @ price, total, account) to the owner and get confirmation before calling — this writes data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "instrument": { "type": "string", "description": "Instrument name or symbol, e.g. 'Majoris Pasar Uang Indonesia'. Or pass instrument_id." },
                    "instrument_id": { "type": "integer", "description": "Instrument id (from list_instruments) — overrides 'instrument'." },
                    "account": { "type": "string", "description": "Account name, e.g. 'Bibit #4'. Or pass account_id." },
                    "account_id": { "type": "integer", "description": "Account id (from list_accounts) — overrides 'account'." },
                    "entry_type": { "type": "string", "description": "buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance" },
                    "executed_at": { "type": "string", "description": "Date/time, RFC3339 or YYYY-MM-DD. Defaults to now." },
                    "quantity": { "type": "string", "description": "Units (for a fund: jumlah unit)." },
                    "price_native": { "type": "string", "description": "Price per unit in the instrument's currency (for a fund: NAV)." },
                    "amount_native": { "type": "string", "description": "Total transaction value in native currency." },
                    "fee_native": { "type": "string", "description": "Optional fee in native currency." },
                    "currency": { "type": "string", "description": "ISO code; defaults to IDR." },
                    "note": { "type": "string", "description": "Optional note." }
                },
                "required": ["entry_type"]
            }
        },
```

- [ ] **Step 2: Add the dispatch arm**

In `backend/src/assistant/dispatcher.rs` `dispatch`, after the `confirm_review` arm (line 29):

```rust
        "create_transaction" => create_transaction(db, input).await,
```

- [ ] **Step 3: Write the failing handler test**

Add to the dispatcher `tests` module (near the other review tests):

```rust
    #[tokio::test]
    async fn create_transaction_records_fund_buy_with_units_and_nav() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::db::migrate(&db).await.unwrap(); // if other tests migrate; else mirror their setup
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris Pasar Uang Indonesia".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(),
            decimals: Some(4), note: None,
        }).await.unwrap();
        let input = serde_json::json!({
            "account_id": acc.id, "instrument_id": ins.id, "entry_type": "buy",
            "executed_at": "2026-06-18", "quantity": "1236.7898", "price_native": "1617.0896",
        });
        let out = create_transaction(&db, &input).await.unwrap();
        assert!(out.contains("transaksi"));
        let txns = crate::repo::transactions::list_recent(&db, 10, Some(ins.id), None).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].quantity.to_string(), "1236.7898");
        assert_eq!(txns[0].price_native.to_string(), "1617.0896");
    }

    #[tokio::test]
    async fn create_transaction_fund_amount_only_asks_for_nav() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(),
            decimals: Some(4), note: None,
        }).await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None, native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let input = serde_json::json!({
            "account_id": acc.id, "instrument_id": ins.id, "entry_type": "buy",
            "amount_native": "2000000",
        });
        let err = create_transaction(&db, &input).await.unwrap_err();
        assert!(err.to_lowercase().contains("nav") || err.to_lowercase().contains("unit"));
    }
```

Note: match the exact DB setup the other dispatcher tests use (some connect to `sqlite::memory:` and rely on a `migrate` helper; copy whatever they do — do not invent `migrate` if they use a different bootstrap).

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test --bins dispatcher::tests::create_transaction`
Expected: FAIL — `create_transaction` not found.

- [ ] **Step 5: Implement the handler**

Add near `confirm_review` in `dispatcher.rs`:

```rust
async fn create_transaction(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let entry_type = str_arg(input, "entry_type").ok_or("missing required argument 'entry_type'")?;

    // Resolve instrument: id wins, else match by name/symbol.
    let instrument_id = match optional_id(input, "instrument_id")? {
        Some(id) => id,
        None => {
            let name = str_arg(input, "instrument")
                .ok_or("butuh 'instrument' (nama/simbol) atau 'instrument_id'")?;
            crate::ingestion::matching::suggest_instrument_for_entry(db, Some(name), Some(name))
                .await
                .map_err(|e| format!("db error: {e}"))?
                .ok_or_else(|| format!("instrumen '{name}' belum terdaftar — tambah dulu di Web UI → Data"))?
        }
    };
    // Resolve account: id wins, else case-insensitive name match.
    let account_id = match optional_id(input, "account_id")? {
        Some(id) => id,
        None => {
            let name = str_arg(input, "account").ok_or("butuh 'account' (nama) atau 'account_id'")?;
            let accounts = crate::repo::accounts::list(db).await.map_err(|e| format!("db error: {e}"))?;
            accounts.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.id)
                .ok_or_else(|| format!("akun '{name}' nggak ketemu — cek list_accounts"))?
        }
    };

    let ins = crate::repo::instruments::get(db, instrument_id).await
        .map_err(|_| format!("instrumen #{instrument_id} nggak ada"))?;
    crate::repo::accounts::get(db, account_id).await
        .map_err(|_| format!("akun #{account_id} nggak ada"))?;

    let executed_at = match str_arg(input, "executed_at") {
        Some(raw) => crate::ingestion::review::to_rfc3339(raw)
            .ok_or_else(|| format!("tanggal nggak terbaca: {raw}"))?,
        None => chrono::Utc::now().to_rfc3339(),
    };
    let currency = str_arg(input, "currency").unwrap_or("IDR").to_string();
    let mut note = str_arg(input, "note").map(str::to_string);

    let (quantity, price_native) = match crate::service::txn_entry::resolve_qty_price(
        db, &ins, entry_type,
        str_arg(input, "quantity"), str_arg(input, "price_native"), str_arg(input, "amount_native"),
        /* allow_price_one_fallback */ false, &mut note,
    ).await {
        Ok(pair) => pair,
        Err(crate::service::txn_entry::ResolveError::NeedNavOrUnits) =>
            return Err("aku butuh NAV atau jumlah unit-nya dulu buat reksadana ini — kasih salah satu ya".into()),
        Err(crate::service::txn_entry::ResolveError::Other(e)) => return Err(format!("{e}")),
    };

    let usd_idr = crate::repo::prices::latest_fx(db, "USD", "IDR").await
        .map_err(|e| format!("db error: {e}"))?
        .unwrap_or(rust_decimal::Decimal::ONE);
    let fx_to_idr = if currency == "IDR" { "1".to_string() } else { usd_idr.to_string() };

    let nt = crate::repo::transactions::NewTransaction {
        account_id, instrument_id, txn_type: entry_type.to_string(),
        executed_at: chrono::DateTime::parse_from_rfc3339(&executed_at)
            .map_err(|e| format!("tanggal: {e}"))?.with_timezone(&chrono::Utc),
        quantity, price_native, fee_native: str_arg(input, "fee_native").map(str::to_string),
        currency, fx_to_idr, fx_to_usd: "1".to_string(), note, source: Some("chat".into()), external_id: None,
    };
    let txn = crate::repo::transactions::create(db, &nt).await.map_err(|e| format!("{e}"))?;
    Ok(format!("transaksi #{} dicatat: {} {} @ {} di {}", txn.id, txn.txn_type, txn.quantity.normalize(), txn.price_native.normalize(), account_id))
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --bins dispatcher::tests::create_transaction`
Expected: PASS (both tests).

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): create_transaction tool (units+NAV; asks when fund NAV/units missing)"
```

---

### Task 4: `list_transactions` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs` (add tool object)
- Modify: `backend/src/assistant/dispatcher.rs` (add arm + handler)

- [ ] **Step 1: Add the tool schema**

After the `create_transaction` object in `tools.rs`:

```rust
        {
            "name": "list_transactions",
            "description": "List recent recorded transactions (newest first) so the owner can find one to edit or delete. Optionally filter by instrument or account name. Each line shows the txn id, date, type, instrument, qty @ price, and total.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "instrument": { "type": "string", "description": "Optional instrument name/symbol filter." },
                    "account": { "type": "string", "description": "Optional account name filter." },
                    "limit": { "type": "integer", "description": "Max rows, default 10, max 25." }
                }
            }
        },
```

- [ ] **Step 2: Add the dispatch arm**

```rust
        "list_transactions" => list_transactions(db, input).await,
```

- [ ] **Step 3: Write the failing test**

```rust
    #[tokio::test]
    async fn list_transactions_shows_recent_with_ids() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None, native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(), decimals: Some(4), note: None,
        }).await.unwrap();
        crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "1236.7898".into(), price_native: "1617.0896".into(),
            fee_native: None, currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();
        let out = list_transactions(&db, &serde_json::json!({})).await.unwrap();
        assert!(out.contains("#"));
        assert!(out.contains("MJR") || out.contains("Majoris"));
    }
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --bins dispatcher::tests::list_transactions_shows_recent_with_ids`
Expected: FAIL — `list_transactions` not found.

- [ ] **Step 5: Implement the handler**

```rust
async fn list_transactions(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let limit = input.get("limit").and_then(|v| v.as_i64()).unwrap_or(10).clamp(1, 25);
    let instrument_id = match str_arg(input, "instrument") {
        Some(name) => crate::ingestion::matching::suggest_instrument_for_entry(db, Some(name), Some(name))
            .await.map_err(|e| format!("db error: {e}"))?,
        None => None,
    };
    let account_id = match str_arg(input, "account") {
        Some(name) => {
            let accounts = crate::repo::accounts::list(db).await.map_err(|e| format!("db error: {e}"))?;
            accounts.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.id)
        }
        None => None,
    };
    let txns = crate::repo::transactions::list_recent(db, limit, instrument_id, account_id)
        .await.map_err(|e| format!("db error: {e}"))?;
    if txns.is_empty() {
        return Ok("belum ada transaksi".into());
    }
    let mut out = String::new();
    for t in txns {
        let ins = crate::repo::instruments::get(db, t.instrument_id).await.ok();
        let label = ins.map(|i| format!("{} ({})", i.symbol, i.name)).unwrap_or_else(|| format!("#{}", t.instrument_id));
        let total = (t.quantity * t.price_native + t.fee_native).normalize();
        out.push_str(&format!(
            "#{} {} {} — {} @ {} = {} — {}\n",
            t.id, t.executed_at.format("%Y-%m-%d"), t.txn_type, label,
            t.price_native.normalize(), t.quantity.normalize(), total,
        ));
    }
    Ok(out)
}
```

Note: `TxnType` is an enum — format it via its existing `Display`/`as_str` (check `domain/models.rs`); if `t.txn_type` doesn't `Display`, use the matching accessor the codebase already exposes.

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --bins dispatcher::tests::list_transactions_shows_recent_with_ids`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): list_transactions tool for finding txns to edit/delete"
```

---

### Task 5: OCR captures NAV + units when shown

**Files:**
- Modify: `backend/src/ingestion/ingest.rs:15` (the Bibit rule in `SYSTEM_PROMPT`)

- [ ] **Step 1: Write a failing intent test**

Add to a `tests` module in `ingest.rs` (create one if absent):

```rust
    #[test]
    fn system_prompt_tells_model_to_capture_nav_and_units_when_shown() {
        assert!(super::SYSTEM_PROMPT.contains("Jumlah Unit") || super::SYSTEM_PROMPT.contains("unit count"));
        assert!(super::SYSTEM_PROMPT.contains("NAV") && super::SYSTEM_PROMPT.contains("price_native"));
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bins ingestion::ingest::tests::system_prompt_tells_model_to_capture_nav_and_units_when_shown`
Expected: FAIL (current prompt says NEVER invent units/NAV and doesn't mention capturing them from a detail view).

- [ ] **Step 3: Edit the Bibit rule**

In the Bibit paragraph (line 15), REPLACE the sentence
`Mutual fund purchases are usually shown as an IDR amount with no units and no NAV: put that amount in "amount_native" and omit "quantity" and "price_native" entirely — NEVER invent units or NAV.`
with:

```
A pending purchase shows only an IDR amount with no units and no NAV — put that amount in "amount_native" and omit "quantity" and "price_native". BUT a settled order or a transaction-detail view DOES show "NAV" and "Jumlah Unit": when both are visible, set "price_native" to the NAV and "quantity" to the unit count (and still put the IDR total in "amount_native"). Never invent units or NAV — only copy them when the document shows them.
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --bins ingestion::ingest::tests::system_prompt_tells_model_to_capture_nav_and_units_when_shown`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/ingest.rs
git commit -m "feat(ingest): capture fund NAV + units from settled/detail views"
```

---

## Phase 2 — correct existing data

### Task 6: `transactions::update` repo function

**Files:**
- Modify: `backend/src/repo/transactions.rs` (add after `create`)

**Interfaces:**
- Produces: `pub struct TxnPatch { account_id, instrument_id, txn_type, executed_at, quantity, price_native, fee_native, currency, note: each Option<...> }` and `async fn update(db: &Db, id: i64, patch: &TxnPatch) -> anyhow::Result<Transaction>`.

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn update_changes_quantity_and_price_and_renormalizes_idr() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // seed instrument+account inline as other tests do; create a price=1 row
        let (acc, ins) = /* inline seed returning (account_id, instrument_id) */;
        let t = create(&db, &NewTransaction {
            account_id: acc, instrument_id: ins, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "2000000".into(), price_native: "1".into(),
            fee_native: None, currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();
        let patched = update(&db, t.id, &TxnPatch {
            quantity: Some("1236.7898".into()), price_native: Some("1617.0896".into()),
            ..Default::default()
        }).await.unwrap();
        assert_eq!(patched.quantity.to_string(), "1236.7898");
        assert_eq!(patched.price_native.to_string(), "1617.0896");
        assert_eq!(patched.fx_to_idr.to_string(), "1"); // IDR identity preserved
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bins repo::transactions::tests::update_changes_quantity_and_price_and_renormalizes_idr`
Expected: FAIL — `TxnPatch`/`update` not found.

- [ ] **Step 3: Implement `TxnPatch` + `update`**

```rust
#[derive(Debug, Default)]
pub struct TxnPatch {
    pub account_id: Option<i64>,
    pub instrument_id: Option<i64>,
    pub txn_type: Option<String>,
    pub executed_at: Option<DateTime<Utc>>,
    pub quantity: Option<String>,
    pub price_native: Option<String>,
    pub fee_native: Option<String>,
    pub currency: Option<String>,
    pub note: Option<String>,
}

/// Update selected fields of a transaction. Re-applies IDR fx normalization
/// (fx_to_idr = 1 for IDR) so an edit can never persist a bogus rate.
pub async fn update(db: &Db, id: i64, patch: &TxnPatch) -> anyhow::Result<Transaction> {
    let cur = get(db, id).await?;
    let account_id = patch.account_id.unwrap_or(cur.account_id);
    let instrument_id = patch.instrument_id.unwrap_or(cur.instrument_id);
    let txn_type = patch.txn_type.clone().unwrap_or_else(|| cur.txn_type.as_str().to_string());
    TxnType::from_str(&txn_type).map_err(|e| anyhow::anyhow!(e))?;
    let executed_at = patch.executed_at.unwrap_or(cur.executed_at);
    let quantity = patch.quantity.clone().unwrap_or_else(|| cur.quantity.to_string());
    let price_native = patch.price_native.clone().unwrap_or_else(|| cur.price_native.to_string());
    let fee_native = patch.fee_native.clone().unwrap_or_else(|| cur.fee_native.to_string());
    let currency = patch.currency.clone().unwrap_or(cur.currency);
    let note = patch.note.clone().or(cur.note);
    crate::repo::dec(&quantity)?; crate::repo::dec(&price_native)?; crate::repo::dec(&fee_native)?;

    let (fx_to_idr, fx_to_usd) = if currency == "IDR" {
        let usd_idr = crate::repo::prices::latest_fx(db, "USD", "IDR").await?;
        let to_usd = match usd_idr {
            Some(rate) if !rate.is_zero() => (rust_decimal::Decimal::ONE / rate).to_string(),
            _ => cur.fx_to_usd.to_string(),
        };
        ("1".to_string(), to_usd)
    } else {
        (cur.fx_to_idr.to_string(), cur.fx_to_usd.to_string())
    };

    sqlx::query(
        "UPDATE txn SET account_id=?, instrument_id=?, txn_type=?, executed_at=?, quantity=?, price_native=?, fee_native=?, currency=?, fx_to_idr=?, fx_to_usd=?, note=? WHERE id=?")
        .bind(account_id).bind(instrument_id).bind(&txn_type).bind(executed_at.to_rfc3339())
        .bind(&quantity).bind(&price_native).bind(&fee_native).bind(&currency)
        .bind(&fx_to_idr).bind(&fx_to_usd).bind(&note).bind(id)
        .execute(db).await?;
    get(db, id).await
}
```

Note: confirm `TxnType` exposes `as_str()` (used above and in `list_transactions`); if it only implements `Display`, use `cur.txn_type.to_string()` instead.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --bins repo::transactions::tests::update_changes_quantity_and_price_and_renormalizes_idr`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/transactions.rs
git commit -m "feat(repo): add transactions::update with IDR fx renormalization"
```

---

### Task 7: `edit_transaction` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs` (add tool object)
- Modify: `backend/src/assistant/dispatcher.rs` (add arm + handler)

- [ ] **Step 1: Add the tool schema**

```rust
        {
            "name": "edit_transaction",
            "description": "Edit fields of an existing recorded transaction (e.g. fix a reksadana row that was saved as quantity=rupiah, price=1 to real units + NAV). Get the id from list_transactions. Pass only the fields to change. Always echo the change to the owner and get confirmation before calling — this rewrites data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Transaction id from list_transactions." },
                    "entry_type": { "type": "string", "description": "buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance" },
                    "executed_at": { "type": "string", "description": "Date/time, RFC3339 or YYYY-MM-DD." },
                    "quantity": { "type": "string", "description": "New units (for a fund: jumlah unit)." },
                    "price_native": { "type": "string", "description": "New price per unit (for a fund: NAV)." },
                    "fee_native": { "type": "string", "description": "New fee." },
                    "account": { "type": "string", "description": "New account name." },
                    "instrument": { "type": "string", "description": "New instrument name/symbol." },
                    "note": { "type": "string", "description": "New note." }
                },
                "required": ["id"]
            }
        },
```

- [ ] **Step 2: Add the dispatch arm**

```rust
        "edit_transaction" => edit_transaction(db, input).await,
```

- [ ] **Step 3: Write the failing test**

```rust
    #[tokio::test]
    async fn edit_transaction_fixes_price_one_fund_row() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None, native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(), decimals: Some(4), note: None,
        }).await.unwrap();
        let t = crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "2000000".into(), price_native: "1".into(),
            fee_native: None, currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();
        let out = edit_transaction(&db, &serde_json::json!({
            "id": t.id, "quantity": "1236.7898", "price_native": "1617.0896",
        })).await.unwrap();
        assert!(out.contains(&format!("#{}", t.id)));
        let updated = crate::repo::transactions::get(&db, t.id).await.unwrap();
        assert_eq!(updated.quantity.to_string(), "1236.7898");
        assert_eq!(updated.price_native.to_string(), "1617.0896");
    }
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --bins dispatcher::tests::edit_transaction_fixes_price_one_fund_row`
Expected: FAIL — `edit_transaction` not found.

- [ ] **Step 5: Implement the handler**

```rust
async fn edit_transaction(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    crate::repo::transactions::get(db, id).await.map_err(|_| format!("transaksi #{id} nggak ada"))?;

    let instrument_id = match str_arg(input, "instrument") {
        Some(name) => crate::ingestion::matching::suggest_instrument_for_entry(db, Some(name), Some(name))
            .await.map_err(|e| format!("db error: {e}"))?
            .or_else(|| None),
        None => None,
    };
    let account_id = match str_arg(input, "account") {
        Some(name) => {
            let accounts = crate::repo::accounts::list(db).await.map_err(|e| format!("db error: {e}"))?;
            accounts.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.id)
        }
        None => None,
    };
    let executed_at = match str_arg(input, "executed_at") {
        Some(raw) => Some(
            chrono::DateTime::parse_from_rfc3339(
                &crate::ingestion::review::to_rfc3339(raw).ok_or_else(|| format!("tanggal nggak terbaca: {raw}"))?,
            ).map_err(|e| format!("tanggal: {e}"))?.with_timezone(&chrono::Utc),
        ),
        None => None,
    };

    let patch = crate::repo::transactions::TxnPatch {
        account_id, instrument_id,
        txn_type: str_arg(input, "entry_type").map(str::to_string),
        executed_at,
        quantity: str_arg(input, "quantity").map(str::to_string),
        price_native: str_arg(input, "price_native").map(str::to_string),
        fee_native: str_arg(input, "fee_native").map(str::to_string),
        currency: None,
        note: str_arg(input, "note").map(str::to_string),
    };
    let t = crate::repo::transactions::update(db, id, &patch).await.map_err(|e| format!("{e}"))?;
    Ok(format!("transaksi #{} diperbarui: {} @ {}", t.id, t.quantity.normalize(), t.price_native.normalize()))
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --bins dispatcher::tests::edit_transaction_fixes_price_one_fund_row`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): edit_transaction tool (fixes price=1 fund rows to real units)"
```

---

### Task 8: `delete_transaction` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs` (add tool object)
- Modify: `backend/src/assistant/dispatcher.rs` (add arm + handler)

- [ ] **Step 1: Add the tool schema**

```rust
        {
            "name": "delete_transaction",
            "description": "Delete a recorded transaction by id (e.g. a wrong entry). Get the id from list_transactions. Always confirm with the owner before calling — this permanently removes data.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Transaction id from list_transactions." } },
                "required": ["id"]
            }
        },
```

- [ ] **Step 2: Add the dispatch arm**

```rust
        "delete_transaction" => delete_transaction(db, input).await,
```

- [ ] **Step 3: Write the failing test**

```rust
    #[tokio::test]
    async fn delete_transaction_removes_the_row() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None, native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(), decimals: Some(4), note: None,
        }).await.unwrap();
        let t = crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "buy".into(),
            executed_at: chrono::Utc::now(), quantity: "1".into(), price_native: "1000".into(),
            fee_native: None, currency: "IDR".into(), fx_to_idr: "1".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();
        let out = delete_transaction(&db, &serde_json::json!({"id": t.id})).await.unwrap();
        assert!(out.contains(&format!("#{}", t.id)));
        assert!(crate::repo::transactions::get(&db, t.id).await.is_err());
    }
```

- [ ] **Step 4: Run test to verify it fails**

Run: `cargo test --bins dispatcher::tests::delete_transaction_removes_the_row`
Expected: FAIL — `delete_transaction` not found.

- [ ] **Step 5: Implement the handler**

```rust
async fn delete_transaction(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    crate::repo::transactions::get(db, id).await.map_err(|_| format!("transaksi #{id} nggak ada"))?;
    crate::repo::transactions::delete(db, id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("transaksi #{id} dihapus"))
}
```

- [ ] **Step 6: Run test to verify it passes**

Run: `cargo test --bins dispatcher::tests::delete_transaction_removes_the_row`
Expected: PASS.

- [ ] **Step 7: Final full-suite check + commit**

```bash
cargo test --bins
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): delete_transaction tool"
```

Expected: full suite green (note: the `create_invoice_persists_and_reports_number` test is a known parallel-run flake — re-run it in isolation if it fails).

---

## Self-Review notes

- **Spec coverage:** Tujuan 1 → Task 3; Tujuan 2 → Task 5; Tujuan 3 → Tasks 6-8; Tujuan 4 → Task 1 (resolver) + Task 7 (edit fixes legacy rows). `list_transactions` (Task 4) supports edit/delete targeting. Non-goals (instrument creation, account deletion, NAV backfill) intentionally untouched.
- **Confirmation:** description-driven, matching `create_account`/`create_invoice`; no `confirm` flag — consistent with the codebase.
- **Type consistency:** `resolve_qty_price` / `ResolveError` / `TxnPatch` / `list_recent` / `update` signatures are referenced identically across tasks.
- **Open verification points flagged inline:** (a) exact normalized literal in Task 1 Step 2's `amount_and_units_derives_price`; (b) dispatcher test DB bootstrap (`migrate` vs other helper); (c) `TxnType` accessor (`as_str()` vs `Display`). Each has a note telling the implementer to confirm against the codebase rather than assume.
