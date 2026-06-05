# Amount-Only Mutual Fund Buys Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Bibit-style mutual fund purchases (IDR amount only, no units/NAV/date) confirmable end-to-end: extraction → review → ledger transaction, using the existing `quantity = amount, price_native = "1"` convention.

**Architecture:** No schema change. The confirm flow maps amount-only buy/sell entries onto the convention already used by connector deposits (`service/sync.rs`), so cost basis, valuation (at-cost via `avg_cost` fallback), XIRR, and insights all work unchanged. Supporting changes: extraction prompt separates Bibit goal → `account_hint` from fund → `instrument_name`; `needs_attention` accepts name+amount as complete; instrument matching falls back to exact name match; the review UI gains an Amount field.

**Tech Stack:** Rust (axum, sqlx, rust_decimal, anyhow), React + TypeScript (zod, react-query, vitest + msw + testing-library).

**Spec:** `docs/superpowers/specs/2026-06-05-amount-only-mutual-fund-buys-design.md`

**Branch:** `feat/amount-only-fund-buys`

**Important context for the implementer:**
- The live review UI is `ReviewCard` inside `frontend/src/pages/ImportPage.tsx`. `frontend/src/components/ReviewRow.tsx` is a legacy component used only by its own test — do NOT modify it.
- The confirm API endpoint (`backend/src/api/ingest.rs:57`) already maps all confirm errors to HTTP 400, so only the error *message* needs improving, not the status code.
- Backend tests run against `sqlite::memory:`; no external services needed.
- All backend commands run from `backend/`, all frontend commands from `frontend/`.

---

### Task 1: `needs_attention()` accepts name + amount as complete

**Files:**
- Modify: `backend/src/ingestion/ingest.rs:16-23` (function) and the `tests` module in the same file

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `backend/src/ingestion/ingest.rs` (after the existing `complete_high_confidence_ok` test):

```rust
    /// Bibit-style mutual fund buy: name + IDR amount, no symbol/units/NAV.
    fn fund_entry() -> ExtractedEntry {
        ExtractedEntry { entry_type:"buy".into(), symbol:None,
            instrument_name:Some("Sucorinvest Bond Fund".into()),
            quantity:None, price_native:None, fee_native:None, currency:Some("IDR".into()),
            executed_at:None, account_hint:Some("Pendidikan Noah".into()), note:None,
            confidence:0.72, amount_native:Some("13000000".into()), force_attention:false }
    }

    #[test]
    fn amount_only_fund_buy_with_name_is_complete() {
        assert!(!needs_attention(&fund_entry()));
    }
    #[test]
    fn amount_only_without_any_name_needs_attention() {
        let mut e = fund_entry();
        e.instrument_name = None;
        assert!(needs_attention(&e));
    }
    #[test]
    fn name_without_quantity_or_amount_needs_attention() {
        let mut e = fund_entry();
        e.amount_native = None;
        assert!(needs_attention(&e));
    }
```

- [ ] **Step 2: Run tests to verify the new ones fail**

Run: `cargo test -p portfolio-tracker ingestion::ingest::tests`
Expected: `amount_only_fund_buy_with_name_is_complete` FAILS (assertion failed); the other two new tests pass already; existing tests pass.

(If the package name differs, plain `cargo test ingestion::ingest::tests` from `backend/` works.)

- [ ] **Step 3: Implement the new completeness rule**

Replace `needs_attention` in `backend/src/ingestion/ingest.rs` (currently lines 15-23) with:

```rust
/// Decide if an entry needs human attention (low confidence or missing core fields).
pub fn needs_attention(e: &ExtractedEntry) -> bool {
    if e.force_attention { return true; }
    if e.confidence < 0.6 { return true; }
    match e.entry_type.as_str() {
        "deposit" | "withdrawal" | "dividend" | "interest" => e.quantity.is_none() && e.price_native.is_none(),
        // Trades are complete with either a symbol or a name (mutual funds have no
        // ticker) and either units or a total amount (amount-only fund buys).
        _ => {
            let has_name = e.symbol.is_some() || e.instrument_name.is_some();
            let has_size = e.quantity.is_some() || e.amount_native.is_some();
            !has_name || !has_size
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test ingestion::ingest::tests`
Expected: all PASS, including the pre-existing `missing_symbol_needs_attention` (its entry has neither `instrument_name` nor `amount_native`... it has `quantity` but no name → still flagged) and `complete_high_confidence_ok`.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/ingest.rs
git commit -m "feat(ingest): treat name+amount entries as complete in needs_attention"
```

---

### Task 2: Confirm flow maps amount-only buy/sell to quantity=amount, price=1

**Files:**
- Modify: `backend/src/ingestion/review.rs` (`ConfirmPayload`, `confirm()`, tests)

- [ ] **Step 1: Add `amount_native` to `ConfirmPayload`**

In `backend/src/ingestion/review.rs`, add to the `ConfirmPayload` struct (after the `note` field):

```rust
    /// Total transaction value (e.g. Bibit mutual fund buys show only an IDR
    /// amount). Used when quantity/price are absent: quantity = amount, price = 1.
    #[serde(default)]
    pub amount_native: Option<String>,
```

- [ ] **Step 2: Update the three existing test payload literals**

The three `ConfirmPayload { ... }` literals in the tests module (in `confirm_inserts_ledger_txn_and_marks_confirmed`, `double_confirm_is_rejected_and_inserts_one_txn`, `reject_after_confirm_is_refused`) each need the new field. Add `amount_native:None,` after `note:None` in each, e.g.:

```rust
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-01-02T00:00:00Z".into(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:None,
        };
```

- [ ] **Step 3: Write the failing tests**

Add to the tests module in `backend/src/ingestion/review.rs`:

```rust
    #[tokio::test]
    async fn confirm_amount_only_buy_uses_amount_as_quantity_at_price_one() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some("13000000".into()),
        };
        let txn_id = confirm(&db, item.id, &payload).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].quantity, dec!(13000000));
        assert_eq!(txns[0].price_native, dec!(1));
        assert_eq!(review_items::get(&db, item.id).await.unwrap().created_txn_id, Some(txn_id));
    }

    #[tokio::test]
    async fn confirm_amount_only_sell_also_maps() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"asdc.jpeg", source_path:"p",
            doc_type:"txn_history", needs_attention:false, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"sell".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some("5000000".into()),
        };
        confirm(&db, item.id, &payload).await.unwrap();
        let txns = transactions::list_all(&db).await.unwrap();
        assert_eq!(txns[0].quantity, dec!(5000000));
        assert_eq!(txns[0].price_native, dec!(1));
        assert_eq!(txns[0].txn_type, crate::domain::models::TxnType::Sell);
    }

    #[tokio::test]
    async fn confirm_without_quantity_price_or_amount_errors_clearly() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:true, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"buy".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:None,
        };
        let err = confirm(&db, item.id, &payload).await.unwrap_err();
        assert!(err.to_string().contains("amount_native"), "unhelpful message: {err}");
        // nothing persisted, item still pending
        assert_eq!(transactions::list_all(&db).await.unwrap().len(), 0);
        assert_eq!(review_items::get(&db, item.id).await.unwrap().status, "pending");
    }

    #[tokio::test]
    async fn confirm_amount_only_dividend_is_refused() {
        // The amount->quantity convention applies to buy/sell only.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let (account_id, instrument_id) = seed(&db).await;
        let item = review_items::create(&db, &review_items::NewReviewItem {
            batch_id:"b", source_kind:"image", source_filename:"f.png", source_path:"p",
            doc_type:"txn_history", needs_attention:true, payload_json:"{}", raw_llm_json:"{}",
            suggested_instrument_id:Some(instrument_id), suggested_account_id:Some(account_id),
        }).await.unwrap();
        let payload = ConfirmPayload {
            account_id, instrument_id, entry_type:"dividend".into(),
            executed_at:"2026-06-05T00:00:00Z".into(), quantity:"".into(), price_native:"".into(),
            fee_native:None, currency:"IDR".into(), fx_to_idr:None, fx_to_usd:None, note:None,
            amount_native:Some("100000".into()),
        };
        assert!(confirm(&db, item.id, &payload).await.is_err());
        assert_eq!(transactions::list_all(&db).await.unwrap().len(), 0);
    }
