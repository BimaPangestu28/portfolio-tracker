# Invoice Generator (Telegram) — Design

**Date:** 2026-06-13
**Status:** Approved

## Overview

Generate a freelance invoice PDF from a chat instruction and send it back over
Telegram. The owner dictates the client and line items
("buatin invoice PT AIS: landing page 10jt, hosting & domain 2jt"); the bot
resolves (or creates) the client, assigns the next invoice number, renders a PDF
that matches the existing Catalyst Labs template, sends it as a Telegram
document, and persists the invoice.

The bot is the Rust backend (`portfolio-tracker`). PDF rendering uses Typst
embedded as a library (no external browser/binary), keeping the container light
for k3s.

## Decisions (locked during brainstorming)

- **Content source:** dictated in chat (not auto-pulled from ClickUp).
- **Render:** Typst, embedded via `typst-as-lib` + bundled fonts.
- **Delivery:** the PDF is sent back in Telegram (new `send_document`).
- **Numbering:** `INV/<YYYY>/<roman-month>/<NNN>`, NNN resets each month.
- **Clients:** stored and reused by name; first use captures the details.
- **Public-repo hygiene:** issuer + bank details live in env, never in the repo.

## Non-goals (v1)

- Pulling line items from ClickUp billable tasks (deferred; could feed
  `create_invoice` later).
- Payment-status tracking / mark-as-paid, reminders for unpaid invoices.
- A web UI for invoices (download/re-render). The invoice DATA is persisted, so
  this can be added later without schema changes.
- PPN/tax lines (template shows none); IDR only.
- Storing the rendered PDF bytes (re-render from data when needed).

## Architecture

New `invoice` module + two repos. Units:

- **`invoice::model`** — `InvoiceData`, `LineItem`, `ClientInfo`, `Issuer`. Pure
  data; no I/O.
- **`invoice::terbilang`** — `terbilang(amount: i64) -> String`: Indonesian
  number-to-words ("Dua belas juta rupiah"). Pure, heavily unit-tested.
- **`invoice::number`** — `next_number(db, now_wib) -> String`: the per-month
  sequence. Reads the latest invoice for the current YYYY+month, increments NNN
  (001 when none), formats `INV/YYYY/<roman>/NNN`. Roman month via a pure
  `roman_month(month: u32) -> &str`.
- **`invoice::render`** — `render_pdf(&InvoiceData) -> anyhow::Result<Vec<u8>>`:
  fills a Typst template (`invoice.typ`, embedded via `include_str!`) with the
  data and compiles to PDF bytes using `typst-as-lib` with bundled fonts
  (`include_bytes!`). This is the main implementation effort (Typst World +
  font setup).
- **`repo::clients`** — CRUD for the `client` table.
- **`repo::invoices`** — insert + "latest this month" query for numbering.

## Data model (two new tables)

```sql
CREATE TABLE client (
  id           INTEGER PRIMARY KEY,
  name         TEXT NOT NULL UNIQUE,   -- "PT AIS" (the dictate key, case-insensitive)
  sub_name     TEXT,                   -- "AIS Helicopter"
  website      TEXT,                   -- "www.aishelicopter.com"
  created_at   TEXT NOT NULL
);

CREATE TABLE invoice (
  id              INTEGER PRIMARY KEY,
  number          TEXT NOT NULL UNIQUE, -- "INV/2026/VI/002"
  client_id       INTEGER NOT NULL REFERENCES client(id),
  issue_date      TEXT NOT NULL,        -- RFC3339 / YYYY-MM-DD (WIB calendar)
  due_date        TEXT NOT NULL,
  subtotal        TEXT NOT NULL,        -- decimal string (IDR)
  total           TEXT NOT NULL,
  line_items_json TEXT NOT NULL,        -- [{title, body, qty, unit_price, amount}]
  created_at      TEXT NOT NULL
);
```

Line items are stored as JSON because an invoice is write-once — no separate
line table needed. The numbering query keys off `number` (LIKE `INV/2026/VI/%`)
or off `issue_date`'s YYYY-MM; the design uses the `number` prefix.

