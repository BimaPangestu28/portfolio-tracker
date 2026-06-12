# Google Calendar Sync — Phase 1: OAuth + Events Two-Way

## Context

The assistant already manages an internal agenda: `events` (title, location, notes,
`start_at`, status), `reminders`, and `todos`, surfaced in proactive briefings and
recaps. Users want this agenda connected to Google Calendar so app-created events
appear in their normal calendar, and their real Google schedule informs the
assistant's briefings.

The full integration is **two-way** and spans three entities across two Google APIs:

- `events` + `reminders` → Google **Calendar** (primary calendar)
- `todos` → Google **Tasks**

Because this is large, it is **phased**. Each phase ships and is tested independently:

- **Phase 1 (this spec):** Google OAuth connection + `events` two-way sync with the
  primary calendar, plus pull-only import of foreign Google events for briefing
  awareness. This phase de-risks OAuth and the sync engine first.
- **Phase 2 (later spec):** `reminders` → Calendar.
- **Phase 3 (later spec):** `todos` → Google Tasks.

The existing `connectors` module is financial-only (`evm_wallet`, `binance`, tied to
an `account_id`); Google does not fit it. Google integration is its own module under
`backend/src/google/`, but follows established patterns: a thin reqwest HTTP client
like `llm/`, a mockable client trait like `ToolModel`, incremental sync via a stored
token (like the connector `cursor`), and execution from the existing 5-minute
proactive tick rather than a new scheduler.

Single-user app: exactly one connected Google account.

## Ownership Model

Two-way sync only touches app-owned items. Safety boundary on the user's primary
calendar:

- App-created Google events are tagged `extendedProperties.private.app = "portfolio"`.
- Only tagged events are ever patched or deleted by the app.
- All other (foreign) primary-calendar events are **pull-only**: imported read-only
  for awareness, never modified or deleted by the app.

## Data Model

### New table `google_integration` (single row)

```sql
CREATE TABLE google_integration (
  id INTEGER PRIMARY KEY CHECK (id = 1),     -- single-row guard
  access_token TEXT NOT NULL,
  refresh_token TEXT NOT NULL,               -- sensitive; encrypted at rest (see below)
  expiry TEXT NOT NULL,                       -- RFC3339 UTC access-token expiry
  scope TEXT NOT NULL,
  calendar_sync_token TEXT,                   -- Google events.list nextSyncToken (incremental)
  status TEXT NOT NULL DEFAULT 'connected'
    CHECK (status IN ('connected', 'disconnected', 'error')),
  last_error TEXT,                            -- last failure reason for the UI banner
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
```

`refresh_token` (and `access_token`) are encrypted at rest with AES-GCM using a key
from `GOOGLE_TOKEN_ENC_KEY` (32-byte, base64). If the env key is unset the integration
refuses to connect (fail closed) rather than storing plaintext tokens.

### Changes to `events`

```sql
ALTER TABLE events ADD COLUMN source TEXT NOT NULL DEFAULT 'local'
  CHECK (source IN ('local', 'google'));   -- 'local' = app-owned (two-way); 'google' = foreign (read-only)
ALTER TABLE events ADD COLUMN google_event_id TEXT;   -- Google event id once synced
ALTER TABLE events ADD COLUMN google_etag TEXT;       -- for If-Match optimistic concurrency
ALTER TABLE events ADD COLUMN synced_at TEXT;         -- last successful sync (nullable)
ALTER TABLE events ADD COLUMN updated_at TEXT;        -- last app-side edit; drives conflict resolution
```

`updated_at` is set on every app-side create/update/cancel. Existing rows backfill
`updated_at = created_at` in the migration.

`source = 'google'` rows are foreign imports: the agenda read paths (briefing/recap)
include them unchanged, but the assistant's event tools refuse to edit or cancel them,
and the sync engine never pushes them back.

## OAuth & Connection

Single Google account, web-frontend-initiated consent.

Routing note: Caddy strips the `/api` prefix before the backend, so backend routes are
registered at root (e.g. `/chat`, `/health`); the public URL adds `/api`. Endpoints
below show the backend route, with the public URL noted where it matters.

- **Frontend:** Settings → Integrations → "Connect Google" / "Disconnect", with a
  status indicator (connected / error-with-reason / disconnected).
- **`GET /google/oauth/start`** *(protected — JWT)* — builds the Google consent URL with
  scope `https://www.googleapis.com/auth/calendar.events`, `access_type=offline`,
  `prompt=consent` (to guarantee a refresh token), and an HMAC-signed, short-lived
  `state`; returns the URL for the SPA to redirect the browser to.
- **`GET /google/oauth/callback?code&state`** *(public — no JWT)* — Google redirects the
  browser here as a top-level navigation that cannot carry the SPA's JWT, so this route
  lives in the **public** router group and is guarded by the signed `state` (CSRF +
  origin check) instead. It validates `state`, exchanges the code for tokens, encrypts
  and stores them in `google_integration`, sets `status='connected'`, and redirects back
  to the frontend Settings page. Public callback URL (and the Google redirect URI):
  `https://portfolio.catalystlabs.id/api/google/oauth/callback`.