```

Note: if `txns[0].txn_type` is not comparable like that, check `backend/src/domain/models.rs` for the `TxnType` enum — it derives `PartialEq` for the other tests' usage. If it doesn't, assert via `format!("{:?}", txns[0].txn_type) == "Sell"` instead.

- [ ] **Step 4: Run tests to verify the new ones fail**

Run: `cargo test ingestion::review::tests`
Expected: the two amount-only mapping tests FAIL with `bad decimal ''` (from `repo::dec` on the empty string); `confirm_without_quantity_price_or_amount_errors_clearly` FAILS on the message assertion; existing tests pass.

- [ ] **Step 5: Implement the amount-only mapping in `confirm()`**

In `backend/src/ingestion/review.rs`, inside `confirm()` after the FX defaulting (after the `fx_to_usd` line, before `let nt = ...`), add:

```rust
    // Amount-only mutual fund trades (e.g. Bibit "Order" screenshots) carry a
    // total IDR amount but no units/NAV — and never will (value-based tracking).
    // Record them with the same convention as connector deposits (service/sync.rs):
    // quantity = amount, price = 1, so cost basis equals the invested amount and
    // the position is valued at cost via the avg_cost fallback.
    let (quantity, price_native) = if p.quantity.trim().is_empty() && p.price_native.trim().is_empty() {
        let amount = p.amount_native.as_deref().map(str::trim).filter(|a| !a.is_empty());
        match (amount, p.entry_type.as_str()) {
            (Some(a), "buy" | "sell") => (a.to_string(), "1".to_string()),
            _ => return Err(anyhow::anyhow!(
                "quantity/price or amount_native is required for a {} entry",
                p.entry_type
            )),
        }
    } else {
        (p.quantity.clone(), p.price_native.clone())
    };
```

Then change the `NewTransaction` construction to use the mapped values:

```rust
        quantity,
        price_native,
