# Assistant-Driven Review Confirmation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the Telegram assistant agent see pending ingest review items, create a missing account, and confirm/reject the transaction conversationally — closing the dead-end when an ingested item's account isn't auto-matched.

**Architecture:** Move the existing confirmability/payload helper into `ingestion::review` so both Telegram buttons and the assistant share one rule. Add a repo method to fill in resolved account/instrument ids. Add five assistant tools (`list_pending_reviews`, `list_accounts`, `create_account`, `confirm_review`, `reject_review`) as JSON schemas in `assistant::tools` plus handlers in `assistant::dispatcher`, and teach the system prompt the resolve-then-confirm flow.

**Tech Stack:** Rust, sqlx (SQLite), tokio, serde_json, anyhow. Tests run with `cargo test` from `backend/`.

---

## File Structure

- `backend/src/ingestion/review.rs` — gains `to_rfc3339` + `build_confirm_payload` (moved from telegram) and their unit tests. Already owns `ConfirmPayload`, `confirm`, `reject`.
- `backend/src/telegram/mod.rs` — drops the two moved functions; call sites point at `crate::ingestion::review::build_confirm_payload`. Keeps `item_summary`, `fmt_payload_num`, callback handling.
- `backend/src/repo/review_items.rs` — gains `set_suggestions`.
- `backend/src/assistant/tools.rs` — gains 5 tool schemas; schema tests updated.
- `backend/src/assistant/dispatcher.rs` — gains 5 handlers + `optional_id` helper; dispatch match arms; tests.
- `backend/src/assistant/agent.rs` — `SYSTEM` const gains review-flow guidance; prompt test updated.

All commands run from `backend/`.

---

### Task 1: Move the confirm-payload helper into `ingestion::review`

**Files:**
- Modify: `backend/src/ingestion/review.rs`
- Modify: `backend/src/telegram/mod.rs`

- [ ] **Step 1: Add the two functions to `ingestion::review`**

At the top of `backend/src/ingestion/review.rs`, confirm these imports exist (add any missing): `use crate::ingestion::extract::ExtractedEntry;` and `use crate::repo::review_items::ReviewItemRow;`. Then add, after the `ConfirmPayload` struct:

```rust
/// Coerce a payload date into RFC3339: full RFC3339 passes through,
/// "YYYY-MM-DDTHH:MM" and date-only values are assumed UTC.
pub fn to_rfc3339(s: &str) -> Option<String> {
    if chrono::DateTime::parse_from_rfc3339(s).is_ok() {
        return Some(s.to_string());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Some(format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S")));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(format!("{d}T00:00:00Z"));
    }
    None
}

/// Build the ConfirmPayload for one-tap confirmation, or explain (in user
/// language) why the item must be completed in the web UI instead.
pub fn build_confirm_payload(item: &ReviewItemRow) -> Result<ConfirmPayload, String> {
    if item.needs_attention != 0 {
        return Err("item ini perlu dicek manual".into());
    }
    let account_id = item.suggested_account_id.ok_or("akun belum dikenali")?;
    let instrument_id = item.suggested_instrument_id.ok_or("instrumen belum dikenali")?;
    let entry: ExtractedEntry = serde_json::from_str(&item.payload_json)
        .map_err(|e| format!("payload tidak terbaca: {e}"))?;
    let amount_only = entry.quantity.is_none()
        && entry.price_native.is_none()
        && entry.amount_native.is_some()
        && matches!(entry.entry_type.as_str(), "buy" | "sell");
    let (quantity, price_native) = if amount_only {
        (String::new(), String::new())
    } else {
        (
            entry.quantity.ok_or("jumlah tidak ada")?,
            entry.price_native.ok_or("harga tidak ada")?,
        )
    };
    let currency = entry.currency.ok_or("mata uang tidak ada")?;
    let executed_at = match &entry.executed_at {
        Some(raw) => to_rfc3339(raw).ok_or_else(|| format!("tanggal tidak terbaca: {raw}"))?,
        None => chrono::Utc::now().to_rfc3339(),
    };
    Ok(ConfirmPayload {
        account_id,
        instrument_id,
        entry_type: entry.entry_type,
        executed_at,
        quantity,
        price_native,
        fee_native: entry.fee_native,
        currency,
        fx_to_idr: None,
        fx_to_usd: None,
        note: entry.note,
        amount_native: entry.amount_native,
    })
}
```

