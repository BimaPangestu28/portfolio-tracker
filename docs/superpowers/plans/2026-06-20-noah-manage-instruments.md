# Noah: Manage Instruments From Chat — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give the Telegram assistant (Noah) three write tools — `create_instrument`, `edit_instrument`, `delete_instrument` — so the owner never has to use the web UI to register/fix/remove an instrument, and unblock the three places that currently dead-end at the web UI.

**Architecture:** Each tool is a JSON schema entry in `assistant/tools.rs::definitions()` plus a handler `fn` in `assistant/dispatcher.rs` wired into `dispatch()`. Handlers are thin wrappers over the existing, already-tested `repo::instruments` functions (`find_or_create`, `update`, `delete`, `txn_count`, `find_by_symbol`). All three write data, so they follow the existing **recap-then-execute** convention (the tool *description* tells the model to echo + confirm with the owner before calling; the handler itself just writes).

**Tech Stack:** Rust, `sqlx` (SQLite), `serde_json`, `tokio`, `chrono`. Tests are `#[tokio::test]` against an in-memory DB.

## Global Constraints

- **No `cargo fmt`** — backend convention is clippy + tests only; never reformat.
- Run backend commands from the `backend/` directory.
- Handler signature: `async fn name(db: &Db, input: &serde_json::Value) -> Result<String, String>` — `Ok(text)` feeds the model a tool_result, `Err(text)` becomes an `is_error` result the model self-corrects from.
- Helpers already in `dispatcher.rs`: `str_arg(input, key) -> Option<&str>` (trims, empty→None), `id_arg(input, key) -> Result<i64,String>` (required int), `optional_id(input, key) -> Result<Option<i64>,String>` (absent/null→None, non-int→Err).
- Instrument `symbol` and `native_currency` are **immutable** (symbol = dedup identity; currency would silently break cost-basis). `UpdateInstrument` reflects this — it has no such fields.
- Test seeding helpers already in `dispatcher.rs` tests: `mem_db()` and `seed_instrument(&db, "SYM") -> InstrumentRow`.
- The `defines_all_tools_with_schemas` test in `tools.rs` asserts the **exact ordered list** of tool names — every task that adds a tool MUST extend that vec or the test fails.

---

### Task 1: `create_instrument` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs` (add schema after `list_instruments` def ~line 253; extend name vec in test ~line 489)
- Modify: `backend/src/assistant/dispatcher.rs` (add dispatch arm after line 34; add handler after `create_account` ~line 846; add test in `mod tests`)

**Interfaces:**
- Consumes: `repo::instruments::{NewInstrument, find_or_create, find_by_symbol}` (already exist).
- Produces: dispatch route `"create_instrument"`; handler `async fn create_instrument(db, input) -> Result<String,String>`.

- [ ] **Step 1: Write the failing test**

Add to `backend/src/assistant/dispatcher.rs` inside `mod tests`:

```rust
    #[tokio::test]
    async fn create_instrument_creates_then_reuses_by_symbol() {
        let db = mem_db().await;
        let out = dispatch(&db, "create_instrument", &serde_json::json!({
            "symbol": "USDC", "name": "USD Coin", "instrument_type": "crypto",
            "native_currency": "USD", "price_source": "manual"
        })).await.unwrap();
        assert!(out.contains("USDC"), "{out}");
        assert!(out.contains("dibuat"), "{out}");

        // Idempotent on case-insensitive symbol — second call reuses, no duplicate.
        let again = dispatch(&db, "create_instrument", &serde_json::json!({
            "symbol": "usdc", "name": "USD Coin", "instrument_type": "crypto",
            "native_currency": "USD", "price_source": "manual"
        })).await.unwrap();
        assert!(again.contains("udah ada"), "{again}");
        assert_eq!(crate::repo::instruments::list(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn create_instrument_requires_symbol() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_instrument", &serde_json::json!({
            "name": "USD Coin", "instrument_type": "crypto", "price_source": "manual"
        })).await.unwrap_err();
        assert!(err.contains("symbol"), "{err}");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test create_instrument_ 2>&1 | tail -20`
Expected: FAIL — `dispatch` returns `Err("unknown tool: create_instrument")`, so `.unwrap()` panics.