```

(replacing `quantity: p.quantity.clone(),` and `price_native: p.price_native.clone(),`).

The mapped amount string still goes through `crate::repo::dec()` validation inside `transactions::create`, so a malformed `amount_native` is rejected before anything persists.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test ingestion::review::tests`
Expected: all PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/ingestion/review.rs
git commit -m "feat(ingest): confirm amount-only fund buys as quantity=amount at price 1"
```

---

### Task 3: Instrument suggestion falls back to exact name match

**Files:**
- Modify: `backend/src/ingestion/matching.rs` (new function + tests)
- Modify: `backend/src/ingestion/ingest.rs:2-3,98` (call site)
- Modify: `backend/src/api/ingest.rs:4,82` (CSV call site)

- [ ] **Step 1: Write the failing test**

Add to the tests module in `backend/src/ingestion/matching.rs`:

```rust
    #[tokio::test]
    async fn suggest_instrument_for_entry_falls_back_to_name() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument {
            symbol:"SBF".into(), name:"Sucorinvest Bond Fund".into(),
            instrument_type:"mutual_fund".into(), native_currency:"IDR".into(),
            category_id:None, price_source:"manual".into(), decimals:Some(4), note:None,
        }).await.unwrap();
        // mutual funds: no symbol extracted -> exact name match, case-insensitive
        assert_eq!(suggest_instrument_for_entry(&db, None, Some("sucorinvest bond fund")).await.unwrap(), Some(ins.id));
        // symbol match still takes precedence
        assert_eq!(suggest_instrument_for_entry(&db, Some("sbf"), None).await.unwrap(), Some(ins.id));
        // unmatched symbol falls through to the name
        assert_eq!(suggest_instrument_for_entry(&db, Some("XXXX"), Some("Sucorinvest Bond Fund")).await.unwrap(), Some(ins.id));
        // nothing matches
        assert_eq!(suggest_instrument_for_entry(&db, Some("XXXX"), Some("Unknown Fund")).await.unwrap(), None);
        assert_eq!(suggest_instrument_for_entry(&db, None, None).await.unwrap(), None);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test ingestion::matching::tests`
Expected: compile error — `suggest_instrument_for_entry` not found.

- [ ] **Step 3: Implement the function**

Add to `backend/src/ingestion/matching.rs` (after `suggest_instrument`):

```rust
/// Suggest an instrument for an extracted entry: exact symbol match first, then
/// exact name match. Mutual funds (e.g. Bibit) carry a fund name but no ticker,
/// so the name fallback is what makes their suggestions work at all.
pub async fn suggest_instrument_for_entry(db: &Db, symbol: Option<&str>, name: Option<&str>) -> anyhow::Result<Option<i64>> {
    if let Some(s) = symbol {
        if let Some(id) = suggest_instrument(db, s).await? {
            return Ok(Some(id));
        }
    }
    if let Some(n) = name {
        let row = sqlx::query_as::<_, (i64,)>("SELECT id FROM instrument WHERE LOWER(name) = LOWER(?) LIMIT 1")
            .bind(n).fetch_optional(db).await?;
        return Ok(row.map(|(id,)| id));
    }
    Ok(None)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test ingestion::matching::tests`
Expected: all PASS.

- [ ] **Step 5: Switch both call sites to the new function**

In `backend/src/ingestion/ingest.rs` line 3, extend the import:

```rust
use crate::ingestion::matching::{suggest_account, suggest_instrument_for_entry};
```

and replace line 98:

```rust
            let sug_ins = suggest_instrument_for_entry(db, entry.symbol.as_deref(), entry.instrument_name.as_deref()).await?;
```

In `backend/src/api/ingest.rs` line 4, extend the import:

```rust
use crate::ingestion::matching::{suggest_account, suggest_instrument_for_entry};
```

and replace line 82:

```rust
        let sug_ins = suggest_instrument_for_entry(&s.db, entry.symbol.as_deref(), entry.instrument_name.as_deref()).await.map_err(AppError::Other)?;
```

- [ ] **Step 6: Run the full backend suite**

Run: `cargo test`
Expected: all PASS (no remaining callers of the old single-arg call pattern; `suggest_instrument` itself stays, used by the new function and its existing test).

- [ ] **Step 7: Commit**

```bash
git add backend/src/ingestion/matching.rs backend/src/ingestion/ingest.rs backend/src/api/ingest.rs
git commit -m "feat(ingest): suggest instruments by exact name when symbol is absent"
```

---

### Task 4: Extraction fixture test + Bibit rules in the system prompt

**Files:**
- Modify: `backend/src/ingestion/extract.rs` (test only)
- Modify: `backend/src/ingestion/ingest.rs:8-13` (`SYSTEM_PROMPT`)

- [ ] **Step 1: Write the fixture test (should pass immediately — it pins current behavior)**

Add to the tests module in `backend/src/ingestion/extract.rs`:

```rust
    #[test]
    fn bibit_amount_only_fund_buys_parse_and_stay_untouched() {
        // Verbatim raw_llm_json from production review items 54-56 (asdc.jpeg,
        // Bibit "Transaksi -> Order" tab): three fund buys with an IDR amount
        // and no units/NAV. normalize_entry must not invent quantity/price
        // (its price >= IDX_PRICE_FLOOR guard skips these) or flag them.
        let raw = r#"{"doc_type": "txn_history", "entries": [{"entry_type": "buy", "instrument_name": "Pendidikan Noah - Obligasi (Sucorinvest Bond Fund)", "amount_native": "13000000", "currency": "IDR", "note": "Goal: Pendidikan Noah, Obligasi category. Fund: Sucorinvest Bond Fund. Pembelian Berhasil.", "confidence": 0.72}, {"entry_type": "buy", "instrument_name": "Mobil - Pasar Uang (Majoris Pasar Uang Indonesia)", "amount_native": "20000000", "currency": "IDR", "note": "Goal: Mobil, Pasar Uang category. Fund: Majoris Pasar Uang Indonesia. Pembelian Berhasil.", "confidence": 0.72}, {"entry_type": "buy", "instrument_name": "Dana Darurat - Pasar Uang (Majoris Pasar Uang Indonesia)", "amount_native": "3000000", "currency": "IDR", "note": "Goal: Dana Darurat, Pasar Uang category. Fund: Majoris Pasar Uang Indonesia. Pembelian Berhasil.", "confidence": 0.72}]}"#;
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "txn_history");
        assert_eq!(e.entries.len(), 3);
        for entry in &e.entries {
            assert_eq!(entry.quantity, None, "must not invent units");
            assert_eq!(entry.price_native, None, "must not invent NAV");
            assert!(!entry.force_attention);
        }
        assert_eq!(e.entries[0].amount_native.as_deref(), Some("13000000"));
        assert_eq!(e.entries[1].amount_native.as_deref(), Some("20000000"));
        assert_eq!(e.entries[2].amount_native.as_deref(), Some("3000000"));
    }