- [ ] **Step 2: Move the unit tests into `ingestion::review`'s test module**

In `backend/src/ingestion/review.rs`'s `#[cfg(test)] mod tests`, add the helper, constants, and tests (copied verbatim from `telegram/mod.rs` so coverage travels with the code):

```rust
fn review_item(payload_json: &str) -> crate::repo::review_items::ReviewItemRow {
    crate::repo::review_items::ReviewItemRow {
        id: 42,
        batch_id: "tg-1".into(),
        source_kind: "image".into(),
        source_filename: "telegram-photo.jpg".into(),
        source_path: "".into(),
        doc_type: "txn_history".into(),
        status: "pending".into(),
        needs_attention: 0,
        payload_json: payload_json.into(),
        raw_llm_json: "{}".into(),
        suggested_instrument_id: Some(9),
        suggested_account_id: Some(2),
        created_txn_id: None,
        created_at: "2026-06-05T00:00:00Z".into(),
        confirmed_at: None,
    }
}

const FULL_PAYLOAD: &str = r#"{
    "entry_type": "buy", "symbol": "BTC", "quantity": "0.00128248",
    "price_native": "1169608882", "fee_native": "0", "currency": "IDR",
    "executed_at": "2026-06-04", "confidence": 0.95
}"#;

const AMOUNT_ONLY_PAYLOAD: &str = r#"{
    "entry_type": "buy", "instrument_name": "Sucorinvest Bond Fund",
    "amount_native": "13000000", "currency": "IDR", "confidence": 0.72
}"#;

#[test]
fn coerces_dates_to_rfc3339() {
    assert_eq!(to_rfc3339("2026-06-04T11:32:00Z").as_deref(), Some("2026-06-04T11:32:00Z"));
    assert_eq!(to_rfc3339("2026-06-04T11:32").as_deref(), Some("2026-06-04T11:32:00Z"));
    assert_eq!(to_rfc3339("2026-06-04").as_deref(), Some("2026-06-04T00:00:00Z"));
    assert_eq!(to_rfc3339("kemarin"), None);
}

#[test]
fn full_items_build_a_confirm_payload() {
    let payload = build_confirm_payload(&review_item(FULL_PAYLOAD)).expect("confirmable");
    assert_eq!(payload.account_id, 2);
    assert_eq!(payload.instrument_id, 9);
    assert_eq!(payload.entry_type, "buy");
    assert_eq!(payload.quantity, "0.00128248");
    assert_eq!(payload.executed_at, "2026-06-04T00:00:00Z");
    assert_eq!(payload.currency, "IDR");
}

#[test]
fn attention_items_are_not_confirmable() {
    let mut item = review_item(FULL_PAYLOAD);
    item.needs_attention = 1;
    assert!(build_confirm_payload(&item).is_err());
}

#[test]
fn items_without_suggestions_are_not_confirmable() {
    let mut item = review_item(FULL_PAYLOAD);
    item.suggested_account_id = None;
    assert!(build_confirm_payload(&item).is_err());

    let mut item = review_item(FULL_PAYLOAD);
    item.suggested_instrument_id = None;
    assert!(build_confirm_payload(&item).is_err());
}

#[test]
fn items_missing_core_fields_are_not_confirmable() {
    let payload = r#"{ "entry_type": "buy", "symbol": "BTC", "confidence": 0.95 }"#;
    assert!(build_confirm_payload(&review_item(payload)).is_err());
}

#[test]
fn amount_only_fund_items_build_a_confirm_payload() {
    let payload =
        build_confirm_payload(&review_item(AMOUNT_ONLY_PAYLOAD)).expect("confirmable");
    assert_eq!(payload.quantity, "");
    assert_eq!(payload.price_native, "");
    assert_eq!(payload.amount_native.as_deref(), Some("13000000"));
    assert_eq!(payload.currency, "IDR");
}

#[test]
fn amount_only_dividend_is_not_confirmable() {
    let payload = r#"{ "entry_type": "dividend", "amount_native": "100000", "currency": "IDR", "confidence": 0.9 }"#;
    assert!(build_confirm_payload(&review_item(payload)).is_err());
}
```

