# Telegram Photo Ingest — Design

**Date:** 2026-06-05
**Status:** Approved

## Overview

Let the linked owner send a photo or document (image/PDF) of a transaction to
the Telegram bot; the file runs through the existing ingestion pipeline and
lands as a review item in the web UI, exactly like a web upload. The bot
replies with a short summary. Confirmation stays in the web UI.

## Flow

1. Owner (linked chat only) sends a photo or document to the bot.
2. Poller picks the attachment: `message.photo` (largest size) or
   `message.document` with mime `image/*` / `application/pdf`.
3. `getFile` resolves the `file_id` to a path; the file is downloaded from
   `https://api.telegram.org/file/bot<token>/<file_path>` and base64-encoded.
4. The file is passed to the existing `ingestion::ingest::ingest_batch`
   (batch id `tg-<millis>`), producing pending review items.
5. Bot replies: "📥 <n> item masuk antrian review. Buka web UI → Data untuk
   konfirmasi." On failure: a short apology; the poller never dies.

## Rules

- Attachments from unlinked/foreign chats are ignored (same gate as text).
- Captions are ignored — extraction reads the image itself.
- Documents with other mime types get "format tidak didukung".
- Text-only behavior is unchanged.

## Testing

- DTO parsing tests for photo/document updates.
- Pure `pick_attachment` tests: largest photo chosen, pdf/image documents
  accepted, other mimes unsupported, text-only → none.
- Download/ingest path covered by manual smoke (needs real bot + API key).
