# Natural-chat review resolution + DB-aware account matching — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the owner sends a transaction screenshot to the Telegram bot, auto-resolve the account from the database (instrument history + fuzzy name) and have the assistant confirm it in natural chat, instead of dead-ending unrecognized accounts at "lengkapi di web UI → Data".

**Architecture:** Two independent changes. (A) ingestion gains a smarter `resolve_account` that checks exact-name → instrument transaction history → single fuzzy-name match before giving up. (B) the Telegram upload handler stops emitting inline-button prompts and instead kicks off the assistant agent with a model-facing "seed" describing the staged items; the assistant speaks naturally and drives the existing `confirm_review`/`list_accounts`/`create_account` tools on the owner's reply.

**Tech Stack:** Rust, sqlx (SQLite), tokio, the existing Claude tool-use agent loop. Tests are `#[tokio::test]` in-module unit tests against `sqlite::memory:`.

**Spec:** `docs/superpowers/specs/2026-06-13-natural-chat-review-resolution-design.md`

**Working directory:** all `cargo` commands run from `backend/`. All paths below are relative to the repo root.

---

### Task 1: `accounts_for_instrument` repo helper

Returns, per account that has traded an instrument, `(account_id, txn_count, last_executed_at)` ordered by count desc then recency desc. Drives the "infer from history" step of account resolution.

**Files:**
- Modify: `backend/src/repo/transactions.rs` (add public fn near `has_price_one_txn`, ~line 120; add test in the `#[cfg(test)] mod tests` block)

- [ ] **Step 1: Write the failing test**

Add to the `tests` module in `backend/src/repo/transactions.rs` (after `has_price_one_txn_detects_value_based_rows`):

```rust
    #[tokio::test]
    async fn accounts_for_instrument_orders_by_count_then_recency() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc_a = accounts::create(&db, &accounts::NewAccount { name:"A".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let acc_b = accounts::create(&db, &accounts::NewAccount { name:"B".into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        let ins = instruments::create(&db, &instruments::NewInstrument { symbol:"QQQM".into(), name:"Invesco NASDAQ 100 ETF".into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();

        let buy = |account_id: i64, when: DateTime<Utc>| NewTransaction {
            account_id, instrument_id: ins.id, txn_type:"buy".into(), executed_at: when,
            quantity:"1".into(), price_native:"100".into(), fee_native:None, currency:"USD".into(),
            fx_to_idr:"16000".into(), fx_to_usd:"1".into(), note:None, source:None, external_id:None,
        };
        // acct A: 2 txns, acct B: 1 (more recent than either A txn)
        let t0 = DateTime::parse_from_rfc3339("2026-01-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let t1 = DateTime::parse_from_rfc3339("2026-02-01T00:00:00Z").unwrap().with_timezone(&Utc);
        let t2 = DateTime::parse_from_rfc3339("2026-03-01T00:00:00Z").unwrap().with_timezone(&Utc);
        create(&db, &buy(acc_a.id, t0)).await.unwrap();
        create(&db, &buy(acc_a.id, t1)).await.unwrap();
        create(&db, &buy(acc_b.id, t2)).await.unwrap();

        let rows = accounts_for_instrument(&db, ins.id).await.unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, acc_a.id, "most-frequent account first");
        assert_eq!(rows[0].1, 2);
        assert_eq!(rows[1].0, acc_b.id);
        assert_eq!(rows[1].1, 1);
        // empty for an instrument with no history
        let ins2 = instruments::create(&db, &instruments::NewInstrument { symbol:"VOO".into(), name:"VOO".into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap();
        assert!(accounts_for_instrument(&db, ins2.id).await.unwrap().is_empty());
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test accounts_for_instrument_orders_by_count_then_recency`
Expected: FAIL to compile — `cannot find function accounts_for_instrument`.

- [ ] **Step 3: Write minimal implementation**

Add to `backend/src/repo/transactions.rs` immediately after `has_price_one_txn` (before `existing_external_ids`):