- [ ] **Step 3: Delete the moved code from `telegram/mod.rs` and repoint call sites**

In `backend/src/telegram/mod.rs`: delete the `to_rfc3339` fn (lines ~80-93) and the `build_confirm_payload` fn (lines ~95-140). Keep `item_summary`, `fmt_payload_num`, `pick_attachment`, etc. Replace the two call sites:
- In `send_review_prompts`: `match build_confirm_payload(item) {` → `match crate::ingestion::review::build_confirm_payload(item) {`
- In `confirm_item`: `let payload = build_confirm_payload(&item)` → `let payload = crate::ingestion::review::build_confirm_payload(&item)`

Then delete from the `telegram` test module: `review_item`, `FULL_PAYLOAD`, `AMOUNT_ONLY_PAYLOAD` (now in review.rs), and the tests `coerces_dates_to_rfc3339`, `full_items_build_a_confirm_payload`, `attention_items_are_not_confirmable`, `items_without_suggestions_are_not_confirmable`, `items_missing_core_fields_are_not_confirmable`, `amount_only_fund_items_build_a_confirm_payload`, `amount_only_dividend_is_not_confirmable`.

KEEP in `telegram` tests (they use `item_summary`, not the moved fns): `amount_only_summary_shows_the_nominal` and `summary_shows_the_extracted_details`. These reference `review_item`/`FULL_PAYLOAD`/`AMOUNT_ONLY_PAYLOAD`, so re-add private copies of those three helpers to the telegram test module (same bodies as Step 2) so the kept summary tests still compile.

- [ ] **Step 4: Build and run the moved tests**

Run: `cargo test --bin portfolio-tracker ingestion::review:: 2>&1 | tail -20`
Expected: PASS, including the 7 moved tests.

Run: `cargo test --bin portfolio-tracker telegram:: 2>&1 | tail -20`
Expected: PASS, including `amount_only_summary_shows_the_nominal` and `summary_shows_the_extracted_details`.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/review.rs backend/src/telegram/mod.rs
git commit -m "refactor: move build_confirm_payload into ingestion::review"
```

---

### Task 2: Add `review_items::set_suggestions`

**Files:**
- Modify: `backend/src/repo/review_items.rs`

- [ ] **Step 1: Write the failing test**

In `backend/src/repo/review_items.rs`'s `#[cfg(test)] mod tests`, add:

```rust
#[tokio::test]
async fn set_suggestions_fills_only_provided_ids() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let item = create(&db, &NewReviewItem {
        batch_id: "b1", source_kind: "image", source_filename: "f.jpg",
        source_path: "", doc_type: "txn_history", needs_attention: false,
        payload_json: "{}", raw_llm_json: "{}",
        suggested_instrument_id: Some(7), suggested_account_id: None,
    }).await.unwrap();

    let updated = set_suggestions(&db, item.id, Some(3), None).await.unwrap();
    assert_eq!(updated.suggested_account_id, Some(3));
    assert_eq!(updated.suggested_instrument_id, Some(7), "instrument left unchanged");

    let updated = set_suggestions(&db, item.id, None, Some(9)).await.unwrap();
    assert_eq!(updated.suggested_account_id, Some(3), "account left unchanged");
    assert_eq!(updated.suggested_instrument_id, Some(9));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bin portfolio-tracker repo::review_items::tests::set_suggestions_fills_only_provided_ids 2>&1 | tail -20`