## Flow & tools (assistant agent)

New tools in `assistant::tools` / `assistant::dispatcher`:

| Tool | Input | Behavior |
|------|-------|----------|
| `list_clients` | — | List saved clients (name) so the agent reuses one. |
| `create_invoice` | `client_name`, `line_items` (array of {title, body?, qty?, amount}), `client_details?` ({sub_name?, website?}), `due_days?` | Resolve client by name (case-insensitive); if unknown, the agent first asks for details and passes `client_details`. Compute number, build `InvoiceData`, render PDF, send via Telegram `send_document`, persist the invoice. Returns the number + total. |

`line_items`: each has a `title` (the bold line, e.g. "Pengembangan Landing Page
Website"), an optional `body` (the smaller description paragraph beneath it),
`qty` (default 1), and `amount` (IDR; the line total). `unit_price = amount /
qty`. The agent parses the user's prose into this array — short dictations yield
just a title; richer ones fill the body.

Disambiguation mirrors the ClickUp project flow: unknown client → agent asks for
the details, confirms, then creates. No hard confirm-gate beyond that (an
invoice is a document, not an irreversible external write — but the agent should
echo the parsed line items + total before sending so the owner can catch typos).

## Telegram delivery

Add `send_document(chat_id, filename, bytes, caption)` to
`telegram::client::TelegramClient` (Bot API `sendDocument`, multipart upload).
The poller calls it after `render_pdf`. Filename `INV-2026-VI-002.pdf`
(slashes → dashes). On send failure, the invoice is still persisted and the
error is reported in chat.

## Configuration (env — kept out of the public repo)

The issuer and payment block come from env, defaulting to empty (the tool errors
with a clear "invoice belum dikonfigurasi" if the issuer name is unset):

- `INVOICE_ISSUER_NAME`, `INVOICE_ISSUER_COMPANY`, `INVOICE_ISSUER_WEBSITE`
- `INVOICE_ISSUER_CITY` (signature line, e.g. "Jakarta")
- `INVOICE_BANK`, `INVOICE_ACCOUNT_NO`, `INVOICE_ACCOUNT_NAME`
- `INVOICE_DUE_DAYS` (default 14)

Documented in `backend/.env.example` and `k8s/secret.example.yaml` with
placeholders only. Real values live in `backend/.env` / the k8s secret.

## Defaults

- Issue date = today (WIB). Due date = issue + `INVOICE_DUE_DAYS` (14).
- Currency IDR; no PPN line.
- Terbilang computed from the total.

## Error handling

- Missing issuer config → `create_invoice` returns a clear "belum dikonfigurasi"
  error (like the ClickUp tools); the bot keeps running.
- Render failure (Typst) → `Err(String)` surfaced to the agent; no half-sent
  state (persist happens only after a successful render).
- Duplicate number (race) → the UNIQUE constraint rejects; the handler retries
  the number query once.
- All tool errors map to `Err(String)` (existing dispatcher contract), never
  panic the poller.

## Testing

- **`terbilang`** — pure, extensive: 0, units, belasan (11–19), puluhan, ratusan,
  ribuan, juta, miliar, and the template case (12_000_000 → "dua belas juta
  rupiah"); casing as the template renders it.
- **`number`** — `roman_month` for all 12 months; `next_number` with a fake
  "latest this month" (001 when none, increments, resets across months) by
  passing `now_wib` in.
- **`render_pdf`** — smoke: non-empty PDF bytes, starts with `%PDF`; (visual
  fidelity checked manually against the template).
- **`create_invoice` handler** — fake DB + a fake document-sender seam: unknown
  client asks for details; known client reuses; line-item math (subtotal/total);
  persists with the computed number.
- **`send_document`** — DTO/multipart shape (pure builder tested; live send via
  manual smoke).

## Implementation phases (for the plan)

1. `terbilang` + `number` (pure, fully testable) + the two repos + migration.
2. `invoice::model` + `render` (Typst template + fonts) + `send_document`.
3. `create_invoice` / `list_clients` tools + env config + prompt + wiring.