```rust
/// Per account that has traded this instrument: (account_id, txn_count,
/// last_executed_at), ordered by count desc then most-recent first. Drives the
/// "infer the account from history" step of ingest account resolution.
pub async fn accounts_for_instrument(
    db: &Db,
    instrument_id: i64,
) -> anyhow::Result<Vec<(i64, i64, String)>> {
    let rows = sqlx::query_as::<_, (i64, i64, String)>(
        "SELECT account_id, COUNT(*) AS cnt, MAX(executed_at) AS last_at \
         FROM txn WHERE instrument_id = ? \
         GROUP BY account_id ORDER BY cnt DESC, last_at DESC",
    )
    .bind(instrument_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd backend && cargo test accounts_for_instrument_orders_by_count_then_recency`
Expected: PASS (1 passed).

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/transactions.rs
git commit -m "$(cat <<'EOF'
feat(repo): accounts_for_instrument for history-based account inference

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 2: `resolve_account` in the matching layer

Replaces the bare exact-name `suggest_account` at the ingest call-site with: exact name → instrument history → single fuzzy-name match → None. `suggest_account` is kept and reused for the exact step.

**Files:**
- Modify: `backend/src/ingestion/matching.rs` (add `resolve_account` + private `fuzzy_account`; add tests)

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module in `backend/src/ingestion/matching.rs` (after `suggest_instrument_for_entry_falls_back_to_name`). These helpers need `transactions` and `chrono`:

```rust
    use crate::repo::transactions::{self, NewTransaction};

    async fn buy(db: &Db, account_id: i64, instrument_id: i64) {
        transactions::create(db, &NewTransaction {
            account_id, instrument_id, txn_type:"buy".into(),
            executed_at: chrono::Utc::now(), quantity:"1".into(), price_native:"100".into(),
            fee_native:None, currency:"USD".into(), fx_to_idr:"16000".into(), fx_to_usd:"1".into(),
            note:None, source:None, external_id:None,
        }).await.unwrap();
    }

    async fn mk_account(db: &Db, name: &str) -> i64 {
        accounts::create(db, &accounts::NewAccount { name:name.into(), account_type:"manual".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap().id
    }

    async fn mk_instrument(db: &Db, symbol: &str) -> i64 {
        instruments::create(db, &instruments::NewInstrument { symbol:symbol.into(), name:symbol.into(), instrument_type:"etf".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(8), note:None }).await.unwrap().id
    }

    #[tokio::test]
    async fn resolve_account_prefers_exact_name_over_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let pluang = mk_account(&db, "Pluang").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, pluang, qqqm).await; // history points at Pluang
        // exact hint "ibkr" must win over the history account
        assert_eq!(resolve_account(&db, Some("ibkr"), Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_infers_from_single_history() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, ibkr, qqqm).await;
        // no hint at all -> inferred from the instrument's history
        assert_eq!(resolve_account(&db, None, Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_picks_most_frequent_when_history_spans_accounts() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let pluang = mk_account(&db, "Pluang").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, ibkr, qqqm).await;
        buy(&db, ibkr, qqqm).await;
        buy(&db, pluang, qqqm).await;
        assert_eq!(resolve_account(&db, None, Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_history_beats_fuzzy_name() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR").await;
        let _other = mk_account(&db, "Pluang Premium").await;
        let qqqm = mk_instrument(&db, "QQQM").await;
        buy(&db, ibkr, qqqm).await;
        // "pluang" would fuzzy-match "Pluang Premium", but history (IBKR) is checked first
        assert_eq!(resolve_account(&db, Some("pluang"), Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_single_fuzzy_match() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ibkr = mk_account(&db, "IBKR Pro").await;
        let qqqm = mk_instrument(&db, "QQQM").await; // no history
        // hint "ibkr" is contained in "IBKR Pro" (case-insensitive) -> single match
        assert_eq!(resolve_account(&db, Some("ibkr"), Some(qqqm)).await.unwrap(), Some(ibkr));
    }

    #[tokio::test]
    async fn resolve_account_ambiguous_fuzzy_is_none() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let _a = mk_account(&db, "Bank Jago").await;
        let _b = mk_account(&db, "Bank BCA").await;
        let qqqm = mk_instrument(&db, "QQQM").await; // no history
        assert_eq!(resolve_account(&db, Some("bank"), Some(qqqm)).await.unwrap(), None);
    }

    #[tokio::test]
    async fn resolve_account_none_when_nothing_matches() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let _a = mk_account(&db, "IBKR").await;
        let qqqm = mk_instrument(&db, "QQQM").await; // no history
        assert_eq!(resolve_account(&db, Some("nonexistent"), Some(qqqm)).await.unwrap(), None);
        assert_eq!(resolve_account(&db, None, Some(qqqm)).await.unwrap(), None);
        assert_eq!(resolve_account(&db, None, None).await.unwrap(), None);
    }
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test resolve_account_`
Expected: FAIL to compile — `cannot find function resolve_account`.