Expected: FAIL — `cannot find function set_suggestions`.

- [ ] **Step 3: Implement `set_suggestions`**

In `backend/src/repo/review_items.rs`, after `update_payload`:

```rust
/// Fill in a previously-unmatched account and/or instrument suggestion. A
/// `None` argument leaves that column untouched. Returns the refreshed row.
pub async fn set_suggestions(
    db: &Db,
    id: i64,
    account_id: Option<i64>,
    instrument_id: Option<i64>,
) -> anyhow::Result<ReviewItemRow> {
    if let Some(aid) = account_id {
        sqlx::query("UPDATE review_item SET suggested_account_id = ? WHERE id = ?")
            .bind(aid).bind(id).execute(db).await?;
    }
    if let Some(iid) = instrument_id {
        sqlx::query("UPDATE review_item SET suggested_instrument_id = ? WHERE id = ?")
            .bind(iid).bind(id).execute(db).await?;
    }
    get(db, id).await
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --bin portfolio-tracker repo::review_items::tests::set_suggestions_fills_only_provided_ids 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/review_items.rs
git commit -m "feat: add review_items::set_suggestions to fill resolved ids"
```

---

### Task 3: Add the `optional_id` dispatcher helper + `reject_review` tool

**Files:**
- Modify: `backend/src/assistant/dispatcher.rs`
- Modify: `backend/src/assistant/tools.rs`

> Doing `reject_review` first because it is the smallest end-to-end tool and exercises the schema + dispatch wiring the later tools reuse.

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, inside the `json!([ ... ])` array, after the `cancel_event` object (and its trailing comma), add:

```rust
        ,
        {
            "name": "reject_review",
            "description": "Reject (discard) a pending ingest review item so it is not turned into a transaction. Get the id from list_pending_reviews.",
            "input_schema": {
                "type": "object",
                "properties": { "review_id": { "type": "integer", "description": "Review item id" } },
                "required": ["review_id"]
            }
        }
```

- [ ] **Step 2: Write the failing dispatcher test**

In `backend/src/assistant/dispatcher.rs`'s test module, add a shared test helper (used by this and later tasks) and the test:

```rust
async fn seed_pending_item(db: &Db, account_id: Option<i64>, instrument_id: Option<i64>) -> i64 {
    crate::repo::review_items::create(db, &crate::repo::review_items::NewReviewItem {
        batch_id: "b1", source_kind: "image", source_filename: "f.jpg",
        source_path: "", doc_type: "txn_history", needs_attention: false,
        payload_json: r#"{ "entry_type": "buy", "symbol": "BTC", "quantity": "1",
            "price_native": "100", "fee_native": "0", "currency": "IDR",
            "executed_at": "2026-06-04", "confidence": 0.95 }"#,
        raw_llm_json: "{}",
        suggested_instrument_id: instrument_id, suggested_account_id: account_id,
    }).await.unwrap().id
}

#[tokio::test]
async fn reject_review_marks_item_rejected() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let id = seed_pending_item(&db, None, None).await;
    let out = dispatch(&db, "reject_review", &serde_json::json!({ "review_id": id })).await.unwrap();
    assert!(out.contains("ditolak"), "{out}");
    let item = crate::repo::review_items::get(&db, id).await.unwrap();
    assert_eq!(item.status, "rejected");
}
```

- [ ] **Step 3: Run test to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::reject_review_marks_item_rejected 2>&1 | tail -20`
Expected: FAIL — `unknown tool: reject_review`.

- [ ] **Step 4: Implement the helper, handler, and dispatch arm**

In `backend/src/assistant/dispatcher.rs`, add the helper next to `id_arg`:

```rust
/// Optional integer argument: absent/null → None; present-but-not-integer is
/// an error so the model self-corrects instead of assuming a silent default.
fn optional_id(input: &serde_json::Value, key: &str) -> Result<Option<i64>, String> {
    match input.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => Ok(Some(v.as_i64().ok_or_else(|| format!("{key} must be an integer, got {v}"))?)),
    }
}
```

Add the handler:

```rust
async fn reject_review(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let review_id = id_arg(input, "review_id")?;
    crate::ingestion::review::reject(db, review_id)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(format!("review #{review_id} ditolak"))
}
```

Add the dispatch arm inside `match name`, after `"cancel_event" => ...`:

```rust
        "reject_review" => reject_review(db, input).await,