```

- [ ] **Step 2: Run it**

Run: `cargo test ingestion::extract::tests::bibit`
Expected: PASS (this is a pinning test; if it fails, normalization regressed — stop and investigate).

- [ ] **Step 3: Add the Bibit rules to `SYSTEM_PROMPT`**

In `backend/src/ingestion/ingest.rs`, inside the `SYSTEM_PROMPT` raw string, append a new sentence block at the end (after the existing IDX paragraph, before the closing `"#`):

```text

IMPORTANT for Indonesian mutual fund apps such as Bibit: the goal/portfolio name (e.g. "Dana Darurat", "Pendidikan Noah", "Mobil") is NOT the instrument — put it in "account_hint" only. Put the clean fund name (e.g. "Sucorinvest Bond Fund", "Majoris Pasar Uang Indonesia") in "instrument_name". Mutual fund purchases are usually shown as an IDR amount with no units and no NAV: put that amount in "amount_native" and omit "quantity" and "price_native" entirely — NEVER invent units or NAV. Skip failed or cancelled orders. Put the order status (e.g. "Pembelian Berhasil") in "note".
```

(Keep it as one paragraph on its own line inside the raw string, matching the style of the IDX paragraph above it.)

- [ ] **Step 4: Run the backend suite**

Run: `cargo test`
Expected: all PASS (prompt text is not asserted by unit tests; the `live_extract_smoke` test is `#[ignore]`d).

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/extract.rs backend/src/ingestion/ingest.rs
git commit -m "feat(ingest): teach extractor Bibit goal/fund separation and amount-only buys"
```

---

### Task 5: Review UI — Amount field, amount-only hint, pass-through

**Files:**
- Modify: `frontend/src/api/schemas.ts:101-113` (`ExtractedEntrySchema`)
- Modify: `frontend/src/pages/ImportPage.tsx` (`ReviewCard`)
- Test: `frontend/src/pages/ImportPage.test.tsx`

- [ ] **Step 1: Write the failing test**

Add to `frontend/src/pages/ImportPage.test.tsx`. New imports needed at the top:

```tsx
import { fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
```

(`render`, `screen`, `waitFor` are already imported.)

```tsx
test("amount-only fund item shows amount, hint, and confirms with amount_native", async () => {
  const reviewItem = {
    id: 54, batch_id: "b", source_kind: "image", source_filename: "asdc.jpeg",
    source_path: "p", doc_type: "txn_history", status: "pending", needs_attention: 0,
    payload_json: JSON.stringify({
      entry_type: "buy",
      instrument_name: "Sucorinvest Bond Fund",
      amount_native: "13000000",
      currency: "IDR",
      account_hint: "Pendidikan Noah",
      confidence: 0.72,
    }),
    raw_llm_json: "{}",
    suggested_instrument_id: 7,
    suggested_account_id: 3,
    created_txn_id: null,
    created_at: "2026-06-05T06:37:50Z",
    confirmed_at: null,
  };
  let confirmBody: Record<string, unknown> | null = null;
  server.use(
    http.get("/api/ingest/review", () => HttpResponse.json([reviewItem])),
    http.get("/api/instruments", () => HttpResponse.json([{
      id: 7, symbol: "SBF", name: "Sucorinvest Bond Fund", instrument_type: "mutual_fund",
      native_currency: "IDR", category_id: null, price_source: "manual", decimals: 4, note: null,
    }])),
    http.get("/api/accounts", () => HttpResponse.json([{
      id: 3, name: "Pendidikan Noah", account_type: "manual", institution: null,
      native_currency: "IDR", note: null, created_at: "2026-01-01T00:00:00Z",
    }])),
    http.post("/api/ingest/review/:id/confirm", async ({ request }) => {
      confirmBody = (await request.json()) as Record<string, unknown>;
      return HttpResponse.json({ created_txn_id: 99 });
    }),
  );

  render(<ImportPage />, { wrapper });

  // Amount field prefilled from the payload
  expect(await screen.findByLabelText("Amount")).toHaveValue("13000000");
  // amount-only hint shown because quantity & price are empty
  expect(screen.getByText(/amount-only/i)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: /konfirmasi item ini/i }));

  await waitFor(() => expect(confirmBody).not.toBeNull());
  expect(confirmBody!.amount_native).toBe("13000000");
  expect(confirmBody!.quantity).toBe("");
  expect(confirmBody!.price_native).toBe("");
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/pages/ImportPage.test.tsx`
Expected: new test FAILS — `findByLabelText("Amount")` finds nothing.

- [ ] **Step 3: Add `amount_native` to the zod schema**

In `frontend/src/api/schemas.ts`, add to `ExtractedEntrySchema` (after `note`):

```ts
  amount_native: z.string().nullable().optional(),
```

- [ ] **Step 4: Implement the ReviewCard changes in `frontend/src/pages/ImportPage.tsx`**

4a. Pre-select inline-create for named-but-unmatched instruments (mutual funds have no symbol). Replace the `defaultInstrumentId` block (currently lines 57-61):

```tsx
  // Pre-select inline-create when something identifiable was extracted but no
  // instrument matched: a symbol (stocks) or a bare name (mutual funds).
  const defaultInstrumentId = item.suggested_instrument_id
    ? String(item.suggested_instrument_id)
    : p.symbol || p.instrument_name
    ? CREATE_NEW
    : "";
```

4b. Track the amount in form state and seed `new_symbol` from the fund name when there is no ticker. In the `useState({ ... })` initializer, change:

```tsx
    new_symbol: p.symbol ?? "",
```

to:

```tsx
    new_symbol: p.symbol ?? p.instrument_name ?? "",
```

and add after `executed_at`:

```tsx
    amount_native: p.amount_native ?? "",
```

4c. Add the amount-only flag right after the `needs` constant (line 79):

```tsx
  // Bibit-style mutual fund entries: an IDR amount, no units/NAV. The backend
  // records these as quantity = amount at price 1 (value-based tracking).
  const isAmountOnly = !form.quantity && !form.price_native && !!form.amount_native;
```

4d. Pass the amount through on confirm — in the `confirm.mutateAsync` payload, after `currency: form.currency,` add:

```tsx
          amount_native: form.amount_native,
```

4e. Add the Nominal field to the editable grid, after the "Harga" label block (after current line 216):

```tsx
        <label className="field">
          <span className="field-label">Nominal</span>
          <input className="input" value={form.amount_native} onChange={set("amount_native")} aria-label="Amount" />
          {isAmountOnly && (
            <span className="t-xs t-muted">amount-only — dicatat sebagai nominal di harga 1</span>
          )}
        </label>
```

4f. Make the empty Jumlah/Harga inputs self-explanatory — add a placeholder to both (lines 211 and 215):

```tsx
          <input className="input" value={form.quantity} onChange={set("quantity")} aria-label="Quantity" placeholder={isAmountOnly ? "amount-only" : undefined} />
```

```tsx
          <input className="input" value={form.price_native} onChange={set("price_native")} aria-label="Price" placeholder={isAmountOnly ? "amount-only" : undefined} />
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `npx vitest run src/pages/ImportPage.test.tsx`
Expected: all PASS (including the pre-existing upload/empty-state tests).

- [ ] **Step 6: Run the full frontend suite + typecheck**

Run: `npx vitest run && npm run build`
Expected: all tests PASS, build succeeds with no TS errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/pages/ImportPage.tsx frontend/src/pages/ImportPage.test.tsx
git commit -m "feat(frontend): amount field and amount-only confirm for fund review items"
```

---

### Task 6: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Backend — tests and lints**

Run from `backend/`:

```bash
cargo test && cargo clippy --all-targets -- -D warnings && cargo fmt --check
```

Expected: tests PASS, clippy clean, formatting clean. (CI gates on clippy — see commit d6d171f.)

- [ ] **Step 2: Frontend — tests, lint, build**

Run from `frontend/`:

```bash
npx vitest run && npm run lint --if-present && npm run build
```

Expected: all PASS / clean.

- [ ] **Step 3: Commit any stragglers and review the diff**

```bash
git status          # should be clean apart from intended files
git log --oneline origin/main..HEAD
git diff origin/main..HEAD --stat
```

Expected: the 5 feature commits (+ the spec/plan docs commits) on `feat/amount-only-fund-buys`.

Do NOT push or open a PR in this task — integration is decided by the finishing-a-development-branch workflow (note: pushing to `main` auto-deploys to prod, so everything goes through a PR).