- **`POST /google/disconnect`** *(protected — JWT)* — revokes the token with Google,
  deletes the `google_integration` row, and leaves existing `source='google'` events in
  place (see Out of Scope).
- **Token refresh:** before each sync, if `expiry` is within a 60s skew, refresh using
  the refresh token and persist the new access token + expiry.

The Google Cloud Console redirect URI must exactly equal `GOOGLE_REDIRECT_URI`
(`https://portfolio.catalystlabs.id/api/google/oauth/callback`).

## Sync Engine

Runs from the existing 5-minute proactive tick, only when `status = 'connected'`.
The reconciler is a set of **pure functions** that take current app + Google state and
return a list of operations; the tick executes the operations through the HTTP client.
Pure-function design keeps the diff logic fully unit-testable without network access.

**Step 1 — Refresh token if needed.** On refresh failure (revoked/invalid_grant), set
`status='error'`, record `last_error`, and stop this cycle. The tick never panics on a
Google failure.

**Step 2 — Outbound (app → Google).** Select `source='local'` events changed since
`synced_at` (or never synced):

- No `google_event_id` → **create** in Google, tag `extendedProperties.private.app=portfolio`,
  store `google_event_id` + `etag` + `synced_at`.
- `updated_at > synced_at` → **patch** the Google event, sending `If-Match: <etag>`.
- `status='cancelled'` → **delete** the Google event; keep the app row.

**Step 3 — Inbound (Google → app).** Call `events.list` with `calendar_sync_token`
(incremental) bounded to a forward window (`timeMin = now`, `timeMax = now + 30d`):

- Tagged (`app=portfolio`) Google event changed → update the matching `source='local'`
  app row; if deleted in Google → set the app row `cancelled`.
- Foreign Google event → upsert a `source='google'` app row (read-only); if deleted in
  Google → remove it from the app.
- Persist the returned `nextSyncToken`. If Google returns **410 Gone** (expired token),
  clear the token and full-resync the window next cycle.

### Conflict Resolution

Last-write-wins per item: compare app `updated_at` against the Google event `updated`
timestamp; the newer wins. `etag` + `If-Match` on patch prevents lost updates on the
Google side (a 412 Precondition Failed means Google changed first → re-pull that item
and apply Google's version).

## Failure Modes

- **Refresh fails / token revoked** → `status='error'`, `last_error` set, UI banner
  prompts re-connect, sync paused until reconnected.
- **Rate limit (429) / 5xx** → skip the rest of this cycle; retry next tick. All
  operations are idempotent (create is keyed by absence of `google_event_id`; patch is
  etag-guarded), so retries are safe.
- **Per-item failure** → logged with context, does not block other items in the cycle.
- **410 Gone on sync token** → drop token, full-resync the window.
- **Missing `GOOGLE_TOKEN_ENC_KEY`** → connection refused (fail closed); no plaintext
  tokens are ever written.

## Testing

- **Unit (core): reconciler.** Given `(app_events, google_events, mapping)`, assert the
  exact op list. Cases: new local event → create; edited local → patch; cancelled local
  → delete; foreign event → read-only import; both-sides edit → last-write-wins; deleted
  in Google → cancel/remove per ownership; 410 → full-resync path.
- **Unit:** token refresh boundary (expiry within skew vs not), `extendedProperties`
  tag write/parse, AES-GCM encrypt/decrypt round-trip.
- **Mockable client:** Google Calendar HTTP access behind a trait (like `ToolModel`), so
  the reconciler and tick step are tested without network.
- **Live smoke** (`#[ignore]`): end-to-end create/patch/delete + inbound import against a
  dedicated test Google account, gated on env presence.

## Manual Setup (one-time)

1. Google Cloud Console → create/select a project → enable the **Google Calendar API**.
2. Configure the OAuth consent screen (External, single test user = the owner).
3. Create an **OAuth Client ID** (type: Web application).
4. Add redirect URI: `https://portfolio.catalystlabs.id/api/google/oauth/callback`.
5. Set backend env / k8s secret: `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET`,
   `GOOGLE_REDIRECT_URI`, `GOOGLE_TOKEN_ENC_KEY` (base64 32-byte).

## Out of Scope (this phase)

- `reminders` → Calendar (Phase 2) and `todos` → Google Tasks (Phase 3).
- Webhook/push notifications (Google `watch` channels) — polling on the tick is used;
  realtime can be added later without changing the data model.
- Multiple Google accounts / multi-user.
- Secondary or dedicated calendars — Phase 1 targets the primary calendar only.
- Syncing event attendees, recurrence rules, attachments, conferencing, colors — only
  title, location, notes, and start time are mapped in Phase 1.
- On disconnect, imported `source='google'` rows are kept as-is (they simply stop
  updating). A cleanup option can be a small follow-up if desired.