```

- [ ] **Step 5: Run test to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::reject_review_marks_item_rejected 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat: add reject_review assistant tool"
```

---

### Task 4: Add `list_accounts` and `create_account` tools

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Add the schemas**

In `backend/src/assistant/tools.rs`, after the `reject_review` object, add:

```rust
        ,
        {
            "name": "list_accounts",
            "description": "List the owner's investment accounts (id, name, type). Use before create_account to reuse an existing account, and to find an account_id for confirm_review.",
            "input_schema": { "type": "object", "properties": {} }
        },
        {
            "name": "create_account",
            "description": "Create a new investment account (e.g. a broker like Nanovest the owner doesn't have yet). Always ask the user to confirm before calling — this writes data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Account name, e.g. Nanovest" },
                    "account_type": { "type": "string", "description": "Type, e.g. broker, exchange, bank" },
                    "native_currency": { "type": "string", "description": "ISO currency code, e.g. IDR or USD" },
                    "institution": { "type": "string", "description": "Optional institution name" },
                    "note": { "type": "string", "description": "Optional note" }
                },
                "required": ["name", "account_type", "native_currency"]
            }
        }
```

- [ ] **Step 2: Write the failing tests**

In `backend/src/assistant/dispatcher.rs` test module:

```rust
#[tokio::test]
async fn create_account_then_list_shows_it() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let out = dispatch(&db, "create_account", &serde_json::json!({
        "name": "Nanovest", "account_type": "broker", "native_currency": "IDR"
    })).await.unwrap();
    assert!(out.contains("Nanovest"), "{out}");

    let listed = dispatch(&db, "list_accounts", &serde_json::json!({})).await.unwrap();
    assert!(listed.contains("Nanovest"), "{listed}");
    assert!(listed.contains("broker"), "{listed}");
}

#[tokio::test]
async fn create_account_requires_name() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let err = dispatch(&db, "create_account", &serde_json::json!({
        "account_type": "broker", "native_currency": "IDR"
    })).await.unwrap_err();
    assert!(err.contains("name"), "{err}");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_account 2>&1 | tail -20`
Expected: FAIL — `unknown tool: create_account`.

- [ ] **Step 4: Implement handlers and dispatch arms**

In `backend/src/assistant/dispatcher.rs`, add handlers:

```rust
async fn list_accounts(db: &Db) -> Result<String, String> {
    let accounts = crate::repo::accounts::list(db).await.map_err(|e| format!("db error: {e}"))?;
    if accounts.is_empty() {
        return Ok("no accounts yet".into());
    }
    let mut out = String::new();
    for a in accounts {
        out.push_str(&format!("#{} {} ({})\n", a.id, a.name, a.account_type));
    }
    Ok(out)
}

async fn create_account(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let name = str_arg(input, "name").ok_or("missing required argument 'name'")?;
    let account_type =
        str_arg(input, "account_type").ok_or("missing required argument 'account_type'")?;
    let native_currency =
        str_arg(input, "native_currency").ok_or("missing required argument 'native_currency'")?;
    let account = crate::repo::accounts::create(db, &crate::repo::accounts::NewAccount {
        name: name.to_string(),
        account_type: account_type.to_string(),
        institution: str_arg(input, "institution").map(str::to_string),
        native_currency: native_currency.to_string(),
        note: str_arg(input, "note").map(str::to_string),
    })
    .await
    .map_err(|e| format!("db error: {e}"))?;
    Ok(format!("created account #{} '{}'", account.id, account.name))
}
```

