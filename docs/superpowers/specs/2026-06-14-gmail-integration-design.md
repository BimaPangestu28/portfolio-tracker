# Gmail Integration — Design

**Date:** 2026-06-14
**Status:** Approved (design), pending implementation plan
**Phase:** Productivity roadmap — Fase 5

## Overview

Calendar is already fully integrated (OAuth + two-way sync + agenda in the
briefing), so Fase 5 is **Gmail**: read and summarize important mail from chat,
surface important email in the morning briefing, and draft replies (saved as
Gmail drafts — never auto-sent). It mirrors the existing `calendar.rs` client
pattern and reuses the existing encrypted-token OAuth infrastructure.

## Goals

- "ada email penting?" → list important unread mail (sender, subject, snippet).
- "baca email dari X" / summarize a specific message.
- Draft a reply ("balas si X bilang ok meeting jam 3") → create a Gmail **draft**
  in that thread; the owner reviews and sends it in Gmail.
- A short "email penting" section in the morning briefing.

## Non-Goals (YAGNI for v1)

- No sending email from chat (drafts only; `gmail.compose`, not `gmail.send`).
- No LLM importance ranking — use Gmail's own `is:important is:unread` markers.
- No attachments, no label management, no full-text search UI beyond the
  important-unread query.
- No new DB table/migration — reuses the existing Google token storage.

## Constraints / Dependencies

- **OAuth scopes.** `oauth.rs` currently has a single `SCOPE` =
  `…/auth/calendar.events`. Extend it (space-separated) to add
  `…/auth/gmail.readonly` and `…/auth/gmail.compose`. The consent flow already
  uses `prompt=consent`, so re-running "Connect Google" re-grants with the new
  scopes. **A one-time re-consent is required**; until then the existing
  calendar-only token lacks Gmail access and Gmail calls return 403.
- **Graceful degradation.** When Google isn't connected, or the token predates
  the Gmail scope (403), the Gmail tools reply with a clear "Gmail belum
  tersambung — sambungin/sambungin ulang Google dulu" and the briefing omits the
  email section. Calendar and everything else are unaffected.

## Architecture

### OAuth scope (`src/google/oauth.rs`)

`SCOPE` becomes the three space-separated scopes (calendar.events,
gmail.readonly, gmail.compose). The existing `consent_url` URL-encodes `SCOPE`,
so no other change is needed there.

### Token access (`src/google/engine.rs` / `mod.rs`)

`ensure_access_token(db, cfg, key)` already refreshes and returns a valid access
token (private to the sync engine). Expose a public helper
`current_access_token(db) -> anyhow::Result<String>` that builds `OAuthConfig` +
the encryption key (as `run_cycle` does) and returns a fresh token, so the Gmail
tools can obtain one. Returns an error when Google isn't connected.

### Gmail client (`src/google/gmail.rs`, mirrors `calendar.rs`)

```
pub struct EmailSummary { pub id, pub thread_id, pub from, pub subject, pub snippet }
pub struct EmailDetail  { pub id, pub thread_id, pub from, pub to, pub subject, pub body }

#[async_trait] pub trait GmailApi {
    async fn list_important_unread(&self, max: u32) -> Result<Vec<EmailSummary>, GmailError>;
    async fn get_message(&self, id: &str) -> Result<EmailDetail, GmailError>;
    async fn create_draft(&self, thread_id: &str, to: &str, subject: &str, body: &str)
        -> Result<String, GmailError>; // returns draft id
}

pub struct HttpGmail { access_token: String, client: reqwest::Client }
```

- `list_important_unread`: `GET /gmail/v1/users/me/messages?q=is:unread is:important&maxResults=N`,
  then per-id `GET /messages/{id}?format=metadata&metadataHeaders=From,Subject`
  to fill sender/subject; snippet comes from the message resource.
- `get_message`: `GET /messages/{id}?format=full`; decode the text body from the
  MIME parts (prefer `text/plain`, base64url).
- `create_draft`: build an RFC822 message (`To`, `Subject`, `In-Reply-To`/thread),
  base64url-encode, `POST /drafts` with `{ message: { raw, threadId } }`.
- `GmailError` mirrors `CalendarError` (Http / Api{status,body}); a 403 is mapped
  to a distinct `ScopeMissing` so tools can prompt re-consent.

### Agent tools (`tools.rs` + `dispatcher.rs`)

Each builds an `HttpGmail` from `google::current_access_token(db)`; if that errors
(not connected) → "Gmail belum tersambung". On `GmailError::ScopeMissing` →
"sambungin ulang Google buat akses Gmail".

- `list_emails` — `{ max?: integer (default 10) }` → important-unread list:
  `from — subject — snippet` lines with ids.
- `read_email` — `{ id: string }` → the message body (for the model to summarize
  or to draft a contextual reply).
- `draft_reply` — `{ id: string, body: string }` → look up the message
  (thread/from/subject), create a draft replying to it with `body`, return
  "draft balasan disimpan di Gmail — cek & kirim dari sana". The model composes
  `body`; the draft is created directly (it cannot send).

### Briefing (`proactive/briefing.rs` + `compose.rs`)

Add an optional "email penting" section to `BriefingData` (like `clickup_due`):
`Option<Vec<EmailSummary>>` — `Some(list)` when Gmail is reachable, `None` when
not configured (section omitted). `gather` calls `list_important_unread(5)`
best-effort (errors → log + omit). `render_data_block` adds an "Email penting:"
section; `BRIEFING_SYSTEM` already instructs the model to use provided sections.

### Prompt (`agent.rs`)

`SYSTEM` gains guidance: "ada email penting?" → `list_emails`; to read/summarize a
specific one → `read_email`; "balas …" → `read_email` for context then
`draft_reply` (drafts are saved to Gmail, not sent — tell the owner to review).

## Error Handling

- Not connected / no token → friendly "Gmail belum tersambung".
- 403 (scope missing) → "sambungin ulang Google" re-consent hint.
- Network/API errors propagate via `GmailError` mapping; briefing degrades to
  omitting the section.
- `read_email`/`draft_reply` with an unknown id → the Gmail 404 surfaces as a
  clear error so the model can re-list.

## Testing

- `gmail.rs`: pure parsing of a sample messages-list + message resource into
  `EmailSummary`/`EmailDetail` (header extraction, base64url body decode); RFC822
  draft construction (`create_draft` body builder) as a pure helper, unit-tested.
  The live HTTP calls are not unit-tested (same discipline as `calendar.rs`).
- Dispatcher: `list_emails`/`read_email`/`draft_reply` against a fake `GmailApi`
  (mirrors the `FakeClickUp` pattern) — list formatting, read passthrough, draft
  creation records the body + thread; not-connected path returns the friendly
  error.
- Briefing: the "email penting" section renders when `Some`, omitted when `None`.
- Tool registration test updated with the three new names.
- `oauth.rs`: the consent URL contains all three scopes.

## Open Coordination Item

No migration. Touches `agent.rs` SYSTEM, `tools.rs` tool list, and
`proactive/briefing.rs`/`compose.rs` — the same append/merge points as the other
roadmap branches; expect trivial conflicts if merged out of order.
