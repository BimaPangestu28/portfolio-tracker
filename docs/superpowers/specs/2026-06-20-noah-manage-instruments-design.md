# Noah: manage instruments from chat

**Date:** 2026-06-20
**Branch:** `feat/assistant-manage-instruments`

## Problem

The Telegram assistant (Noah) can record transactions, create accounts, and
edit/delete transactions from chat, but it **cannot create instruments**. When
the owner mentions an asset that isn't registered yet (e.g. USDC), Noah hits a
dead end and tells the owner to go to the web UI → Data. This breaks the
"do it all from chat" goal: a single missing instrument blocks recording an
otherwise-valid transaction.

The block exists in three places:

- `assistant/dispatcher.rs` `create_transaction` — when the instrument name
  doesn't resolve, returns `"instrumen '{name}' belum terdaftar — tambah dulu
  di Web UI → Data"`.
- `assistant/tools.rs` `list_instruments` description — instructs the model to
  tell the user to add it in the web UI.
- `assistant/agent.rs` system prompt — `"instruments can't be created from chat"`.

The backend already has everything needed: `repo::instruments::find_or_create`
(idempotent on case-insensitive symbol), `update`, `delete`, and `txn_count`.
The gap is purely the assistant tool layer.

## Goal

Let Noah create, edit, and delete instruments from chat, so the owner never has
to touch the web UI to register a new asset. All three operations write data, so
they follow the existing **recap-then-execute** convention already used by
`create_transaction` / `edit_transaction` / `delete_transaction`: echo the
parsed change to the owner and get confirmation before calling.

## Design

### New tools (`assistant/tools.rs` + `assistant/dispatcher.rs`)

| Tool | Backend (already exists) | Notes / guardrails |
|------|--------------------------|--------------------|
| `create_instrument` | `instruments::find_or_create` | Idempotent on symbol — reuse existing if present, echo whether created or reused. Noah asks the owner for the price source first (see below). |
| `edit_instrument` | `instruments::update` | Only editable fields: `name`, `instrument_type`, `price_source`, `decimals`. **`symbol` and `native_currency` are locked** (symbol = identity/dedup key; currency would silently break cost-basis math — matches `UpdateInstrument`). |
| `delete_instrument` | `instruments::txn_count` then `instruments::delete` | **Refuses if any transaction references the instrument** — returns the count and tells the owner to delete those transactions first. Avoids a raw FK-constraint failure and accidental data loss. |

#### `create_instrument` input schema

- `symbol` (required) — dedup key, case-insensitive.
- `name` (required) — display name.
- `instrument_type` (required) — `crypto | stock_id | stock_us | etf | mutual_fund | cash | bond | gold | other`.
- `price_source` (required) — `"manual"`, or a live source like `"coingecko:usd-coin"` / `"yahoo:ASII.JK"`. **Noah asks the owner which they want before calling** (decision below).
- `native_currency` (optional, default `"IDR"`).
- `decimals` (optional, default `8`).
- `note` (optional).

#### `edit_instrument` input schema

- `id` (required) — from `list_instruments`.
- `name`, `instrument_type`, `price_source`, `decimals` (all optional; pass only
  what changes). Symbol / native currency intentionally absent.

#### `delete_instrument` input schema

- `id` (required) — from `list_instruments`.

### Price source: Noah asks each time

When creating a new instrument, Noah asks the owner whether they want **live
pricing** (coingecko for crypto, yahoo for stocks) or **manual**, instead of
guessing an external id (a wrong id silently breaks valuation). The chosen value
goes into `price_source`. Stablecoins like USDC are fine on `"manual"`.

### Unblock the three "web UI" pointers

- `dispatcher.rs` `create_transaction` error → reword to
  `"instrumen '{name}' belum terdaftar — bikin dulu pakai create_instrument"`.
  The model reads the error, calls `create_instrument`, and retries.
- `tools.rs` `list_instruments` description → replace the web-UI instruction with
  "if it genuinely doesn't exist, create it with `create_instrument` after
  confirming with the owner."
- `agent.rs` system prompt → replace `"instruments can't be created from chat"`
  with guidance to use `create_instrument`.

### End-to-end flow

> Owner: "catat deposit USDC 100 ke MetaMask"
> Noah: "USDC belum terdaftar. Mau harga live (coingecko) atau manual? Gue bikin
> instrument USDC (crypto, USD) terus catat deposit 100 ke MetaMask — oke?"
> Owner: "manual, ya"
> Noah: `create_instrument` (USDC) → `create_transaction` (deposit 100, MetaMask).

## Testing

- `tools.rs`: bump the tool-count assertion to reflect the three new tools.
- `dispatcher.rs`:
  - `create_instrument` creates a new instrument and echoes it; a second call
    with the same symbol (different case) reuses it (idempotent).
  - `edit_instrument` changes only the passed fields; symbol/currency unchanged.
  - `delete_instrument` **refuses** when a transaction references the instrument,
    and **succeeds** when none do.
- Existing system-prompt test continues to pass (and, if cheap, assert it no
  longer points at the web UI for instruments).

Backend convention: clippy + tests, **no `cargo fmt`**.

## Out of scope

- Bulk import / CSV of instruments (web UI handles that).
- Category assignment from chat (`update` supports `category_id`, but exposing
  category resolution by name to the model is deferred — not needed to unblock
  the USDC case).
- Editing an instrument's `symbol` or `native_currency` (deliberately locked).