Add dispatch arms after `"reject_review" => ...`:

```rust
        "list_accounts" => list_accounts(db).await,
        "create_account" => create_account(db, input).await,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_account 2>&1 | tail -20`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat: add list_accounts and create_account assistant tools"
```

---

### Task 5: Add `list_pending_reviews` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, after the `create_account` object:

```rust
        ,
        {
            "name": "list_pending_reviews",
            "description": "List ingest review items awaiting confirmation (from photos/PDFs the owner sent). Each line shows the review id, type, instrument, account, amounts, date, and flags items whose account or instrument isn't recognized yet. Use when the user asks to enter/confirm a transaction they sent.",
            "input_schema": { "type": "object", "properties": {} }
        }
```

- [ ] **Step 2: Write the failing test**

In `backend/src/assistant/dispatcher.rs` test module:

```rust
#[tokio::test]
async fn list_pending_reviews_flags_unknown_account() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
        symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
        native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
        decimals: Some(8), note: None,
    }).await.unwrap();
    let id = seed_pending_item(&db, None, Some(instrument.id)).await;

    let out = dispatch(&db, "list_pending_reviews", &serde_json::json!({})).await.unwrap();
    assert!(out.contains(&format!("#{id}")), "{out}");
    assert!(out.contains("BTC"), "instrument shown: {out}");
    assert!(out.contains("belum dikenali"), "unknown account flagged: {out}");
    assert!(out.contains("perlu dilengkapi"), "blocker noted: {out}");
}

#[tokio::test]
async fn list_pending_reviews_empty_is_explicit() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let out = dispatch(&db, "list_pending_reviews", &serde_json::json!({})).await.unwrap();
    assert!(out.contains("no pending"), "{out}");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_pending_reviews 2>&1 | tail -20`
Expected: FAIL — `unknown tool: list_pending_reviews`.

- [ ] **Step 4: Implement the handler and dispatch arm**

In `backend/src/assistant/dispatcher.rs`:

```rust
async fn list_pending_reviews(db: &Db) -> Result<String, String> {
    let items = crate::repo::review_items::list_by_status(db, "pending")
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if items.is_empty() {
        return Ok("no pending review items".into());
    }
    let mut out = String::new();
    for item in items {
        let entry: Option<crate::ingestion::extract::ExtractedEntry> =
            serde_json::from_str(&item.payload_json).ok();
        let etype = entry.as_ref().map(|e| e.entry_type.as_str()).unwrap_or("?");
        let instrument = match item.suggested_instrument_id {
            Some(iid) => crate::repo::instruments::get(db, iid)
                .await
                .ok()
                .map(|i| format!("{} ({})", i.symbol, i.name))
                .unwrap_or_else(|| "❓ belum dikenali".into()),
            None => "❓ belum dikenali".into(),
        };
        let account = match item.suggested_account_id {
            Some(aid) => crate::repo::accounts::get(db, aid)
                .await
                .ok()
                .map(|a| a.name)
                .unwrap_or_else(|| "❓ belum dikenali".into()),
            None => "❓ belum dikenali".into(),
        };
        out.push_str(&format!("#{} {} — instrumen: {instrument} — akun: {account}", item.id, etype));
        if let Some(e) = &entry {
            if let (Some(q), Some(p)) = (&e.quantity, &e.price_native) {
                out.push_str(&format!(" — {q} @ {p}"));
            } else if let Some(a) = &e.amount_native {
                out.push_str(&format!(" — nominal {a}"));
            }
            if let Some(d) = &e.executed_at {
                out.push_str(&format!(" — {d}"));
            }
        }
        if item.suggested_account_id.is_none() || item.suggested_instrument_id.is_none() {
            out.push_str(" [perlu dilengkapi sebelum konfirmasi]");
        }
        out.push('\n');
    }
    Ok(out)
}
```

Add the dispatch arm after `"create_account" => ...`:

```rust
        "list_pending_reviews" => list_pending_reviews(db).await,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_pending_reviews 2>&1 | tail -20`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat: add list_pending_reviews assistant tool"
```

