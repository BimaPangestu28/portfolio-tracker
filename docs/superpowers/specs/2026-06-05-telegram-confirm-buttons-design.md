# Telegram Inline Confirmation — Design

**Date:** 2026-06-05
**Status:** Approved

## Overview

After the bot stages review items from an ingested photo/PDF, let the owner
confirm or reject each item directly in Telegram via inline keyboard buttons,
instead of having to open the web UI. The web UI review flow stays available;
either place can act on an item, and the existing "already <status>" guard in
`review::confirm`/`reject` prevents double-processing.

## Flow

1. After `ingest_batch` stages items, the bot sends ONE message per item:
   a short summary (entry type, symbol, qty, price, date, suggested
   account/instrument names) plus inline buttons.
2. Confirmable items get `[✅ Konfirmasi] [❌ Tolak]`; items that can't be
   auto-confirmed get only `[❌ Tolak]` plus a note to complete them in the
   web UI → Data.
3. Buttons carry `callback_data` `confirm:<id>` / `reject:<id>`. The poller
   handles `callback_query` updates — only when the callback's chat is the
   linked owner chat; all others are answered (to stop the spinner) and
   ignored.
4. Confirm builds a `ConfirmPayload` from the item's `payload_json` plus the
   suggested account/instrument ids and calls the existing
   `ingestion::review::confirm`; the bot edits the message to
   "✅ Transaksi #<txn_id> dibuat" (editing drops the buttons). Reject calls
   `review::reject` and edits to "❌ Ditolak".
5. Every callback is acknowledged via `answerCallbackQuery`.

## Confirmability rules (pure function over `ReviewItemRow`)

An item is auto-confirmable when ALL hold:
- `needs_attention == 0`
- `suggested_account_id` and `suggested_instrument_id` present
- payload has `quantity`, `price_native`, `currency`
- `executed_at` parses (RFC3339, `YYYY-MM-DDTHH:MM`, or date-only → midnight
  UTC); missing `executed_at` defaults to now (mirrors the web UI default)
  and the summary says so

Otherwise: no confirm button, note pointing to the web UI.

## Client additions

- DTOs: `TgUpdate.callback_query` → `TgCallbackQuery { id, message:
  { message_id, chat }, data }`
- `send_message_with_buttons(chat_id, text, buttons)` (single-row inline
  keyboard), `answer_callback_query(id)`, `edit_message_text(chat_id,
  message_id, text)`
- Pure `build_inline_keyboard` helper for tests

## Error handling

- Confirm/reject failures (already-processed item, unknown ids, parse
  errors) edit the message with a short error and never kill the poller.
- Malformed/unknown `callback_data` is acknowledged and ignored.

## Testing

- Pure tests: callback parsing, confirmability rules + payload building
  (each missing-field case), date coercion, inline keyboard JSON shape,
  callback DTO parsing.
- Live confirm path via manual smoke.
