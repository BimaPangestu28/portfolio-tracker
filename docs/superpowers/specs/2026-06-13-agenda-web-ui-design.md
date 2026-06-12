# Agenda Web UI — Events on the Web (Full CRUD + Calendar)

## Context

The backend manages an assistant agenda (`events`, `reminders`, `todos`) that today is
only reachable through the chat assistant and proactive Telegram briefings — there is no
web surface for any of it, and no HTTP endpoint. The just-shipped Google Calendar sync
imports the user's calendar into `events` as `source='google'` rows, but the user has no
way to *see* those (or app-created events) in the web app.

This round brings the **events** surface to the web with **full CRUD**, so the user can
view and manage their agenda — and visually confirm the Google sync — without chat.
`reminders` and `todos` are deferred to a later round (see Out of Scope).

Relevant existing `events` schema (migrations 0013 + 0014):
`id, title, location?, notes?, start_at (UTC "...Z"), status (scheduled|cancelled),
created_at, source (local|google), google_event_id?, google_etag?, synced_at?, updated_at?`.

**Key property — mutations flow through the existing sync engine automatically:** any web
create/edit/cancel on an app-owned (`source='local'`) event bumps `updated_at`, so the
5-minute Google sync loop pushes/patches/deletes it on the next tick. No new sync code is
needed. `source='google'` events are read-only on the web (ownership boundary), enforced
in both the UI (no edit/cancel controls) and the backend (rejected).

## Backend API

New module `backend/src/api/events.rs`, all routes JWT-protected (mirroring the existing
CRUD routes), registered in `api/mod.rs`'s `protected` group:

- **`GET /events?from=<Z>&to=<Z>`** — scheduled events with `from <= start_at < to`,
  ordered by `start_at`. Uses the existing `repo::events::list_between`. Response items:
  `{id, title, location, notes, start_at, status, source, google_event_id}`. `from`/`to`
  are RFC3339 `Z` strings (the frontend sends the visible month's UTC bounds).
- **`POST /events`** — body `{title, start_at, location?, notes?}` → `repo::events::create`
  (creates a `source='local'` row) → returns the created row. 400 on empty title or
  unparseable `start_at`.
- **`PATCH /events/:id`** — body `{title, start_at, location?, notes?}` (full field set,
  sent prefilled from the edit dialog) → new `repo::events::update`. Updates the four
  fields and bumps `updated_at`, **only** for `source='local'` `status='scheduled'` rows.
  The handler returns the refreshed row (via `repo::events::get`) when `update` returns
  `true`, else **404** (missing, cancelled, or `source='google'` — the read-only guard).
- **`POST /events/:id/cancel`** — `repo::events::cancel` (already guards `source='local'`).
  Returns 200 on success, 404 when nothing was cancelled.

New repo function (in `backend/src/repo/events.rs`):

```rust
/// Edit an app-owned scheduled event. Bumps updated_at so the next Google sync
/// pushes the change. Refuses foreign (source='google') and non-scheduled rows.
/// Returns false when no row matched.
pub async fn update(
    db: &Db, id: i64, title: &str, location: Option<&str>, notes: Option<&str>, start_at: &str,
) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE events SET title = ?, location = ?, notes = ?, start_at = ?, updated_at = ?
         WHERE id = ? AND status = 'scheduled' AND source = 'local'",
    )
    .bind(title).bind(location).bind(notes).bind(start_at).bind(&now).bind(id)
    .execute(db).await?;
    Ok(result.rows_affected() > 0)
}
```

(Distinct from `update_from_google`, which is the sync-side writer that also sets
`synced_at`; `update` is the user-side editor that bumps `updated_at` to trigger a push.)

## Frontend

Stack: React + Vite + TypeScript + Tailwind + shadcn UI, data via React Query, tests via
vitest + MSW. API client is `frontend/src/api/client.ts` (`request(path, zodSchema, init?)`).

### API client additions (`api/client.ts`)
Zod schemas + methods, following the existing pattern:
- `listEvents(fromZ, toZ)` → `GET /events?from&to` → array of event objects.
- `createEvent({title, start_at, location?, notes?})` → `POST /events`.
- `updateEvent(id, {title, start_at, location?, notes?})` → `PATCH /events/:id`.
- `cancelEvent(id)` → `POST /events/:id/cancel`.

### Agenda page (`/agenda`, new nav item "Agenda")
- **Hand-built month grid** (7 columns Mon–Sun, WIB). No calendar library. Header with
  `‹ Month YYYY ›` navigation; "Today" cell highlighted.
- Each day cell shows an **event indicator** (a count badge or up to ~3 dots) computed from
  that WIB day's events.
- **Clicking a day** opens a **day-detail panel/list**: that day's events as rows
  (time in WIB, title, location, a **"Google"** badge when `source==='google'`). App-owned
  rows show **Edit** and **Cancel** actions; `source==='google'` rows are read-only.
- **`+ Tambah`** opens a dialog (title, date, time, location, notes) → `createEvent`.
  **Edit** opens the same dialog prefilled → `updateEvent`. **Cancel** → `cancelEvent`
  (confirm first). All mutations invalidate the events query so the grid refreshes.
- Data: one React Query per visible month — `listEvents(monthStartZ, monthEndZ)` (fetch a
  small pad around the month so edge weeks render). Loading/error via the existing
  `QueryState` component.
- New components: `pages/AgendaPage.tsx`, `components/MonthGrid.tsx`,
  `components/DayEventsPanel.tsx`, `components/EventDialog.tsx`.

### Dashboard widget (`components/DashboardAgendaCard.tsx`, mounted in `DashboardPage.tsx`)
- Compact card titled "Agenda": events from **today through the next 7 days** (WIB),
  max ~5 rows. Each row: time, title, "Google" badge when applicable.
- **"Lihat semua →"** link to `/agenda`. Empty state: "Belum ada agenda".
- Data: `listEvents(todayStartZ, plus7Z)`.

### Timezone (WIB)
Events are stored UTC `...Z`. A small frontend util (`lib/wib.ts`, offset +07:00):
- group/display events by **WIB day** and format times in WIB;
- convert the dialog's date+time inputs (interpreted as WIB) to a UTC `Z` string for
  create/update;
- compute UTC `from`/`to` bounds for a given WIB month / the next-7-days window.

### Nav + routing
Add `<Route path="agenda" element={<AgendaPage />} />` in `App.tsx` and an "Agenda" nav
item (calendar icon) in `AppShell.tsx`, placed between "Rencana" and "Chat".

## Failure Modes

- **List fetch fails** → `QueryState` error UI (existing pattern); no crash.
- **Editing/cancelling a `source='google'` event** → not offered in the UI; if attempted,
  backend returns 404 and the UI surfaces a non-fatal error/toast. Defense in depth.
- **Create/edit validation** → empty title or invalid date rejected client-side (disabled
  submit) and server-side (400).
- **Empty agenda** → explicit empty states in the widget and the day panel.
- **Mutation failure** (network/500) → error toast; the query re-fetches on success only.

## Testing

**Backend** (`backend/src/api/events.rs` + `repo/events.rs`):
- `repo::events::update` unit tests: edits an app event + bumps `updated_at`; returns false
  for a `source='google'` row (guard) and for a cancelled row.
- Route tests (in the existing `router_tests` style): `GET /events` is JWT-protected;
  `POST` creates; `PATCH` on a google-source event id returns 404; `cancel` works.

**Frontend** (vitest + MSW):
- `AgendaPage`: renders the month grid; clicking a day with events shows them; the
  "+ Tambah" dialog calls `createEvent`; a `source='google'` event shows the Google badge
  and NO edit/cancel controls.
- `DashboardAgendaCard`: renders today+upcoming rows; shows the empty state when none;
  renders the Google badge for imported events.
- `lib/wib.ts`: unit tests for WIB-day grouping and the WIB↔UTC conversions (e.g. a
  `00:30 WIB` event maps to the correct WIB day, not the UTC day).

## Out of Scope (this round)

- `reminders` and `todos` web UI (a later round).
- Recurring-event UI, drag-and-drop, week/day calendar views (month grid only).
- Editing or deleting `source='google'` events from the web (read-only by design).
- Showing cancelled events (the grid/list shows `status='scheduled'` only).
- Any change to the sync engine — mutations ride the existing 5-minute loop unchanged.