---

### Task 6: Add `confirm_review` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Add the schema**

In `backend/src/assistant/tools.rs`, after the `list_pending_reviews` object:

```rust
        ,
        {
            "name": "confirm_review",
            "description": "Confirm a pending review item, turning it into a transaction. Pass account_id and/or instrument_id to fill in anything flagged 'belum dikenali' (create the account first with create_account if needed). Always ask the user to confirm before calling — this writes a transaction.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "review_id": { "type": "integer", "description": "Review item id from list_pending_reviews" },
                    "account_id": { "type": "integer", "description": "Optional account id to set when the item's account is unknown" },
                    "instrument_id": { "type": "integer", "description": "Optional instrument id to set when the item's instrument is unknown" }
                },
                "required": ["review_id"]
            }
        }
```

- [ ] **Step 2: Write the failing tests**

In `backend/src/assistant/dispatcher.rs` test module:

```rust
#[tokio::test]
async fn confirm_review_with_account_override_creates_txn() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
        symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
        native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
        decimals: Some(8), note: None,
    }).await.unwrap();
    let account = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
        name: "Nanovest".into(), account_type: "broker".into(), institution: None,
        native_currency: "IDR".into(), note: None,
    }).await.unwrap();
    // Account unknown at ingest time; instrument matched.
    let id = seed_pending_item(&db, None, Some(instrument.id)).await;

    let out = dispatch(&db, "confirm_review", &serde_json::json!({
        "review_id": id, "account_id": account.id
    })).await.unwrap();
    assert!(out.contains("dibuat"), "{out}");

    let item = crate::repo::review_items::get(&db, id).await.unwrap();
    assert_eq!(item.status, "confirmed");
    assert!(item.created_txn_id.is_some());
}

#[tokio::test]
async fn confirm_review_still_incomplete_returns_reason() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    // No account supplied and none suggested → not confirmable.
    let id = seed_pending_item(&db, None, Some(1)).await;
    let err = dispatch(&db, "confirm_review", &serde_json::json!({ "review_id": id })).await.unwrap_err();
    assert!(err.contains("akun belum dikenali"), "{err}");
    let item = crate::repo::review_items::get(&db, id).await.unwrap();
    assert_eq!(item.status, "pending", "must not confirm");
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::confirm_review 2>&1 | tail -20`
Expected: FAIL — `unknown tool: confirm_review`.

- [ ] **Step 4: Implement the handler and dispatch arm**

In `backend/src/assistant/dispatcher.rs`:

```rust
async fn confirm_review(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let review_id = id_arg(input, "review_id")?;
    let account_id = optional_id(input, "account_id")?;
    let instrument_id = optional_id(input, "instrument_id")?;
    if account_id.is_some() || instrument_id.is_some() {
        crate::repo::review_items::set_suggestions(db, review_id, account_id, instrument_id)
            .await
            .map_err(|e| format!("db error: {e}"))?;
    }
    let item = crate::repo::review_items::get(db, review_id)
        .await
        .map_err(|_| format!("review #{review_id} not found"))?;
    let payload = crate::ingestion::review::build_confirm_payload(&item)?;
    let txn_id = crate::ingestion::review::confirm(db, review_id, &payload)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(format!("transaksi #{txn_id} dibuat dari review #{review_id}"))
}
```

Add the dispatch arm after `"list_pending_reviews" => ...`:

```rust
        "confirm_review" => confirm_review(db, input).await,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::confirm_review 2>&1 | tail -20`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs
