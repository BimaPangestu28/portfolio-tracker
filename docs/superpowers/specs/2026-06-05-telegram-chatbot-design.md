# Telegram Chatbot Integration — Design

**Date:** 2026-06-05
**Status:** Approved

## Overview

Add Telegram as a second chat channel for the portfolio assistant, with feature
parity to the existing WhatsApp channel: text-only Q&A about the portfolio,
answered by Claude with the portfolio snapshot as context.

Unlike WhatsApp (which needs the Baileys gateway service for QR pairing),
Telegram's Bot API is plain HTTP. The Rust backend talks to Telegram directly
via long-polling — no new service, no new deployment, no auth-state volume.

## Goals

- Owner can chat with the bot on Telegram and get the same answers as the
  in-app and WhatsApp channels.
- Only the owner's Telegram account can use the bot; everyone else is ignored.
- Setup requires only a bot token (from @BotFather) plus a one-time linking
  step in the web UI.

## Non-Goals

- Document/photo ingestion via Telegram.
- Proactive notifications or scheduled summaries.
- Multi-user support (the app is single-user; one linked chat).
- Webhook mode (long-polling avoids needing a public callback URL).

## Architecture

```
Telegram Bot API <--long-poll (getUpdates, ~30s timeout)-- Backend (Rust/Axum)
                                                              |
                                                              +-- service::chat::answer(channel="telegram")
                                                              +-- SQLite (chat_message, telegram_link)

Web UI --- JWT ---> GET  /telegram/status
                    POST /telegram/link-code
                    POST /telegram/unlink
```

A Tokio background task is spawned at startup **only when `TELEGRAM_BOT_TOKEN`
is set**. It long-polls `getUpdates`, processes text messages, and replies via
`sendMessage`.

## Components

### Backend: `src/telegram.rs` (or `src/telegram/` module)

- **`TelegramClient`** — small `reqwest` wrapper over the Bot API:
  - `get_updates(offset, timeout)` → `Vec<Update>`
  - `send_message(chat_id, text)`
  - Base URL `https://api.telegram.org/bot<token>`.
- **Polling loop** (spawned from `main`):
  1. `getUpdates` with the last-seen offset + 1.
  2. Filter to non-empty text messages (ignore edits, media, channels).
  3. If the sender's `chat_id` matches the linked chat → call
     `service::chat::answer(channel = "telegram")` and send the reply.
  4. If unlinked sender → check the message against the active link code
     (see Linking). Match → persist link, confirm. No match → reply once with
     a short "send the code from the web UI" hint when no link exists yet;
     ignore silently when a link already exists.
  5. Errors (network, Telegram 5xx) are logged and retried with a simple
     backoff (e.g. sleep 5s); the loop never exits. A 401 from Telegram
     (bad token) logs a clear error and marks the channel `not_configured`
     in the runtime state so the UI can surface it.

### Linking (one-time code)

- **New migration** `telegram_link` table:

  ```sql
  CREATE TABLE telegram_link (
    id INTEGER PRIMARY KEY CHECK (id = 1),  -- single row, single user
    chat_id INTEGER NOT NULL,
    username TEXT,
    linked_at TEXT NOT NULL
  );
  ```

- **Link code**: 6-digit numeric code, generated on demand, held in memory
  with a 10-minute TTL (same in-memory state pattern as `wa_state.rs`).
  Generating a new code invalidates the previous one. Codes are single-use.
- **JWT-protected endpoints** (added to the protected router):
  - `GET /telegram/status` → `{ configured: bool, linked: bool, username: string|null }`
  - `POST /telegram/link-code` → `{ code: string, expires_in: number }`
    (409 if the bot token is not configured)
  - `POST /telegram/unlink` → deletes the `telegram_link` row.
- Messages from any `chat_id` other than the linked one are ignored (after
  the link-code check above).

### Chat persistence

`chat_message.channel` gains the value `"telegram"`. The column already
exists and is free-form text — **no schema migration needed** for it.
`service::chat::answer()` already takes `channel: &str`; it is reused as-is.

### Frontend: `TelegramPage`

New page at `/telegram`, modeled on `WhatsAppPage.tsx`:

- **States** (from `GET /telegram/status`):
  - `not_configured` — token not set; show instructions to set
    `TELEGRAM_BOT_TOKEN` (create a bot via @BotFather).
  - `unlinked` — show a "Generate kode" button; once generated, display the
    6-digit code with instructions: "Kirim kode ini ke bot kamu di Telegram".
    Poll status every 2s while waiting so the page flips to `linked`
    automatically.
  - `linked` — show the linked username and an "Unlink" button.
- **API hooks** (`src/api/hooks.ts`): `useTelegramStatus()` (2s refetch),
  `useTelegramLinkCode()`, `useUnlinkTelegram()`.
- **Schema** (`src/api/schemas.ts`):

  ```ts
  const TelegramStatusSchema = z.object({
    configured: z.boolean(),
    linked: z.boolean(),
    username: z.string().nullable(),
  });
  ```

- **Nav**: add a "Telegram" item (e.g. `Send` icon from lucide) next to the
  WhatsApp entry, desktop sidebar + mobile "Lainnya".

## Error Handling

- Bad/missing token → channel reports `not_configured`; UI shows a clear
  message; polling task exits (token missing) or idles after logging (401).
- Telegram API/network failures → logged with context, retried with backoff;
  inbound messages during downtime are not lost (Telegram queues updates
  until acknowledged via offset).
- LLM failure while answering → reply with a short apology text (same
  behavior as the WhatsApp inbound handler); nothing stored (existing
  `answer()` atomicity).
- Expired/wrong link code → bot replies that the code is invalid/expired.

## Testing

- **Backend**:
  - Unit tests for link-code state: generate, expire (TTL), single-use,
    regenerate invalidates old code.
  - Handler tests for `GET /telegram/status`, `POST /telegram/link-code`,
    `POST /telegram/unlink` (auth required, not-configured cases).
  - Unit test for update filtering/dispatch: linked chat → answer path,
    unlinked chat + valid code → link, unlinked chat + bad code → rejection,
    non-text updates ignored.
- **Frontend**: `TelegramPage.test.tsx` following `WhatsAppPage.test.tsx` —
  renders each state, generate-code flow, unlink flow.

## Deployment

- `docker-compose.yml` / `docker-compose.prod.yml`: pass optional
  `TELEGRAM_BOT_TOKEN` env to the backend service.
- k8s: add `TELEGRAM_BOT_TOKEN` to the backend deployment from
  `portfolio-secrets`.
- No new service, image, PVC, or CI job.