- [ ] **Step 3: Write minimal implementation**

Add to `backend/src/ingestion/matching.rs` after `suggest_account` (before the `#[cfg(test)]` block):

```rust
/// Resolve the account for an extracted entry, checking the DB before giving up:
/// exact name match on the hint, then the instrument's transaction history
/// (most-frequent account), then a single unambiguous fuzzy name match. Returns
/// None only when nothing in the DB points at an account — the caller then asks
/// the owner in chat. Never silently writes, so a best-guess default is safe.
pub async fn resolve_account(
    db: &Db,
    account_hint: Option<&str>,
    instrument_id: Option<i64>,
) -> anyhow::Result<Option<i64>> {
    // 1. Exact name match on the hint — the strongest, most reliable signal.
    if let Some(hint) = account_hint {
        if let Some(id) = suggest_account(db, hint).await? {
            return Ok(Some(id));
        }
    }
    // 2. Infer from the instrument's transaction history.
    if let Some(instrument_id) = instrument_id {
        let history = crate::repo::transactions::accounts_for_instrument(db, instrument_id).await?;
        if let Some((account_id, _, _)) = history.first() {
            return Ok(Some(*account_id));
        }
    }
    // 3. Single unambiguous fuzzy name match on the hint.
    if let Some(hint) = account_hint {
        if let Some(id) = fuzzy_account(db, hint).await? {
            return Ok(Some(id));
        }
    }
    Ok(None)
}

/// Case-insensitive containment match (either direction) on account name.
/// Returns Some only when EXACTLY ONE account matches; zero or many -> None so
/// the assistant asks rather than guessing wrong.
async fn fuzzy_account(db: &Db, hint: &str) -> anyhow::Result<Option<i64>> {
    let needle = hint.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(None);
    }
    let rows = sqlx::query_as::<_, (i64,)>(
        "SELECT id FROM account \
         WHERE INSTR(LOWER(name), ?) > 0 OR INSTR(?, LOWER(name)) > 0",
    )
    .bind(&needle)
    .bind(&needle)
    .fetch_all(db)
    .await?;
    if rows.len() == 1 {
        Ok(Some(rows[0].0))
    } else {
        Ok(None)
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test resolve_account_`
Expected: PASS (7 passed). Also run `cd backend && cargo test --lib matching` to confirm no regressions.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/matching.rs
git commit -m "$(cat <<'EOF'
feat(ingest): resolve_account — exact name, then history, then fuzzy

Checks the DB before declaring an account "belum dikenali": exact hint
match, the instrument's prior-transaction account, then a single
unambiguous fuzzy name match.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 3: Wire `resolve_account` into the ingest call-site

Mechanical swap: pass the just-resolved instrument id into account resolution.

**Files:**
- Modify: `backend/src/ingestion/ingest.rs:3` (import) and `:124` (call-site)

- [ ] **Step 1: Update the import**

Change line 3 of `backend/src/ingestion/ingest.rs` from:

```rust
use crate::ingestion::matching::{suggest_account, suggest_instrument_for_entry};
```

to:

```rust
use crate::ingestion::matching::{resolve_account, suggest_instrument_for_entry};
```

- [ ] **Step 2: Update the call-site**

In `backend/src/ingestion/ingest.rs`, change the account-suggestion line (currently line 124) from:

```rust
            let sug_acc = match &entry.account_hint { Some(a) => suggest_account(db, a).await?, None => None };
```

to:

```rust
            // Resolve the account against the DB (exact name, then this
            // instrument's history, then a single fuzzy match) so previously-seen
            // instruments don't show up as "belum dikenali".
            let sug_acc = resolve_account(db, entry.account_hint.as_deref(), sug_ins).await?;
```

- [ ] **Step 3: Build to verify it compiles**

Run: `cd backend && cargo build`
Expected: builds clean. (`suggest_account` is still defined and used by `resolve_account`, so no unused-import warning.)

- [ ] **Step 4: Run the ingest tests**

Run: `cd backend && cargo test --lib ingest`
Expected: PASS — existing ingest tests still green (`ingest_batch` takes a concrete `NativeLlmClient` and is not unit-tested here; `resolve_account`'s behavior is covered by Task 2).

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/ingest.rs
git commit -m "$(cat <<'EOF'
feat(ingest): use resolve_account at the staging call-site

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 4: Refactor the agent loop + add `handle_upload_event`

Extract the tool-use loop into a reply-only core (`run_tool_loop`) and a history loader (`load_history`), then add `handle_upload_event`, which seeds the model with upload context but stores a concise marker in chat history.

**Files:**
- Modify: `backend/src/assistant/agent.rs` (refactor `handle_message`, add `run_tool_loop`, `load_history`, `handle_upload_event`; add two tests)

- [ ] **Step 1: Extract `run_tool_loop` and `load_history`**

In `backend/src/assistant/agent.rs`, add these two helpers immediately before `pub async fn handle_message` (line ~128):

```rust
/// Load the channel's recent chat history as (role, content) pairs.
async fn load_history(db: &Db, channel: &str) -> Vec<(String, String)> {
    crate::repo::chat::recent_by_channel(db, channel, HISTORY_LIMIT)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.role, m.content))
        .collect()
}

/// Drive the tool-use loop over `messages` until the model returns text or hits
/// the iteration cap. Returns the reply only — persisting is the caller's job.
/// A shape anomaly or the cap yields a fallback reply (Ok), never an Err; only a
/// transport/LLM error propagates.
async fn run_tool_loop<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    system: &str,
    mut messages: Vec<serde_json::Value>,
) -> anyhow::Result<String> {
    let tools = super::tools::definitions();
    for _ in 0..MAX_ITERATIONS {
        let resp = model
            .complete_tools(system, &messages, &tools)
            .await
            .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
        let blocks = match extract_blocks(&resp) {
            Ok(blocks) => blocks,
            Err(e) => {
                tracing::warn!("assistant: unusable model response ({e}); using fallback reply");
                return Ok(NO_TEXT_REPLY.to_string());
            }
        };
        let tool_uses: Vec<(String, String, serde_json::Value)> = blocks
            .iter()
            .filter_map(|b| match b {
                ResponseBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if tool_uses.is_empty() {
            let mut reply: String = blocks
                .into_iter()
                .filter_map(|b| match b {
                    ResponseBlock::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            if reply.trim().is_empty() {
                reply = NO_TEXT_REPLY.to_string();
            }
            return Ok(reply);
        }

        messages.push(serde_json::json!({ "role": "assistant", "content": resp["content"].clone() }));
        let mut results = Vec::new();
        for (id, name, input) in &tool_uses {
            let outcome = super::dispatcher::dispatch(db, name, input).await;
            tracing::info!(
                "assistant tool {name}: {}",
                if outcome.is_ok() { "ok" } else { "error" }
            );
            results.push(tool_result_block(id, &outcome));
        }
        messages.push(serde_json::json!({ "role": "user", "content": results }));
    }
    Ok(ITERATION_CAP_REPLY.to_string())
}
```

- [ ] **Step 2: Rewrite `handle_message` to use the helpers**

Replace the entire body of `pub async fn handle_message` (lines ~128-211, from `let now_wib` through the final `Ok(ITERATION_CAP_REPLY...)`) with:

```rust
pub async fn handle_message<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    channel: &str,
    user_msg: &str,
) -> anyhow::Result<String> {
    let now_wib = chrono::Utc::now().with_timezone(&super::time::wib()).to_rfc3339();
    let memory = super::memory::MemoryClient::from_env();
    let facts = match &memory {
        Some(client) => client.search(user_msg, INJECT_FACT_LIMIT).await,
        None => Vec::new(),
    };
    let system = compose_system(&now_wib, &facts);
    let history = load_history(db, channel).await;
    let messages = build_messages(&history, user_msg);

    // Tool side effects commit eagerly per iteration and are intentionally NOT
    // rolled back if a later model call fails. Only chat rows wait for success.
    let reply = run_tool_loop(db, model, &system, messages).await?;
    store_and_ingest(db, memory, channel, user_msg, &reply).await?;
    Ok(reply)
}
```

- [ ] **Step 3: Add `handle_upload_event`**

Add immediately after `handle_message`:

```rust
/// Kick off the assistant after a file upload. The model sees `seed` (the staged
/// items plus how to handle them) as the opening turn and replies naturally, but
/// chat history stores the concise `history_marker` in place of the verbose seed,
/// so a later "iya" still has the assistant's question for context. Long-term
/// memory is not consulted here — the seed already carries the resolved account.
pub async fn handle_upload_event<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    channel: &str,
    seed: &str,
    history_marker: &str,
) -> anyhow::Result<String> {
    let now_wib = chrono::Utc::now().with_timezone(&super::time::wib()).to_rfc3339();
    let system = compose_system(&now_wib, &[]);
    let history = load_history(db, channel).await;
    let messages = build_messages(&history, seed);

    let reply = run_tool_loop(db, model, &system, messages).await?;
    store_and_ingest(db, None, channel, history_marker, &reply).await?;
    Ok(reply)
}
```

- [ ] **Step 4: Verify the refactor didn't change behavior**

Run: `cd backend && cargo test --lib assistant::agent`
Expected: PASS — all existing agent tests still green (the refactor preserves the same store-on-every-exit and error-propagation behavior).

- [ ] **Step 5: Write the failing tests for `handle_upload_event`**

Add to the `tests` module in `backend/src/assistant/agent.rs` (after `unusable_response_yields_fallback_not_error`):

```rust
    #[tokio::test]
    async fn upload_event_seeds_model_and_stores_marker() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![text_response("Aku baca beli QQQM Rp2jt, catat ke IBKR ya?")]);
        let reply = handle_upload_event(
            &db, &model, "telegram",
            "SEED-CTX-XYZ beli QQQM ke IBKR",
            "(kirim 1 bukti transaksi)",
        ).await.unwrap();
        assert_eq!(reply, "Aku baca beli QQQM Rp2jt, catat ke IBKR ya?");

        // The model saw the verbose seed as the trailing user message.
        let seen = model.messages_of_call(0);
        let last = seen.last().unwrap();
        assert_eq!(last["role"], "user");
        assert!(last["content"].as_str().unwrap().contains("SEED-CTX-XYZ"), "{last:?}");

        // Chat history stores the concise marker, not the seed.
        let history = crate::repo::chat::recent_by_channel(&db, "telegram", 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "(kirim 1 bukti transaksi)");
        assert_eq!(history[1].content, "Aku baca beli QQQM Rp2jt, catat ke IBKR ya?");
    }

    #[tokio::test]
    async fn followup_after_upload_sees_prior_question() {
        let db = mem_db().await;
        let model1 = ScriptedModel::new(vec![text_response("Beli QQQM Rp2jt, catat ke IBKR ya?")]);
        handle_upload_event(&db, &model1, "telegram", "seed", "(kirim 1 bukti transaksi)").await.unwrap();

        // The owner replies "iya"; the model must see its own prior question.
        let model2 = ScriptedModel::new(vec![text_response("Sip, kecatat.")]);
        handle_message(&db, &model2, "telegram", "iya").await.unwrap();
        let seen = model2.messages_of_call(0);
        assert!(
            seen.iter().any(|m| m["content"].as_str().map_or(false, |c| c.contains("IBKR"))),
            "follow-up turn should include the prior assistant question: {seen:?}"
        );
    }
```

- [ ] **Step 6: Run tests to verify they fail, then pass**

Run: `cd backend && cargo test --lib assistant::agent`
Expected: the two new tests PASS, all prior agent tests still PASS. (If you ran before adding `handle_upload_event` in Step 3, they would fail to compile.)

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/agent.rs
git commit -m "$(cat <<'EOF'
feat(assistant): handle_upload_event to kick off natural-chat review

Extract the tool-use loop into a reply-only core and add an upload entry
point that seeds the model with the staged items but stores a concise
marker in chat history.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 5: Telegram upload handler — build the seed and call the assistant

Replace the `send_review_prompts` call with a seed-builder + `handle_upload_event` kickoff.

**Files:**
- Modify: `backend/src/telegram/mod.rs` (add `build_upload_seed` + `seed_entry_line`; rewrite the `AttachmentPick::Some` branch in `handle_update`)

- [ ] **Step 1: Add the seed builder**

Add to `backend/src/telegram/mod.rs` immediately after `fmt_payload_num` (line ~86, before the old `item_summary` which Task 6 removes):

```rust
/// Build the model-facing seed and the concise history marker for an upload.
/// The seed lists each staged item with its DB-resolved account/instrument and
/// tells the assistant to confirm naturally before writing; the marker is what
/// gets stored in chat history.
async fn build_upload_seed(db: &Db, items: &[ReviewItemRow]) -> (String, String) {
    let mut lines = String::new();
    for item in items {
        let entry: Option<ExtractedEntry> = serde_json::from_str(&item.payload_json).ok();
        let instrument = match item.suggested_instrument_id {
            Some(id) => crate::repo::instruments::get(db, id)
                .await
                .ok()
                .map(|i| format!("{} ({})", i.symbol, i.name))
                .unwrap_or_else(|| "belum dikenali".into()),
            None => "belum dikenali".into(),
        };
        let account = match item.suggested_account_id {
            Some(id) => crate::repo::accounts::get(db, id)
                .await
                .ok()
                .map(|a| a.name)
                .unwrap_or_else(|| "belum dikenali".into()),
            None => "belum dikenali".into(),
        };
        lines.push_str(&format!(
            "- #{} {} — instrumen: {instrument} — akun: {account}",
            item.id,
            seed_entry_line(entry.as_ref())
        ));
        if item.needs_attention != 0 {
            lines.push_str(" — perlu dicek (confidence rendah / data kurang)");
        }
        lines.push('\n');
    }
    let count = items.len();
    let seed = format!(
        "[event:upload] Owner baru mengirim bukti transaksi ({count} item). \
         Item review yang ter-stage:\n{lines}\
         Sapa singkat, sebut yang kamu baca, lalu minta owner mengonfirmasi akun \
         secara natural sebelum memanggil confirm_review. Kalau akun 'belum dikenali', \
         tanya akun mana — boleh create_account setelah owner setuju. Kalau instrumen \
         'belum dikenali', minta owner menambahkannya di web UI -> Data (instrumen tidak \
         bisa dibuat dari chat). JANGAN menulis transaksi tanpa 'ya' eksplisit dari owner."
    );
    let marker = format!("(kirim {count} bukti transaksi)");
    (seed, marker)
}

/// One-line entry summary for the upload seed: type, symbol, size, date.
fn seed_entry_line(entry: Option<&ExtractedEntry>) -> String {
    let Some(e) = entry else {
        return "(tidak terbaca)".to_string();
    };
    let mut out = e.entry_type.clone();
    if let Some(symbol) = &e.symbol {
        out.push_str(&format!(" {symbol}"));
    }
    let currency = e.currency.as_deref().unwrap_or("");
    if let (Some(qty), Some(price)) = (&e.quantity, &e.price_native) {
        out.push_str(&format!(" — {} @ {currency} {}", fmt_payload_num(qty), fmt_payload_num(price)));
    } else if let Some(amount) = &e.amount_native {
        out.push_str(&format!(" — nominal {currency} {}", fmt_payload_num(amount)));
    }
    out.push_str(&format!(" — {}", e.executed_at.as_deref().unwrap_or("hari ini")));
    out
}
```

- [ ] **Step 2: Rewrite the upload branch in `handle_update`**

In `backend/src/telegram/mod.rs`, replace the `AttachmentPick::Some(attachment)` arm (currently lines ~234-242) with:

```rust
            AttachmentPick::Some(attachment) => {
                match ingest_attachment(client, db, &attachment).await {
                    Ok(items) if items.is_empty() => {
                        send_or_log(client, chat_id, "Tidak ada transaksi yang terbaca dari file itu.").await;
                    }
                    Ok(items) => {
                        let (seed, marker) = build_upload_seed(db, &items).await;
                        match crate::llm::claude::ClaudeClient::from_env() {
                            Ok(llm) => {
                                let reply = crate::assistant::agent::handle_upload_event(
                                    db, &llm, "telegram", &seed, &marker,
                                )
                                .await
                                .unwrap_or_else(|e| {
                                    tracing::error!("telegram: upload kickoff failed: {e:#}");
                                    ANSWER_FAILED_REPLY.to_string()
                                });
                                send_or_log(client, chat_id, &reply).await;
                            }
                            Err(e) => {
                                tracing::error!("telegram: chat unavailable: {e:#}");
                                send_or_log(client, chat_id, ANSWER_FAILED_REPLY).await;
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("telegram: ingest failed: {e:#}");
                        send_or_log(client, chat_id, INGEST_FAILED_REPLY).await;
                    }
                }
            }
```

- [ ] **Step 3: Build**

Run: `cd backend && cargo build`
Expected: compiles. Warnings about now-unused `send_review_prompts` / `item_summary` are expected and removed in Task 6.

- [ ] **Step 4: Run the telegram tests**

Run: `cd backend && cargo test --lib telegram`
Expected: PASS — the `item_summary` tests still pass (that fn is removed in Task 6); `pick_attachment` and callback tests unaffected.

- [ ] **Step 5: Commit**

```bash
git add backend/src/telegram/mod.rs
git commit -m "$(cat <<'EOF'
feat(telegram): kick off the assistant on upload instead of buttons

Build a model-facing seed from the staged items and hand off to
handle_upload_event so unrecognized accounts get resolved in natural
chat rather than dead-ending at "lengkapi di web UI -> Data".

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 6: Remove the dead button/review-prompt code

`send_review_prompts`, `item_summary`, and the `confirm:`/`reject:` callback path are now unreachable. The `tododone:` callback stays.

**Files:**
- Modify: `backend/src/telegram/mod.rs` (delete functions, trim `CallbackAction`/`parse_callback`/`handle_callback`, delete obsolete tests)

- [ ] **Step 1: Trim `CallbackAction`**

In `backend/src/telegram/mod.rs`, change the enum (lines ~57-64) from:

```rust
pub enum CallbackAction {
    Confirm(i64),
    Reject(i64),
    /// "✅ Selesai" on a reminder notification: mark its todo done.
    TodoDone(i64),
}
```

to:

```rust
pub enum CallbackAction {
    /// "✅ Selesai" on a reminder notification: mark its todo done.
    TodoDone(i64),
}
```

- [ ] **Step 2: Trim `parse_callback`**

Change the `match action` block (lines ~71-76) from:

```rust
    match action {
        "confirm" => Some(CallbackAction::Confirm(id)),
        "reject" => Some(CallbackAction::Reject(id)),
        "tododone" => Some(CallbackAction::TodoDone(id)),
        _ => None,
    }
```

to:

```rust
    match action {
        "tododone" => Some(CallbackAction::TodoDone(id)),
        _ => None,
    }
```

Also update the doc comment above `parse_callback` to drop the `confirm:`/`reject:` mention:

```rust
/// Parse callback_data ("tododone:<todo_id>").
```

- [ ] **Step 3: Delete `item_summary` and `send_review_prompts`**

Delete the entire `item_summary` function (the `fn item_summary(...)` block, ~lines 88-130, including its doc comment) and the entire `send_review_prompts` function (`async fn send_review_prompts(...)`, ~lines 313-367, including its doc comment).

- [ ] **Step 4: Trim `handle_callback` and delete the review-callback helpers**

In `handle_callback`, replace the `let text = match action { ... }` block (lines ~390-398) with:

```rust
    let text = match action {
        CallbackAction::TodoDone(todo_id) => todo_done_text(db, todo_id).await,
    };
```

Then delete `confirm_item` (~lines 404-411), `reject_item` (~lines 413-416), and `review_callback_text` (~lines 418-422), including their doc comments.

- [ ] **Step 5: Delete the obsolete tests**

In the `#[cfg(test)] mod tests` block of `backend/src/telegram/mod.rs`:

- Replace `parses_confirm_and_reject_callbacks` (lines ~512-520) with a tododone-only version:

```rust
    #[test]
    fn parses_tododone_callback() {
        assert_eq!(parse_callback("tododone:9"), Some(CallbackAction::TodoDone(9)));
        assert_eq!(parse_callback("confirm:42"), None);
        assert_eq!(parse_callback("reject:7"), None);
        assert_eq!(parse_callback("nope:1"), None);
        assert_eq!(parse_callback("tododone:abc"), None);
        assert_eq!(parse_callback("tododone"), None);
    }
```

- Delete the two `item_summary` tests `amount_only_summary_shows_the_nominal` (~lines 565-578) and `summary_shows_the_extracted_details` (~lines 580-592).
- Delete the now-unused test helpers `review_item` (~lines 534-552), `FULL_PAYLOAD` (~lines 554-558), and `AMOUNT_ONLY_PAYLOAD` (~lines 560-563), plus the comment line above `review_item`.

- [ ] **Step 6: Build, lint, and test**

Run: `cd backend && cargo build && cargo clippy --all-targets -- -D warnings && cargo test --lib telegram`
Expected: clean build, no clippy warnings (confirms no leftover unused functions/imports — `ExtractedEntry` and `ReviewItemRow` are still used by `build_upload_seed`/`seed_entry_line`; `fmt_payload_num` by `seed_entry_line`), telegram tests PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/telegram/mod.rs
git commit -m "$(cat <<'EOF'
refactor(telegram): drop the inline-button review path

send_review_prompts, item_summary, and the confirm/reject callback arms
are dead now that uploads go through natural chat. tododone stays.

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

### Task 7: Full verification

**Files:** none (verification only)

- [ ] **Step 1: Run the full backend suite**

Run: `cd backend && cargo test`
Expected: all tests PASS.

- [ ] **Step 2: Lint and format**

Run: `cd backend && cargo clippy --all-targets -- -D warnings && cargo fmt --check`
Expected: no warnings; formatting clean. (If `cargo fmt --check` reports diffs, run `cargo fmt` and amend the relevant commit.)

- [ ] **Step 3: Sanity-check the spec is satisfied**

Confirm against the spec:
- Part A: `resolve_account` checks exact → history → fuzzy (Tasks 2-3). ✓
- Part B: uploads go through `handle_upload_event`, no buttons (Tasks 4-5). ✓
- Part C: seed instructs the assistant on unresolved instrument/account, needs_attention, and the confirm-before-write gate (Task 5). ✓
- Part D: dead code removed (Task 6). ✓

- [ ] **Step 4: Final confirmation**

No commit needed if Tasks 1-6 each committed and the suite is green. If `cargo fmt` changed files, commit:

```bash
git add -A && git commit -m "$(cat <<'EOF'
style: cargo fmt

Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Self-Review notes

- **Spec coverage:** every spec section maps to a task (verified in Task 7 Step 3). The spec's "ingest test (suggested_account_id from history)" item is intentionally dropped — `ingest_batch` takes a concrete `NativeLlmClient` with no mock seam, so refactoring it to a trait would be out-of-scope churn; `resolve_account` unit tests (Task 2) cover the logic and Task 3's swap is compile-checked.
- **Type consistency:** `accounts_for_instrument -> Vec<(i64, i64, String)>` is produced in Task 1 and consumed in Task 2; `handle_upload_event(db, model, channel, seed, marker)` is defined in Task 4 and called with the same arg order in Task 5; `build_upload_seed -> (String, String)` returns `(seed, marker)` consumed positionally in Task 5.
- **No placeholders:** every code step shows the full code; every run step shows the command and expected result.