git commit -m "feat: add confirm_review assistant tool"
```

---

### Task 7: Update tool-schema tests for the five new names

**Files:**
- Modify: `backend/src/assistant/tools.rs`

- [ ] **Step 1: Update the `defines_all_tools_with_schemas` expectation**

In `backend/src/assistant/tools.rs`, extend the `assert_eq!(names, vec![...])` to append the five new names in dispatch order:

```rust
        assert_eq!(
            names,
            vec![
                "create_todo", "list_todos", "complete_todo",
                "create_reminder", "list_reminders", "cancel_reminder",
                "get_portfolio_summary", "search_memory", "remember",
                "create_event", "list_events", "cancel_event",
                "reject_review", "list_accounts", "create_account",
                "list_pending_reviews", "confirm_review",
            ]
        );
```

- [ ] **Step 2: Add a required-fields assertion**

Inside `required_fields_are_marked`, add:

```rust
        assert_eq!(find("reject_review")["input_schema"]["required"], serde_json::json!(["review_id"]));
        assert_eq!(find("confirm_review")["input_schema"]["required"], serde_json::json!(["review_id"]));
        assert_eq!(
            find("create_account")["input_schema"]["required"],
            serde_json::json!(["name", "account_type", "native_currency"])
        );
```

- [ ] **Step 3: Run the schema tests**

Run: `cargo test --bin portfolio-tracker assistant::tools:: 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/src/assistant/tools.rs
git commit -m "test: cover the five new assistant tool schemas"
```

---

### Task 8: Teach the system prompt the resolve-then-confirm flow

**Files:**
- Modify: `backend/src/assistant/agent.rs`

- [ ] **Step 1: Write the failing prompt test**

In `backend/src/assistant/agent.rs` test module, add:

```rust
#[test]
fn system_prompt_mentions_the_review_tools() {
    let prompt = system_prompt("2026-06-12T10:00:00+07:00");
    assert!(prompt.contains("list_pending_reviews"), "{prompt}");
    assert!(prompt.contains("confirm_review"), "{prompt}");
    assert!(prompt.contains("create_account"), "{prompt}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_the_review_tools 2>&1 | tail -20`
Expected: FAIL — assertion: prompt missing `list_pending_reviews`.

- [ ] **Step 3: Extend the `SYSTEM` const**

In `backend/src/assistant/agent.rs`, append to the `SYSTEM` string literal (before the closing `";`), after the agenda sentence:

```rust
const SYSTEM: &str = "...existing text... and cancel_event. \
 You can also enter transactions the owner sent as photos/PDFs: when they ask \
to 'masukin transaksi tadi' or to confirm one, call list_pending_reviews. If an \
item's account or instrument shows 'belum dikenali', call list_accounts to find \
a match; if none fits, ask the user before calling create_account, then \
confirm_review with the account_id (and instrument_id if needed). Unlike \
todos/reminders, ALWAYS ask the user to confirm before create_account or \
confirm_review — these write financial data that can't be silently undone. Use \
reject_review to discard an item.";
```

(Keep the existing text intact; only the trailing review-flow sentences are new. Replace `...existing text... and cancel_event.` with the real current ending of the literal.)

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_the_review_tools 2>&1 | tail -20`
Expected: PASS.

- [ ] **Step 5: Run the full backend test suite**

Run: `cargo test --bin portfolio-tracker 2>&1 | tail -20`
Expected: PASS — no regressions across assistant, telegram, ingestion, repo.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/agent.rs
git commit -m "feat: teach assistant the review resolve-then-confirm flow"
```

---

## Self-Review Notes

- **Spec coverage:** shared helper → Task 1; `set_suggestions` → Task 2; five tools → Tasks 3–6; system prompt + confirm-before-write rule → Task 8; tests → each task + Task 7. All spec sections mapped.
- **Type consistency:** `build_confirm_payload(&ReviewItemRow) -> Result<ConfirmPayload, String>`, `set_suggestions(db, id, Option<i64>, Option<i64>)`, `confirm(db, id, &ConfirmPayload) -> i64`, `NewAccount`/`NewInstrument`/`NewReviewItem` field names match the repo definitions read from source. Dispatch arm names match schema `name` values and the Task 7 ordered list.
- **Out of scope confirmed absent:** no `create_instrument`, no fuzzy matching, no Telegram inline account picker.