- [ ] **Step 3: Add the dispatch arm**

In `dispatch()` in `dispatcher.rs`, immediately after the `"list_instruments" => list_instruments(db).await,` line (line 34):

```rust
        "create_instrument" => create_instrument(db, input).await,
```

- [ ] **Step 4: Add the handler**

In `dispatcher.rs`, immediately after `create_account` (after line 846):

```rust
async fn create_instrument(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let symbol = str_arg(input, "symbol").ok_or("missing required argument 'symbol'")?;
    let name = str_arg(input, "name").ok_or("missing required argument 'name'")?;
    let instrument_type =
        str_arg(input, "instrument_type").ok_or("missing required argument 'instrument_type'")?;
    let price_source =
        str_arg(input, "price_source").ok_or("missing required argument 'price_source'")?;
    // Detect reuse vs create so the echo is honest — find_or_create is idempotent on symbol.
    let existed = crate::repo::instruments::find_by_symbol(db, symbol)
        .await
        .map_err(|e| format!("db error: {e}"))?
        .is_some();
    let ins = crate::repo::instruments::find_or_create(db, &crate::repo::instruments::NewInstrument {
        symbol: symbol.to_string(),
        name: name.to_string(),
        instrument_type: instrument_type.to_string(),
        native_currency: str_arg(input, "native_currency").unwrap_or("IDR").to_string(),
        category_id: None,
        price_source: price_source.to_string(),
        decimals: optional_id(input, "decimals")?,
        note: str_arg(input, "note").map(str::to_string),
    })
    .await
    .map_err(|e| format!("db error: {e}"))?;
    let verb = if existed { "udah ada" } else { "dibuat" };
    Ok(format!("instrumen #{} {} ({}) {verb}", ins.id, ins.symbol, ins.instrument_type))
}
```

- [ ] **Step 5: Add the tool schema**

In `tools.rs`, immediately after the `list_instruments` object (after line 253, before the `list_projects` object), insert:

```rust
        {
            "name": "create_instrument",
            "description": "Register a new instrument the owner mentions that isn't in list_instruments yet (e.g. USDC, a new stock, a reksadana). Idempotent on symbol — if it already exists it's reused, not duplicated. Before calling, ASK the owner whether they want live pricing (coingecko for crypto, e.g. 'coingecko:usd-coin'; yahoo for stocks, e.g. 'yahoo:ASII.JK') or 'manual' (fine for stablecoins) and put that in price_source. Echo the full instrument (symbol, name, type, currency, price source) and get confirmation before calling — this writes data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "symbol": { "type": "string", "description": "Ticker/symbol, e.g. 'USDC'. Case-insensitive dedup key." },
                    "name": { "type": "string", "description": "Display name, e.g. 'USD Coin'." },
                    "instrument_type": { "type": "string", "description": "crypto|stock_id|stock_us|etf|mutual_fund|cash|bond|gold|other" },
                    "native_currency": { "type": "string", "description": "ISO code; defaults to IDR." },
                    "price_source": { "type": "string", "description": "'manual', or a live source like 'coingecko:usd-coin' / 'yahoo:ASII.JK'. Ask the owner first." },
                    "decimals": { "type": "integer", "description": "Fractional precision; defaults to 8." },
                    "note": { "type": "string", "description": "Optional note." }
                },
                "required": ["symbol", "name", "instrument_type", "price_source"]
            }
        },
```

- [ ] **Step 6: Update the tool-name list test**

In `tools.rs` `defines_all_tools_with_schemas`, change line 489 — the `"list_instruments",` entry at the end of the portfolio block — so the new tool follows it:

```rust
                "list_pending_reviews", "confirm_review", "create_transaction", "list_transactions", "edit_transaction", "delete_transaction", "list_instruments", "create_instrument",
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd backend && cargo test create_instrument_ 2>&1 | tail -20 && cargo test --lib assistant::tools 2>&1 | tail -10`
Expected: PASS — both `create_instrument_*` tests and `defines_all_tools_with_schemas` green.

- [ ] **Step 8: Commit**

```bash
cd backend && cargo clippy --all-targets 2>&1 | tail -5
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): create_instrument tool"
```

---

