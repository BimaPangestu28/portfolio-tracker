# Natural-chat review resolution + DB-aware account matching

**Date:** 2026-06-13
**Status:** Approved (design)
**Branch:** `feat/upwork-earnings` (to be split onto its own feature branch)

## Problem

When the owner sends a transaction screenshot/PDF to the Telegram bot, ingestion
stages a review item and tries to auto-match the account. Today that match is an
**exact, case-insensitive name match** on the LLM-extracted `account_hint` only
(`ingestion/matching.rs::suggest_account`). When the source carries no account
name, or a name that doesn't exactly equal an existing account, the account stays
unresolved. The button-based prompt (`telegram/mod.rs::send_review_prompts`) then
dead-ends the item at:

```
🧾 Review #85 — buy
Nominal: IDR 2.000.000
Tanggal: 2026-06-11T13:11:00Z
Instrumen: QQQM (Invesco NASDAQ 100 ETF)

⚠️ akun belum dikenali — lengkapi di web UI → Data.
```

Two things are wrong with this experience:

1. **It doesn't check the DB.** QQQM has been bought before; those prior
   transactions already point at an account. The system should infer the account
   from history instead of giving up.
2. **It isn't natural chat.** The owner wants to resolve this in conversation
   ("itu ke IBKR", "ya catat"), not tap buttons or be sent to the web UI.

## Goals

- Auto-resolve the account from the database far more often (instrument history +
  fuzzy name), so most items never show "belum dikenali".
- Replace the button/web-UI prompt with an **LLM-driven natural-chat** flow: after
  an upload, the assistant says what it read, states/asks the account, and confirms
  on the owner's natural reply.
- Keep the financial-safety gate: nothing is written without an explicit "ya".

## Non-goals

- WhatsApp-specific work. The WhatsApp gateway has no review-prompt path of its
  own; it forwards text to the same backend assistant and inherits both parts.
- Creating instruments from chat (still web UI → Data only).
- Changing the confirm/ledger-write logic in `ingestion/review.rs::confirm`.

## Part A — DB-aware account resolution (ingestion layer)

New function in `ingestion/matching.rs`:

```rust
pub async fn resolve_account(
    db: &Db,
    account_hint: Option<&str>,
    instrument_id: Option<i64>,
) -> anyhow::Result<Option<i64>>;
```

Resolution order, first hit wins:

1. **Exact name match** on `account_hint` — current `suggest_account` behavior,
   kept as the most reliable signal.
2. **Instrument history** — if `instrument_id` has prior transactions, pick the
   account they use. One distinct account → use it. Several → the **most
   frequently used**, tie-broken by **most recent** `executed_at`. Safe as a
   default because the assistant always asks for confirmation before writing.
3. **Fuzzy name match** on `account_hint` — case-insensitive containment
   (`name LIKE %hint%` OR `hint LIKE %name%`). Use only when it resolves to
   **exactly one** account; ambiguous (0 or >1) → skip and let the assistant ask.
   No new fuzzy-string dependency.
4. Otherwise `None`.

Call-site change in `ingestion/ingest.rs` (currently line ~124): the instrument is
resolved just before the account, so pass the resolved `sug_ins` into
`resolve_account(entry.account_hint, sug_ins)`.

New repo helper in `repo/transactions.rs`:

```rust
/// (account_id, txn_count, last_executed_at) per account that has traded this
/// instrument, ordered by count desc then last_executed_at desc.
pub async fn accounts_for_instrument(db: &Db, instrument_id: i64)
    -> anyhow::Result<Vec<(i64, i64, String)>>;
```

`suggest_account` is retained (exact match) and reused by step 1.

## Part B — LLM-driven kickoff (Telegram upload handler)

`telegram/mod.rs::handle_update` upload branch: replace the
`send_review_prompts(...)` call with an assistant kickoff.

Refactor `assistant/agent.rs`: extract the tool-use loop body into a shared core
that both the existing `handle_message` and a new `handle_upload_event` call,
parameterised by (model-facing message, stored user message).

```rust
pub async fn handle_upload_event<M: ToolModel + Sync>(
    db: &Db, model: &M, channel: &str,
    seed: &str,            // model-facing context, NOT stored verbatim
    history_marker: &str,  // concise user-role line stored in chat history
) -> anyhow::Result<String>;
```

- **seed** is built in Rust from the staged items: for each item — id, type,
  instrument label, qty/price or nominal, date, and the auto-resolved account name
  (or "belum dikenali") — plus an instruction: *greet briefly, say what you read,
  and ask the owner to confirm the account naturally before calling
  `confirm_review`.*
- **history_marker** (e.g. `"(kirim 1 bukti transaksi)"`) is stored as the user
  turn instead of the verbose seed, keeping history clean. Because the marker plus
  the assistant's opening question both land in history, the owner's later "iya"
  has the context to resolve, and the assistant can still call
  `list_pending_reviews` for exact ids.

Follow-up replies flow through the existing `answer` → `handle_message` path. The
assistant drives `confirm_review` (which already fills in `account_id` via
`set_suggestions`), `list_accounts`, and `create_account`.

### Example

```
Bot: Aku baca 1 transaksi dari foto itu — beli QQQM Rp2jt, 11 Jun.
     Biasanya QQQM ke akun IBKR ya, catat ke situ?
Owner: iya
Bot: Sip, kecatat. transaksi #91 — QQQM beli Rp2jt ke IBKR ✅
```

## Part C — Edge cases & safety

- **Unresolved instrument** (`belum dikenali`): assistant tells the owner to add it
  in web UI → Data; instruments can't be created from chat (unchanged rule).
- **Account still unresolved after Part A**: assistant asks naturally and may
  `create_account` from chat after the owner confirms.
- **`needs_attention` items** (low confidence / missing core fields): assistant
  flags and asks rather than confirming; `build_confirm_payload` already refuses
  these, so the assistant relays the reason.
- **Confirmation gate kept**: the existing system-prompt rule — ALWAYS ask before
  `create_account`/`confirm_review` — stays. Nothing financial is written without
  an explicit "ya".
- **Multiple items per upload**: the seed lists all; the assistant walks through
  them.
- **Ingest failure / "no entries"**: replies unchanged (`INGEST_FAILED_REPLY`,
  "Tidak ada transaksi yang terbaca dari file itu.").

## Part D — Cleanup

Remove now-dead code and their tests:

- `telegram/mod.rs`: `send_review_prompts`, `item_summary`, the `confirm:` /
  `reject:` callback arms in `parse_callback`, `confirm_item`, `reject_item`,
  `review_callback_text`.
- The `tododone:` callback path stays.

## Testing

- `matching::resolve_account`: exact wins; single-history; multi-history →
  most-frequent (tie-break recent); fuzzy single match; fuzzy ambiguous → `None`;
  nothing → `None`.
- `transactions::accounts_for_instrument`: ordering and counts.
- `ingest_batch`: `suggested_account_id` set from history when `account_hint` is
  absent.
- `agent::handle_upload_event`: model sees the upload context in the seed; chat
  history stores `history_marker` + reply; a follow-up "iya" turn sees the prior
  assistant question in history.
- Remove tests covering the deleted Confirm/Reject callback path; keep
  `pick_attachment` and `tododone` tests.

## Affected files

- `backend/src/ingestion/matching.rs` (new `resolve_account`)
- `backend/src/ingestion/ingest.rs` (call-site)
- `backend/src/repo/transactions.rs` (new `accounts_for_instrument`)
- `backend/src/assistant/agent.rs` (loop refactor + `handle_upload_event`)
- `backend/src/telegram/mod.rs` (upload branch rewrite + dead-code removal)
