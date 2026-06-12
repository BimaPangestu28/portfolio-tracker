# Assistant-Driven Review Confirmation — Design

**Date:** 2026-06-12
**Status:** Approved

## Problem

Two flows that should cooperate are disconnected:

1. **Photo/PDF ingest → inline buttons.** When a staged review item has no
   matched account or instrument, `build_confirm_payload` rejects it, so the
   Telegram prompt shows only `[❌ Tolak]` plus "lengkapi di web UI". The item
   dead-ends — the owner cannot finish it from chat.
2. **Text chat → assistant agent.** The agent's tools are todos, reminders,
   portfolio summary, memory, and events. It has **no** awareness of pending
   review items. So "masukin transaksi yang saya kirim tadi" cannot see the
   staged item and instead asks the user to re-send details.

Account matching in `ingestion::matching` is exact-name only
(`WHERE LOWER(name) = LOWER(?)`). In the motivating case (a Nanovest order for
QQQM) the **instrument matched by name** but the **account did not** — most
likely because no "Nanovest" account exists yet. Fuzzy matching cannot help
when the account simply does not exist; the resolution is to create it.

## Goal

Make the assistant agent a single conversational surface for resolving and
confirming pending review items — including creating the missing account —
so the natural request "masukin transaksi tadi" works end to end:

> "Akun Nanovest belum ada, mau aku buatin & masukin transaksinya?" → "iya" →
> "✅ Transaksi #N dibuat."

## Scope

In scope:
- Move the confirmability/payload logic into a shared location.
- One repo method to fill in previously-unknown account/instrument ids.
- Five assistant tools: list pending reviews, list accounts, create account,
  confirm review, reject review.
- System-prompt guidance for the resolve-then-confirm flow, with a hard rule
  to confirm with the user before any write.

Out of scope (YAGNI, deferred):
- `create_instrument` tool. Instruments already match by name; when an
  instrument is unknown the agent points the user to the web UI → Data. The
  `confirm_review` tool still accepts an `instrument_id` override for the rare
  case the user supplies it.
- Fuzzy / alias account matching in the ingest pipeline. Separate optimization
  to reduce how often resolution is needed; not required to close this gap.
- Telegram inline account-picker buttons. The chat agent covers this case;
  adding callback state for the same outcome is redundant.

## Components

### 1. Shared confirm helper (refactor)

Move `build_confirm_payload(&ReviewItemRow) -> Result<ConfirmPayload, String>`
and its `to_rfc3339` helper from `telegram/mod.rs` into `ingestion::review`
(the module that already owns `confirm`/`reject` and `ConfirmPayload`). Both
`telegram` and the assistant dispatcher import the one definition — the rule
for "when is an item confirmable" lives in exactly one place. The moved unit
tests travel with it.

### 2. Repo: fill in resolved suggestions

`review_items::set_suggestions(db, id, account_id: Option<i64>, instrument_id:
Option<i64>) -> anyhow::Result<ReviewItemRow>`

Updates only the provided fields (a `None` argument leaves that column
unchanged), returns the refreshed row. Persisting (rather than passing the
override straight into the payload) means a resolved item also becomes
confirmable from the original Telegram button, keeping both surfaces
consistent. Existing methods (`get`, `list_by_status`, `mark_confirmed`,
`mark_rejected`) are reused unchanged.

### 3. Assistant tools

Schemas added to `assistant::tools::definitions()`; handlers added as match
arms in `assistant::dispatcher`.

- **`list_pending_reviews`** — no input. Lists `list_by_status("pending")`
  items. Each line: review id, entry type, instrument label (or
  "❓ belum dikenali"), account name (or "❓ belum dikenali"), qty/price or
  nominal, date, and a blocker note when account/instrument is unknown.
- **`list_accounts`** — no input. Returns id, name, type for every account so
  the agent reuses an existing account instead of creating a duplicate.
- **`create_account`** — input: `name`, `account_type`, `native_currency`
  (required); `institution`, `note` (optional). Maps to
  `repo::accounts::create(NewAccount{..})`; returns the new id and name.
- **`confirm_review`** — input: `review_id` (required); `account_id`,
  `instrument_id` (optional overrides). When an override is given, call
  `set_suggestions`, reload the row, then `build_confirm_payload` + `confirm`.
  On success returns "Transaksi #N dibuat". When the item is still incomplete
  (missing account/instrument, `needs_attention`, missing core fields) returns
  the same human-readable reason `build_confirm_payload` produces, so the agent
  can explain what is still needed.
- **`reject_review`** — input: `review_id`. Calls `review::reject`; returns a
  short confirmation.

### 4. System prompt guidance (`agent.rs`)

Describe the resolve-then-confirm flow: for "masukin transaksi yang aku
kirim" / "konfirmasi transaksinya", call `list_pending_reviews`; when an
account shows ❓, call `list_accounts` and, if none fits, propose
`create_account`; then `confirm_review`. **Hard rule:** always confirm with the
user before calling `create_account` or `confirm_review` — both write
financial data that cannot be silently undone.

## Data flow (motivating case)

1. User: "masukin transaksi yang saya kirim tadi."
2. Agent → `list_pending_reviews` → sees Review #84 buy QQQM, account
   "❓ belum dikenali".
3. Agent → `list_accounts` → no Nanovest account.
4. Agent asks the user: create a "Nanovest" account and confirm?
5. User agrees → agent → `create_account(name="Nanovest", ...)` → new id.
6. Agent → `confirm_review(review_id=84, account_id=<new>)` →
   `set_suggestions` → `build_confirm_payload` → `confirm` → txn created.
7. Agent: "✅ Transaksi #N dibuat."

## Error handling

- `confirm_review` on an already-processed item, unknown id, or still-missing
  fields returns a clear reason string; the agent relays it. Tool errors feed
  back into the loop (existing agent behavior) — never panic the handler.
- Unknown tool name stays the existing dispatcher error.
- `create_account` validation errors (e.g. DB constraint) surface as the tool
  error string.

## Testing

Dispatcher tests against an in-memory DB:
- `list_pending_reviews` formats a pending item and flags an unknown account.
- `create_account` creates and returns the new account.
- `confirm_review` with an `account_id` override on a previously-unmatched
  item creates the transaction.
- `confirm_review` on a still-incomplete item returns the blocker reason and
  creates no transaction.
- `reject_review` marks the item rejected.
- Moved `build_confirm_payload` / `to_rfc3339` tests stay green in their new
  home; `tools::definitions()` schema test covers the five new names.