### Task 2: `edit_instrument` tool

**Files:**
- Modify: `backend/src/assistant/tools.rs` (schema after `create_instrument`; extend name vec)
- Modify: `backend/src/assistant/dispatcher.rs` (dispatch arm; handler after `create_instrument`; test)

**Interfaces:**
- Consumes: `repo::instruments::{UpdateInstrument, update, get}` (already exist). `UpdateInstrument` fields: `name`, `instrument_type`, `price_source`, `decimals`, `category_id: Option<Option<i64>>` (absent = leave unchanged).
- Produces: dispatch route `"edit_instrument"`; handler `async fn edit_instrument(db, input) -> Result<String,String>`.

- [ ] **Step 1: Write the failing test**

Add to `dispatcher.rs` `mod tests`:

```rust
    #[tokio::test]
    async fn edit_instrument_updates_only_passed_fields() {
        let db = mem_db().await;
        let ins = seed_instrument(&db, "ASII").await;
        let out = dispatch(&db, "edit_instrument", &serde_json::json!({
            "id": ins.id, "price_source": "yahoo:ASII.JK", "instrument_type": "stock_id"
        })).await.unwrap();
        assert!(out.contains(&format!("#{}", ins.id)), "{out}");

        let updated = crate::repo::instruments::get(&db, ins.id).await.unwrap();
        assert_eq!(updated.price_source, "yahoo:ASII.JK");
        assert_eq!(updated.instrument_type, "stock_id");
        // Untouched identity fields stay put.
        assert_eq!(updated.symbol, "ASII");
        assert_eq!(updated.name, ins.name);
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test edit_instrument_ 2>&1 | tail -20`
Expected: FAIL — `Err("unknown tool: edit_instrument")` → `.unwrap()` panics.

- [ ] **Step 3: Add the dispatch arm**

In `dispatch()`, after the `"create_instrument" => ...` line:

```rust
        "edit_instrument" => edit_instrument(db, input).await,
```

- [ ] **Step 4: Add the handler**

In `dispatcher.rs`, after `create_instrument`:

```rust
async fn edit_instrument(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    crate::repo::instruments::get(db, id).await.map_err(|_| format!("instrumen #{id} nggak ada"))?;
    let u = crate::repo::instruments::UpdateInstrument {
        name: str_arg(input, "name").map(str::to_string),
        instrument_type: str_arg(input, "instrument_type").map(str::to_string),
        price_source: str_arg(input, "price_source").map(str::to_string),
        decimals: optional_id(input, "decimals")?,
        category_id: None, // not edited from chat — leave unchanged
    };
    let ins = crate::repo::instruments::update(db, id, &u).await.map_err(|e| format!("{e}"))?;
    Ok(format!(
        "instrumen #{} {} diperbarui — {} ({}), harga {}",
        ins.id, ins.symbol, ins.name, ins.instrument_type, ins.price_source
    ))
}
```

- [ ] **Step 5: Add the tool schema**

In `tools.rs`, after the `create_instrument` object:

```rust
        {
            "name": "edit_instrument",
            "description": "Edit an existing instrument's editable fields: name, instrument_type, price_source, decimals. Get the id from list_instruments. Pass only the fields to change. The symbol and native currency are NOT editable (symbol is the identity; currency would break cost-basis). Echo the change to the owner and get confirmation before calling — this rewrites data.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "id": { "type": "integer", "description": "Instrument id from list_instruments." },
                    "name": { "type": "string", "description": "New display name." },
                    "instrument_type": { "type": "string", "description": "crypto|stock_id|stock_us|etf|mutual_fund|cash|bond|gold|other" },
                    "price_source": { "type": "string", "description": "'manual' or a live source like 'coingecko:usd-coin' / 'yahoo:ASII.JK'." },
                    "decimals": { "type": "integer", "description": "Fractional precision." }
                },
                "required": ["id"]
            }
        },
```

- [ ] **Step 6: Update the tool-name list test**

Extend the same vec line in `defines_all_tools_with_schemas`:

```rust
                "list_pending_reviews", "confirm_review", "create_transaction", "list_transactions", "edit_transaction", "delete_transaction", "list_instruments", "create_instrument", "edit_instrument",
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd backend && cargo test edit_instrument_ 2>&1 | tail -20 && cargo test --lib assistant::tools 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
cd backend && cargo clippy --all-targets 2>&1 | tail -5
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): edit_instrument tool"
```

---

### Task 3: `delete_instrument` tool (with txn guard)

**Files:**
- Modify: `backend/src/assistant/tools.rs` (schema after `edit_instrument`; extend name vec)
- Modify: `backend/src/assistant/dispatcher.rs` (dispatch arm; handler after `edit_instrument`; two tests)

**Interfaces:**
- Consumes: `repo::instruments::{get, txn_count, delete}` (already exist). `txn_count(db, id) -> Result<i64>` counts ledger rows referencing the instrument; `delete` clears the `review_item` suggestion FK but would fail on a `txn` FK — hence the guard.
- Produces: dispatch route `"delete_instrument"`; handler `async fn delete_instrument(db, input) -> Result<String,String>`.

- [ ] **Step 1: Write the failing tests**

Add to `dispatcher.rs` `mod tests`:

```rust
    #[tokio::test]
    async fn delete_instrument_refuses_when_transactions_exist() {
        let db = mem_db().await;
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Pintu".into(), account_type: "exchange".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = seed_instrument(&db, "USDC").await;
        crate::repo::transactions::create(&db, &crate::repo::transactions::NewTransaction {
            account_id: acc.id, instrument_id: ins.id, txn_type: "deposit".into(),
            executed_at: chrono::Utc::now(), quantity: "100".into(), price_native: "1".into(),
            fee_native: None, currency: "USD".into(), fx_to_idr: "16000".into(), fx_to_usd: "1".into(),
            note: None, source: None, external_id: None,
        }).await.unwrap();

        let err = dispatch(&db, "delete_instrument", &serde_json::json!({ "id": ins.id }))
            .await.unwrap_err();
        assert!(err.contains("transaksi"), "{err}");
        // Still present — guard blocked the delete.
        assert!(crate::repo::instruments::get(&db, ins.id).await.is_ok());
    }

    #[tokio::test]
    async fn delete_instrument_removes_unused_instrument() {
        let db = mem_db().await;
        let ins = seed_instrument(&db, "USDC").await;
        let out = dispatch(&db, "delete_instrument", &serde_json::json!({ "id": ins.id }))
            .await.unwrap();
        assert!(out.contains(&format!("#{}", ins.id)), "{out}");
        assert!(crate::repo::instruments::get(&db, ins.id).await.is_err());
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test delete_instrument_ 2>&1 | tail -20`
Expected: FAIL — `Err("unknown tool: delete_instrument")` → `.unwrap()`/`.unwrap_err()` mismatch panics.

- [ ] **Step 3: Add the dispatch arm**

In `dispatch()`, after the `"edit_instrument" => ...` line:

```rust
        "delete_instrument" => delete_instrument(db, input).await,
```

- [ ] **Step 4: Add the handler**

In `dispatcher.rs`, after `edit_instrument`:

```rust
async fn delete_instrument(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    let ins = crate::repo::instruments::get(db, id).await
        .map_err(|_| format!("instrumen #{id} nggak ada"))?;
    let n = crate::repo::instruments::txn_count(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if n > 0 {
        return Err(format!(
            "instrumen #{id} {} masih dipakai {n} transaksi — hapus transaksinya dulu \
             (list_transactions lalu delete_transaction) sebelum hapus instrumen",
            ins.symbol
        ));
    }
    crate::repo::instruments::delete(db, id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("instrumen #{id} {} dihapus", ins.symbol))
}
```

- [ ] **Step 5: Add the tool schema**

In `tools.rs`, after the `edit_instrument` object:

```rust
        {
            "name": "delete_instrument",
            "description": "Delete an instrument by id (e.g. one added by mistake). Get the id from list_instruments. REFUSES if any transaction still references it — delete those transactions first. Always confirm with the owner before calling — this permanently removes data.",
            "input_schema": {
                "type": "object",
                "properties": { "id": { "type": "integer", "description": "Instrument id from list_instruments." } },
                "required": ["id"]
            }
        },
```

- [ ] **Step 6: Update the tool-name list test**

Extend the same vec line:

```rust
                "list_pending_reviews", "confirm_review", "create_transaction", "list_transactions", "edit_transaction", "delete_transaction", "list_instruments", "create_instrument", "edit_instrument", "delete_instrument",
```

- [ ] **Step 7: Run tests to verify they pass**

Run: `cd backend && cargo test delete_instrument_ 2>&1 | tail -20 && cargo test --lib assistant::tools 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
cd backend && cargo clippy --all-targets 2>&1 | tail -5
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): delete_instrument tool with txn guard"
```

---

### Task 4: Unblock the three "web UI → Data" pointers

**Files:**
- Modify: `backend/src/assistant/dispatcher.rs:936` (`create_transaction` not-found error)
- Modify: `backend/src/assistant/tools.rs:251` (`list_instruments` description)
- Modify: `backend/src/assistant/agent.rs:53-56` (system prompt)

**Interfaces:** No new routes/handlers — copy changes only. The model now learns it can create instruments from chat.

- [ ] **Step 1: Reword the `create_transaction` not-found error**

In `dispatcher.rs`, the `.ok_or_else` on line 936 — replace:

```rust
                .ok_or_else(|| format!("instrumen '{name}' belum terdaftar — tambah dulu di Web UI → Data"))?
```

with:

```rust
                .ok_or_else(|| format!("instrumen '{name}' belum terdaftar — bikin dulu pakai create_instrument, lalu ulangi"))?
```

- [ ] **Step 2: Reword the `list_instruments` description**

In `tools.rs` line 251, replace the trailing sentence of the `list_instruments` description:

```
If it genuinely doesn't exist, tell the user to add it in the web UI → Data (instruments can't be created from chat).
```

with:

```
If it genuinely doesn't exist, create it with create_instrument (after confirming the details with the owner).
```

- [ ] **Step 3: Reword the system prompt**

In `agent.rs`, in the block at lines 53-56, replace:

```
If the instrument shows \
'belum dikenali', call list_instruments to find it (auto-matching only catches \
exact names); if it isn't there, tell the user to add it in the web UI → Data \
— instruments can't be created from chat. The account/instrument shown is only \
```

with:

```
If the instrument shows \
'belum dikenali', call list_instruments to find it (auto-matching only catches \
exact names); if it isn't there, create it with create_instrument (ask the owner \
for the price source — live coingecko/yahoo or manual — and confirm first). \
The account/instrument shown is only \
```

- [ ] **Step 4: Build + full assistant test suite + clippy**

Run: `cd backend && cargo test --lib assistant:: 2>&1 | tail -20 && cargo clippy --all-targets 2>&1 | tail -5`
Expected: PASS — all assistant tests green (including any prompt-content test), no new clippy warnings. There is no test asserting the old "web UI" wording, so nothing should break.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/dispatcher.rs backend/src/assistant/tools.rs backend/src/assistant/agent.rs
git commit -m "feat(assistant): point instrument-not-found flows at create_instrument"
```

---

## Self-Review

**Spec coverage:**
- `create_instrument` (idempotent, asks price source) → Task 1. ✓
- `edit_instrument` (name/type/price_source/decimals; symbol+currency locked) → Task 2. ✓
- `delete_instrument` (refuse when txns reference) → Task 3. ✓
- Unblock 3 web-UI pointers (create_transaction error, list_instruments desc, system prompt) → Task 4. ✓
- Price source "ask each time" → encoded in `create_instrument` description + system prompt. ✓
- Tool-count assertion bumped → Steps in Tasks 1/2/3 each extend the vec. ✓
- Out of scope (category-by-name, symbol/currency edit, bulk import) → not implemented, `category_id: None`. ✓

**Placeholder scan:** No TBD/TODO; every code step shows full code. ✓

**Type consistency:** Handlers all `async fn(db: &Db, input: &serde_json::Value) -> Result<String,String>`. `NewInstrument`/`UpdateInstrument` field names match `repo/instruments.rs` (`decimals: Option<i64>`, `category_id` present on both). `optional_id` returns `Result<Option<i64>,String>` matching `decimals` field. `txn_count` returns `Result<i64>`. Tool-name vec extended consistently across Tasks 1→3. ✓
