# Agenda Web UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring the `events` agenda (including Google-synced events) to the web with full CRUD: backend REST endpoints, a month-grid Agenda page with day-detail CRUD, and a Dashboard agenda widget.

**Architecture:** New JWT-protected `events` REST endpoints reuse the existing `repo::events` (+ a new `update` fn); app-owned mutations ride the existing 5-minute Google sync loop automatically, while `source='google'` events are read-only (enforced in repo + UI). Frontend follows the established React Query hooks (`api/hooks.ts`) + zod schema (`api/schemas.ts`) + `Dialog`/shadcn patterns; a small WIB timezone util handles UTC↔WIB.

**Tech Stack:** Rust (axum, sqlx/SQLite), React + TypeScript + Vite + Tailwind + shadcn, @tanstack/react-query, zod, vitest + @testing-library/react.

**Spec:** `docs/superpowers/specs/2026-06-13-agenda-web-ui-design.md`

**Conventions (follow these):**
- Backend repo fns return `anyhow::Result<T>`; handlers return `Result<Json<T>, AppError>` (`AppError` has `NotFound`→404, `BadRequest(String)`→400, `Other`→500). Timestamps `chrono::Utc::now().to_rfc3339()`. Tests: `crate::db::connect("sqlite::memory:")`, inline `#[cfg(test)]`.
- Frontend data: hooks in `src/api/hooks.ts` (`useQuery` + `useInvalidatingMutation(fn, keys)`), zod schemas in `src/api/schemas.ts`, dialogs via `src/components/Dialog.tsx`, toasts via `sonner`. Run frontend tests: `cd frontend && npx vitest run <file>`; typecheck `npx tsc --noEmit`.
- The backend `EventRow` serializes all its fields; zod `.object()` strips unknown keys, so the frontend `EventSchema` can validate the subset it needs.

---

## Task 1: Backend — `repo::events::update`

**Files:**
- Modify: `backend/src/repo/events.rs`

- [ ] **Step 1: Write the failing tests** — append to the `tests` module in `backend/src/repo/events.rs`:
```rust
    #[tokio::test]
    async fn update_edits_local_and_bumps_updated_at() {
        let db = mem_db().await;
        let e = create(&db, "old", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        let before = get(&db, e.id).await.unwrap();
        assert!(update(&db, e.id, "new", Some("kantor"), Some("catatan"), "2026-06-14T02:00:00Z").await.unwrap());
        let after = get(&db, e.id).await.unwrap();
        assert_eq!(after.title, "new");
        assert_eq!(after.location.as_deref(), Some("kantor"));
        assert_eq!(after.start_at, "2026-06-14T02:00:00Z");
        // updated_at advanced past synced_at-less baseline so the sync loop re-pushes.
        assert!(after.updated_at.as_deref().unwrap() >= before.updated_at.as_deref().unwrap());
    }

    #[tokio::test]
    async fn update_refuses_foreign_and_cancelled() {
        let db = mem_db().await;
        let fid = upsert_foreign(&db, "g-1", "foreign", None, None, "2026-06-13T03:00:00Z", "etag").await.unwrap();
        assert!(!update(&db, fid, "hack", None, None, "2026-06-13T03:00:00Z").await.unwrap());
        assert_eq!(get(&db, fid).await.unwrap().title, "foreign");

        let e = create(&db, "x", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        cancel(&db, e.id).await.unwrap();
        assert!(!update(&db, e.id, "y", None, None, "2026-06-13T07:00:00Z").await.unwrap());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test repo::events::tests::update -- --nocapture`
Expected: FAIL — `update` not found.

- [ ] **Step 3: Implement** — add to `backend/src/repo/events.rs` (after `cancel`):
```rust
/// Edit an app-owned scheduled event. Bumps updated_at so the next Google sync
/// pushes the change. Refuses foreign (source='google') and non-scheduled rows.
/// Returns false when no row matched.
pub async fn update(
    db: &Db,
    id: i64,
    title: &str,
    location: Option<&str>,
    notes: Option<&str>,
    start_at: &str,
) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "UPDATE events SET title = ?, location = ?, notes = ?, start_at = ?, updated_at = ?
         WHERE id = ? AND status = 'scheduled' AND source = 'local'",
    )
    .bind(title)
    .bind(location)
    .bind(notes)
    .bind(start_at)
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test repo::events:: -- --nocapture`
Expected: PASS (all events tests).

- [ ] **Step 5: Commit**
```bash
git add backend/src/repo/events.rs
git commit -m "feat(agenda): events repo update fn (app-owned edit, foreign guard)"
```

---

## Task 2: Backend — `api/events.rs` endpoints + routes

**Files:**
- Create: `backend/src/api/events.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Add a failing route test** — append to the `router_tests` module in `backend/src/api/mod.rs`:
```rust
    #[serial]
    #[tokio::test]
    async fn events_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-events");
        let app = router(test_state().await);
        let res = app.oneshot(
            Request::builder().uri("/events?from=2026-06-01T00:00:00Z&to=2026-07-01T00:00:00Z")
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd backend && cargo test router_tests::events -- --nocapture`
Expected: FAIL — route not registered.

- [ ] **Step 3: Create `backend/src/api/events.rs`:**
```rust
use crate::error::AppError;
use crate::repo::events::{self, EventRow};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct RangeQuery {
    pub from: String,
    pub to: String,
}

/// Scheduled events with from <= start_at < to (both RFC3339 Z), ordered by start.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<RangeQuery>,
) -> Result<Json<Vec<EventRow>>, AppError> {
    let rows = events::list_between(&s.db, &q.from, &q.to)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct EventIn {
    pub title: String,
    pub start_at: String,
    pub location: Option<String>,
    pub notes: Option<String>,
}

fn validate(b: &EventIn) -> Result<(), AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    chrono::DateTime::parse_from_rfc3339(&b.start_at)
        .map_err(|_| AppError::BadRequest("start_at bukan RFC3339 valid".into()))?;
    Ok(())
}

pub async fn create(
    State(s): State<AppState>,
    Json(b): Json<EventIn>,
) -> Result<Json<EventRow>, AppError> {
    validate(&b)?;
    let row = events::create(&s.db, &b.title, b.location.as_deref(), b.notes.as_deref(), &b.start_at)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(row))
}

pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<EventIn>,
) -> Result<Json<EventRow>, AppError> {
    validate(&b)?;
    let ok = events::update(&s.db, id, &b.title, b.location.as_deref(), b.notes.as_deref(), &b.start_at)
        .await
        .map_err(AppError::Other)?;
    if !ok {
        // Missing, cancelled, or source='google' (read-only).
        return Err(AppError::NotFound);
    }
    let row = events::get(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(row))
}

pub async fn cancel(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let ok = events::cancel(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 4: Register module + routes in `backend/src/api/mod.rs`:**
- Add to the module declarations at the top (alphabetical, after `crud`): `pub mod events;`
- Add these to the `protected` router (e.g. after the `/google/disconnect` line, before `/accounts`):
```rust
        .route("/events", get(events::list).post(events::create))
        .route("/events/:id", axum::routing::patch(events::update))
        .route("/events/:id/cancel", post(events::cancel))
```

- [ ] **Step 5: Run to verify pass**

Run: `cd backend && cargo test router_tests:: -- --nocapture && cargo test 2>&1 | tail -3`
Expected: router tests PASS (incl. the new events one); full suite 0 failures.

- [ ] **Step 6: Commit**
```bash
git add backend/src/api/events.rs backend/src/api/mod.rs
git commit -m "feat(agenda): events REST endpoints (list/create/update/cancel)"
```

---

## Task 3: Frontend — WIB timezone util

**Files:**
- Create: `frontend/src/lib/wib.ts`, `frontend/src/lib/wib.test.ts`

- [ ] **Step 1: Write the failing tests** — create `frontend/src/lib/wib.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import { wibDayKey, formatWibTime, wibDateTimeToUtcZ, monthGridDays, gridRangeUtc, nextDaysRangeUtc } from "./wib";

describe("wib util", () => {
  it("wibDayKey shifts UTC into the WIB calendar day", () => {
    // 2026-06-12T19:00:00Z == 2026-06-13 02:00 WIB -> day 2026-06-13
    expect(wibDayKey("2026-06-12T19:00:00Z")).toBe("2026-06-13");
    expect(wibDayKey("2026-06-13T07:00:00Z")).toBe("2026-06-13");
  });

  it("formatWibTime renders HH:MM in WIB", () => {
    expect(formatWibTime("2026-06-13T00:00:00Z")).toBe("07:00");
    expect(formatWibTime("2026-06-12T19:30:00Z")).toBe("02:30");
  });

  it("wibDateTimeToUtcZ converts a WIB wall-clock to UTC Z (no millis)", () => {
    expect(wibDateTimeToUtcZ("2026-06-13", "07:00")).toBe("2026-06-13T00:00:00Z");
    expect(wibDateTimeToUtcZ("2026-06-13", "02:30")).toBe("2026-06-12T19:30:00Z");
  });

  it("monthGridDays returns 42 Mon-started day keys covering the month", () => {
    const days = monthGridDays(2026, 6); // June 2026; 1 Jun 2026 is a Monday
    expect(days.length).toBe(42);
    expect(days[0]).toBe("2026-06-01"); // first cell is Monday 1 Jun
    expect(days).toContain("2026-06-30");
    expect(days[7]).toBe("2026-06-08");
  });

  it("gridRangeUtc spans first day 00:00 WIB to day-after-last 00:00 WIB", () => {
    const r = gridRangeUtc(["2026-06-01", "2026-06-02"]);
    expect(r.fromZ).toBe("2026-05-31T17:00:00Z"); // 1 Jun 00:00 WIB
    expect(r.toZ).toBe("2026-06-02T17:00:00Z");   // 3 Jun 00:00 WIB (exclusive end of 2 Jun)
  });

  it("nextDaysRangeUtc covers today 00:00 WIB through +n days", () => {
    const r = nextDaysRangeUtc("2026-06-13", 7);
    expect(r.fromZ).toBe("2026-06-12T17:00:00Z"); // 13 Jun 00:00 WIB
    expect(r.toZ).toBe("2026-06-19T17:00:00Z");   // 20 Jun 00:00 WIB
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npx vitest run src/lib/wib.test.ts`
Expected: FAIL — module missing.

- [ ] **Step 3: Implement** — create `frontend/src/lib/wib.ts`:
```ts
// WIB (Asia/Jakarta) is a fixed UTC+7 offset (no DST). Events are stored as UTC
// "...Z"; the UI groups and displays them in WIB and converts form input back to UTC.
const WIB_OFFSET_MS = 7 * 60 * 60 * 1000;

function pad(n: number): string {
  return String(n).padStart(2, "0");
}

/** UTC "...Z" -> "YYYY-MM-DD" of the instant in the WIB calendar. */
export function wibDayKey(utcZ: string): string {
  const d = new Date(new Date(utcZ).getTime() + WIB_OFFSET_MS);
  return d.toISOString().slice(0, 10);
}

/** UTC "...Z" -> "HH:MM" in WIB. */
export function formatWibTime(utcZ: string): string {
  const d = new Date(new Date(utcZ).getTime() + WIB_OFFSET_MS);
  return d.toISOString().slice(11, 16);
}

/** WIB wall-clock (date "YYYY-MM-DD", time "HH:MM") -> UTC "...:SSZ" (no millis). */
export function wibDateTimeToUtcZ(dateStr: string, timeStr: string): string {
  const [y, m, d] = dateStr.split("-").map(Number);
  const [hh, mm] = timeStr.split(":").map(Number);
  const utcMs = Date.UTC(y, m - 1, d, hh, mm) - WIB_OFFSET_MS;
  return new Date(utcMs).toISOString().replace(/\.\d{3}Z$/, "Z");
}

/** "YYYY-MM-DD" (WIB) -> UTC "...Z" for 00:00 WIB that day. */
function wibDayStartUtcZ(dayKey: string): string {
  return wibDateTimeToUtcZ(dayKey, "00:00");
}

/** Add `n` days to a "YYYY-MM-DD" key (calendar math, TZ-agnostic). */
function addDays(dayKey: string, n: number): string {
  const [y, m, d] = dayKey.split("-").map(Number);
  const dt = new Date(Date.UTC(y, m - 1, d + n));
  return dt.toISOString().slice(0, 10);
}

/**
 * The 42 day keys ("YYYY-MM-DD") of a Monday-started 6-week grid containing the
 * given WIB month. `month` is 1-12.
 */
export function monthGridDays(year: number, month: number): string[] {
  const first = new Date(Date.UTC(year, month - 1, 1));
  // getUTCDay: 0=Sun..6=Sat. Convert to Monday-start offset (Mon=0..Sun=6).
  const mondayOffset = (first.getUTCDay() + 6) % 7;
  const start = new Date(Date.UTC(year, month - 1, 1 - mondayOffset));
  const days: string[] = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(start.getTime() + i * 24 * 60 * 60 * 1000);
    days.push(d.toISOString().slice(0, 10));
  }
  return days;
}

/** UTC range [first day 00:00 WIB, (last day + 1) 00:00 WIB) for a list of day keys. */
export function gridRangeUtc(days: string[]): { fromZ: string; toZ: string } {
  const first = days[0];
  const last = days[days.length - 1];
  return { fromZ: wibDayStartUtcZ(first), toZ: wibDayStartUtcZ(addDays(last, 1)) };
}

/** UTC range covering today 00:00 WIB through +n days (exclusive end), WIB. */
export function nextDaysRangeUtc(todayKey: string, n: number): { fromZ: string; toZ: string } {
  return { fromZ: wibDayStartUtcZ(todayKey), toZ: wibDayStartUtcZ(addDays(todayKey, n)) };
}

/** Today's WIB day key, from the current instant. */
export function todayWibKey(): string {
  return wibDayKey(new Date().toISOString());
}
```

- [ ] **Step 4: Run to verify pass**

Run: `cd frontend && npx vitest run src/lib/wib.test.ts && npx tsc --noEmit`
Expected: PASS (6 tests); typecheck clean.

- [ ] **Step 5: Commit**
```bash
git add frontend/src/lib/wib.ts frontend/src/lib/wib.test.ts
git commit -m "feat(agenda): WIB timezone util (day grouping, UTC<->WIB, grid ranges)"
```

---

## Task 4: Frontend — event schema + React Query hooks

**Files:**
- Modify: `frontend/src/api/schemas.ts`, `frontend/src/api/hooks.ts`

- [ ] **Step 1: Add the schema** — append to `frontend/src/api/schemas.ts`:
```ts
export const EventSchema = z.object({
  id: z.number(),
  title: z.string(),
  location: z.string().nullable().optional(),
  notes: z.string().nullable().optional(),
  start_at: z.string(),
  status: z.string(),
  source: z.string(),
  google_event_id: z.string().nullable().optional(),
});
export type EventItem = z.infer<typeof EventSchema>;
```

- [ ] **Step 2: Add the hooks** — append to `frontend/src/api/hooks.ts` (the file already imports `z`, `useQuery`, `api`, and defines `useInvalidatingMutation`). Import the schema by adding `EventSchema` to the existing `from "./schemas"` import, then:
```ts
export const useEvents = (fromZ: string, toZ: string) =>
  useQuery({
    queryKey: ["events", fromZ, toZ],
    queryFn: () =>
      api.get(`/events?from=${encodeURIComponent(fromZ)}&to=${encodeURIComponent(toZ)}`, z.array(EventSchema)),
  });

type EventBody = { title: string; start_at: string; location?: string | null; notes?: string | null };

export const useCreateEvent = () =>
  useInvalidatingMutation((b: EventBody) => api.post("/events", EventSchema, b), ["events"]);

export const useUpdateEvent = () =>
  useInvalidatingMutation(
    (args: { id: number; patch: EventBody }) => api.patch(`/events/${args.id}`, EventSchema, args.patch),
    ["events"],
  );

export const useCancelEvent = () =>
  useInvalidatingMutation((id: number) => api.post(`/events/${id}/cancel`, z.unknown(), {}), ["events"]);
```

- [ ] **Step 3: Verify typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: clean (no output). (No new unit test here — exercised by the component tests in Tasks 8-9.)

- [ ] **Step 4: Commit**
```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts
git commit -m "feat(agenda): EventSchema + event query/mutation hooks"
```

---

## Task 5: Frontend — EventDialog (create/edit form)

**Files:**
- Create: `frontend/src/components/EventDialog.tsx`

- [ ] **Step 1: Implement** — create `frontend/src/components/EventDialog.tsx` (mirrors `AddTransactionDialog.tsx`: `Dialog` wrapper, local form state, sonner toast, hook mutation). It supports create (no `event`) and edit (`event` provided):
```tsx
import { useEffect, useState } from "react";
import { toast } from "sonner";
import { Dialog } from "./Dialog";
import { useCreateEvent, useUpdateEvent } from "../api/hooks";
import type { EventItem } from "../api/schemas";
import { wibDateTimeToUtcZ, wibDayKey, formatWibTime } from "../lib/wib";

interface EventDialogProps {
  open: boolean;
  onClose: () => void;
  /** Edit mode when provided; create mode otherwise. */
  event?: EventItem | null;
  /** WIB day pre-selected for a new event (create mode), "YYYY-MM-DD". */
  defaultDay?: string;
}

export function EventDialog({ open, onClose, event, defaultDay }: EventDialogProps) {
  const create = useCreateEvent();
  const update = useUpdateEvent();
  const isEdit = !!event;

  const blank = {
    title: "",
    date: defaultDay ?? new Date().toISOString().slice(0, 10),
    time: "09:00",
    location: "",
    notes: "",
  };
  const [form, setForm] = useState(blank);

  // Re-seed the form whenever the dialog opens for a different event/day.
  useEffect(() => {
    if (!open) return;
    if (event) {
      setForm({
        title: event.title,
        date: wibDayKey(event.start_at),
        time: formatWibTime(event.start_at),
        location: event.location ?? "",
        notes: event.notes ?? "",
      });
    } else {
      setForm({ ...blank, date: defaultDay ?? blank.date });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, event, defaultDay]);

  const set = (k: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.title.trim()) {
      toast.error("Judul wajib diisi");
      return;
    }
    const body = {
      title: form.title.trim(),
      start_at: wibDateTimeToUtcZ(form.date, form.time),
      location: form.location.trim() || null,
      notes: form.notes.trim() || null,
    };
    const opts = {
      onSuccess: () => {
        toast.success(isEdit ? "Agenda diperbarui" : "Agenda ditambahkan");
        onClose();
      },
      onError: (err: unknown) => toast.error((err as Error).message),
    };
    if (isEdit && event) update.mutate({ id: event.id, patch: body }, opts);
    else create.mutate(body, opts);
  };

  return (
    <Dialog
      open={open}
      onClose={onClose}
      title={isEdit ? "Edit Agenda" : "Tambah Agenda"}
      sub="Acara di kalender pribadimu"
      footer={
        <>
          <button type="button" className="btn btn-outline" onClick={onClose}>
            Batal
          </button>
          <button type="submit" form="event-form" className="btn btn-primary">
            {isEdit ? "Simpan" : "Tambah"}
          </button>
        </>
      }
    >
      <form id="event-form" onSubmit={submit} className="space-y-3">
        <label className="block text-sm">
          Judul
          <input className="input mt-1 w-full" value={form.title} onChange={set("title")} autoFocus />
        </label>
        <div className="flex gap-3">
          <label className="block text-sm flex-1">
            Tanggal
            <input type="date" className="input mt-1 w-full" value={form.date} onChange={set("date")} />
          </label>
          <label className="block text-sm w-32">
            Jam (WIB)
            <input type="time" className="input mt-1 w-full" value={form.time} onChange={set("time")} />
          </label>
        </div>
        <label className="block text-sm">
          Lokasi (opsional)
          <input className="input mt-1 w-full" value={form.location} onChange={set("location")} />
        </label>
        <label className="block text-sm">
          Catatan (opsional)
          <textarea className="input mt-1 w-full" rows={2} value={form.notes} onChange={set("notes")} />
        </label>
      </form>
    </Dialog>
  );
}
```

NOTE: confirm `Dialog`'s props by reading `frontend/src/components/Dialog.tsx` and the `btn`/`input` utility classes in the global CSS (used by `AddTransactionDialog`). If `Dialog` requires children differently or the submit button can't target the form by `id`, adapt to match the existing dialog usage while keeping the same fields/behavior.

- [ ] **Step 2: Verify typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 3: Commit**
```bash
git add frontend/src/components/EventDialog.tsx
git commit -m "feat(agenda): EventDialog create/edit form (WIB-aware)"
```

---

## Task 6: Frontend — MonthGrid

**Files:**
- Create: `frontend/src/components/MonthGrid.tsx`

- [ ] **Step 1: Implement** — create `frontend/src/components/MonthGrid.tsx` (pure presentational; counts events per WIB day):
```tsx
import { useMemo } from "react";
import { ChevronLeft, ChevronRight } from "lucide-react";
import type { EventItem } from "../api/schemas";
import { monthGridDays, wibDayKey, todayWibKey } from "../lib/wib";
import { cn } from "@/lib/utils";

const MONTHS = ["Januari","Februari","Maret","April","Mei","Juni","Juli","Agustus","September","Oktober","November","Desember"];
const DOW = ["Sn","Sl","Rb","Km","Jm","Sb","Mg"];

interface MonthGridProps {
  year: number;
  month: number; // 1-12
  events: EventItem[];
  selectedDay: string | null; // "YYYY-MM-DD"
  onSelectDay: (day: string) => void;
  onPrevMonth: () => void;
  onNextMonth: () => void;
}

export function MonthGrid({ year, month, events, selectedDay, onSelectDay, onPrevMonth, onNextMonth }: MonthGridProps) {
  const days = useMemo(() => monthGridDays(year, month), [year, month]);
  const today = todayWibKey();

  const countByDay = useMemo(() => {
    const m = new Map<string, number>();
    for (const e of events) {
      const k = wibDayKey(e.start_at);
      m.set(k, (m.get(k) ?? 0) + 1);
    }
    return m;
  }, [events]);

  return (
    <div className="rounded-lg border p-3">
      <div className="flex items-center justify-between mb-2">
        <button className="btn btn-outline btn-icon" aria-label="Bulan sebelumnya" onClick={onPrevMonth}>
          <ChevronLeft size={16} />
        </button>
        <div className="font-medium">{MONTHS[month - 1]} {year}</div>
        <button className="btn btn-outline btn-icon" aria-label="Bulan berikutnya" onClick={onNextMonth}>
          <ChevronRight size={16} />
        </button>
      </div>
      <div className="grid grid-cols-7 gap-1 text-center text-xs text-muted-foreground mb-1">
        {DOW.map((d) => <div key={d}>{d}</div>)}
      </div>
      <div className="grid grid-cols-7 gap-1">
        {days.map((day) => {
          const inMonth = Number(day.slice(5, 7)) === month;
          const count = countByDay.get(day) ?? 0;
          return (
            <button
              key={day}
              onClick={() => onSelectDay(day)}
              className={cn(
                "aspect-square rounded-md border text-sm flex flex-col items-center justify-center gap-0.5",
                inMonth ? "" : "text-muted-foreground/50",
                day === today ? "ring-1 ring-primary" : "",
                day === selectedDay ? "bg-accent" : "hover:bg-accent/50",
              )}
            >
              <span>{Number(day.slice(8, 10))}</span>
              {count > 0 && <span className="h-1.5 w-1.5 rounded-full bg-primary" aria-label={`${count} agenda`} />}
            </button>
          );
        })}
      </div>
    </div>
  );
}
```

NOTE: `cn` is imported from `@/lib/utils` (used by `DashboardPage.tsx`). `btn`/`btn-outline`/`btn-icon` are existing global classes; if `btn-icon` doesn't exist, use plain padding classes consistent with other icon buttons in the app.

- [ ] **Step 2: Verify typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 3: Commit**
```bash
git add frontend/src/components/MonthGrid.tsx
git commit -m "feat(agenda): MonthGrid calendar (WIB day counts, month nav)"
```

---

## Task 7: Frontend — DayEventsPanel

**Files:**
- Create: `frontend/src/components/DayEventsPanel.tsx`

- [ ] **Step 1: Implement** — create `frontend/src/components/DayEventsPanel.tsx`:
```tsx
import { Plus, Pencil, X } from "lucide-react";
import type { EventItem } from "../api/schemas";
import { formatWibTime, wibDayKey } from "../lib/wib";
import { Badge } from "@/components/ui/badge";

interface DayEventsPanelProps {
  day: string; // "YYYY-MM-DD" WIB
  events: EventItem[]; // all loaded events; this panel filters to `day`
  onAdd: () => void;
  onEdit: (e: EventItem) => void;
  onCancel: (e: EventItem) => void;
}

export function DayEventsPanel({ day, events, onAdd, onEdit, onCancel }: DayEventsPanelProps) {
  const dayEvents = events
    .filter((e) => wibDayKey(e.start_at) === day)
    .sort((a, b) => a.start_at.localeCompare(b.start_at));

  return (
    <div className="rounded-lg border p-3 space-y-2">
      <div className="flex items-center justify-between">
        <h3 className="font-medium">{day}</h3>
        <button className="btn btn-primary btn-sm" onClick={onAdd}>
          <Plus size={14} /> Tambah
        </button>
      </div>

      {dayEvents.length === 0 && (
        <p className="text-sm text-muted-foreground">Tidak ada agenda hari ini.</p>
      )}

      <ul className="space-y-1">
        {dayEvents.map((e) => {
          const isGoogle = e.source === "google";
          return (
            <li key={e.id} className="flex items-center gap-2 rounded-md border px-2 py-1.5">
              <span className="text-sm tabular-nums w-12">{formatWibTime(e.start_at)}</span>
              <span className="flex-1 text-sm truncate">
                {e.title}
                {e.location ? <span className="text-muted-foreground"> · {e.location}</span> : null}
              </span>
              {isGoogle && <Badge variant="secondary">Google</Badge>}
              {!isGoogle && (
                <>
                  <button className="btn btn-ghost btn-icon" aria-label="Edit" onClick={() => onEdit(e)}>
                    <Pencil size={14} />
                  </button>
                  <button className="btn btn-ghost btn-icon" aria-label="Batalkan" onClick={() => onCancel(e)}>
                    <X size={14} />
                  </button>
                </>
              )}
            </li>
          );
        })}
      </ul>
    </div>
  );
}
```

NOTE: `Badge` is at `@/components/ui/badge`. Confirm `btn-sm`/`btn-ghost`/`btn-icon` exist; otherwise use the closest existing button classes. Behavior that matters (and is tested): Google events show the "Google" badge and have NO Edit/Batalkan buttons.

- [ ] **Step 2: Verify typecheck**

Run: `cd frontend && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 3: Commit**
```bash
git add frontend/src/components/DayEventsPanel.tsx
git commit -m "feat(agenda): DayEventsPanel (per-day list, google read-only)"
```

---

## Task 8: Frontend — AgendaPage + route + nav (+ test)

**Files:**
- Create: `frontend/src/pages/AgendaPage.tsx`, `frontend/src/pages/AgendaPage.test.tsx`
- Modify: `frontend/src/App.tsx`, `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Write the failing test** — create `frontend/src/pages/AgendaPage.test.tsx` (mock the hooks layer, matching the `GoogleCalendarCard.test.tsx` precedent):
```tsx
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import AgendaPage from "./AgendaPage";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

const mockEvents = [
  { id: 1, title: "Meeting vendor", location: "kantor", notes: null, start_at: "2026-06-13T02:00:00Z", status: "scheduled", source: "local", google_event_id: null },
  { id: 2, title: "Dokter gigi", location: null, notes: null, start_at: "2026-06-13T07:00:00Z", status: "scheduled", source: "google", google_event_id: "g-1" },
];

beforeEach(() => {
  vi.mocked(hooks.useEvents).mockReturnValue({ data: mockEvents, isLoading: false, isError: false } as any);
  vi.mocked(hooks.useCreateEvent).mockReturnValue({ mutate: vi.fn() } as any);
  vi.mocked(hooks.useUpdateEvent).mockReturnValue({ mutate: vi.fn() } as any);
  vi.mocked(hooks.useCancelEvent).mockReturnValue({ mutate: vi.fn() } as any);
});

describe("AgendaPage", () => {
  it("shows a day's events when its grid cell is clicked, with a Google badge and no edit on google events", async () => {
    render(<AgendaPage />);
    // 13 Jun 2026 is in the default month view only if today is June 2026; force-select via the day cell labelled "13".
    fireEvent.click(screen.getByText("13"));
    await waitFor(() => expect(screen.getByText("Meeting vendor")).toBeInTheDocument());
    expect(screen.getByText("Dokter gigi")).toBeInTheDocument();
    expect(screen.getByText("Google")).toBeInTheDocument();
    // The google event row has no "Edit" control; the local one does.
    expect(screen.getAllByLabelText("Edit").length).toBe(1);
  });
});
```

NOTE: the test assumes the grid shows June 2026 (the events' month). In `AgendaPage`, initialize the visible month from `todayWibKey()`. For the test to be deterministic regardless of the real date, have `AgendaPage` accept an optional `initialDay?: string` prop (defaulting to `todayWibKey()`) and pass `initialDay="2026-06-13"` in the test render: `render(<AgendaPage initialDay="2026-06-13" />)`. Add that prop.

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npx vitest run src/pages/AgendaPage.test.tsx`
Expected: FAIL — page missing.

- [ ] **Step 3: Implement** — create `frontend/src/pages/AgendaPage.tsx`:
```tsx
import { useMemo, useState } from "react";
import { toast } from "sonner";
import { MonthGrid } from "../components/MonthGrid";
import { DayEventsPanel } from "../components/DayEventsPanel";
import { EventDialog } from "../components/EventDialog";
import { QueryState } from "../components/QueryState";
import { useEvents, useCancelEvent } from "../api/hooks";
import { monthGridDays, gridRangeUtc, todayWibKey } from "../lib/wib";
import type { EventItem } from "../api/schemas";

interface AgendaPageProps {
  initialDay?: string; // "YYYY-MM-DD" WIB; defaults to today (testability)
}

export default function AgendaPage({ initialDay }: AgendaPageProps) {
  const start = initialDay ?? todayWibKey();
  const [year, setYear] = useState(Number(start.slice(0, 4)));
  const [month, setMonth] = useState(Number(start.slice(5, 7)));
  const [selectedDay, setSelectedDay] = useState<string>(start);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [editing, setEditing] = useState<EventItem | null>(null);

  const range = useMemo(() => gridRangeUtc(monthGridDays(year, month)), [year, month]);
  const events = useEvents(range.fromZ, range.toZ);
  const cancel = useCancelEvent();

  const prevMonth = () => {
    if (month === 1) { setYear(year - 1); setMonth(12); } else setMonth(month - 1);
  };
  const nextMonth = () => {
    if (month === 12) { setYear(year + 1); setMonth(1); } else setMonth(month + 1);
  };

  const openCreate = () => { setEditing(null); setDialogOpen(true); };
  const openEdit = (e: EventItem) => { setEditing(e); setDialogOpen(true); };
  const onCancel = (e: EventItem) => {
    if (!confirm(`Batalkan "${e.title}"?`)) return;
    cancel.mutate(e.id, {
      onSuccess: () => toast.success("Agenda dibatalkan"),
      onError: (err) => toast.error((err as Error).message),
    });
  };

  const data = events.data ?? [];

  return (
    <div className="space-y-4">
      <h1 className="text-lg font-semibold">Agenda</h1>
      <QueryState query={events}>
        <div className="grid gap-4 md:grid-cols-2">
          <MonthGrid
            year={year}
            month={month}
            events={data}
            selectedDay={selectedDay}
            onSelectDay={setSelectedDay}
            onPrevMonth={prevMonth}
            onNextMonth={nextMonth}
          />
          <DayEventsPanel
            day={selectedDay}
            events={data}
            onAdd={openCreate}
            onEdit={openEdit}
            onCancel={onCancel}
          />
        </div>
      </QueryState>
      <EventDialog
        open={dialogOpen}
        onClose={() => setDialogOpen(false)}
        event={editing}
        defaultDay={selectedDay}
      />
    </div>
  );
}
```

NOTE: read `frontend/src/components/QueryState.tsx` to confirm its prop shape (it may take `query={...}` or `state={...}` and render children when loaded). Adapt the `<QueryState>` usage to its actual API; the test mocks `useEvents` to a loaded state so children render.

- [ ] **Step 4: Add route + nav**
- In `frontend/src/App.tsx`, add an import `import AgendaPage from "./pages/AgendaPage";` and a route inside the `<Route element={<AppShell />}>` block: `<Route path="agenda" element={<AgendaPage />} />`.
- In `frontend/src/components/AppShell.tsx`, add to the `NAV` array (after the Planner item, before Chat) — import a calendar icon from lucide-react (e.g. `CalendarDays`):
```tsx
  { to: "/agenda",    label: "Agenda",     icon: CalendarDays },
```

- [ ] **Step 5: Run to verify pass**

Run: `cd frontend && npx vitest run src/pages/AgendaPage.test.tsx && npx tsc --noEmit`
Expected: PASS (1 test); typecheck clean.

- [ ] **Step 6: Commit**
```bash
git add frontend/src/pages/AgendaPage.tsx frontend/src/pages/AgendaPage.test.tsx frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(agenda): Agenda page (month grid + day CRUD) + nav route"
```

---

## Task 9: Frontend — Dashboard agenda widget (+ test)

**Files:**
- Create: `frontend/src/components/DashboardAgendaCard.tsx`, `frontend/src/components/DashboardAgendaCard.test.tsx`
- Modify: `frontend/src/pages/DashboardPage.tsx`

- [ ] **Step 1: Write the failing test** — create `frontend/src/components/DashboardAgendaCard.test.tsx`:
```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardAgendaCard } from "./DashboardAgendaCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardAgendaCard /></MemoryRouter>);
}

describe("DashboardAgendaCard", () => {
  it("renders upcoming events with a Google badge", async () => {
    vi.mocked(hooks.useEvents).mockReturnValue({
      data: [
        { id: 1, title: "Standup", location: null, notes: null, start_at: "2026-06-13T02:00:00Z", status: "scheduled", source: "local", google_event_id: null },
        { id: 2, title: "Dokter", location: null, notes: null, start_at: "2026-06-13T07:00:00Z", status: "scheduled", source: "google", google_event_id: "g-1" },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Standup")).toBeInTheDocument());
    expect(screen.getByText("Google")).toBeInTheDocument();
  });

  it("shows an empty state when there are no events", async () => {
    vi.mocked(hooks.useEvents).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/belum ada agenda/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run to verify failure**

Run: `cd frontend && npx vitest run src/components/DashboardAgendaCard.test.tsx`
Expected: FAIL — component missing.

- [ ] **Step 3: Implement** — create `frontend/src/components/DashboardAgendaCard.tsx`:
```tsx
import { useMemo } from "react";
import { Link } from "react-router-dom";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Badge } from "@/components/ui/badge";
import { useEvents } from "../api/hooks";
import { nextDaysRangeUtc, todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const UPCOMING_DAYS = 7;
const MAX_ROWS = 5;

export function DashboardAgendaCard() {
  const range = useMemo(() => nextDaysRangeUtc(todayWibKey(), UPCOMING_DAYS), []);
  const events = useEvents(range.fromZ, range.toZ);

  const rows = (events.data ?? [])
    .slice()
    .sort((a, b) => a.start_at.localeCompare(b.start_at))
    .slice(0, MAX_ROWS);

  return (
    <Card>
      <CardHeader className="flex flex-row items-center justify-between">
        <CardTitle>Agenda</CardTitle>
        <Link to="/agenda" className="text-sm text-primary hover:underline">Lihat semua →</Link>
      </CardHeader>
      <CardContent className="space-y-1">
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Belum ada agenda.</p>}
        {rows.map((e) => (
          <div key={e.id} className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground w-24 shrink-0">
              {wibDayKey(e.start_at) === todayWibKey() ? "Hari ini" : wibDayKey(e.start_at).slice(5)} · {formatWibTime(e.start_at)}
            </span>
            <span className="flex-1 truncate">{e.title}</span>
            {e.source === "google" && <Badge variant="secondary">Google</Badge>}
          </div>
        ))}
      </CardContent>
    </Card>
  );
}
```

- [ ] **Step 4: Mount in the dashboard** — in `frontend/src/pages/DashboardPage.tsx`, add `import { DashboardAgendaCard } from "../components/DashboardAgendaCard";` and render `<DashboardAgendaCard />` within the dashboard's card grid/layout (place it alongside the other cards — read the file to find the section/grid and insert it consistently).

- [ ] **Step 5: Run to verify pass**

Run: `cd frontend && npx vitest run src/components/DashboardAgendaCard.test.tsx && npx tsc --noEmit`
Expected: PASS (2 tests); typecheck clean.

- [ ] **Step 6: Full frontend + backend suite**

Run:
```bash
cd frontend && npx vitest run 2>&1 | tail -4 && npx tsc --noEmit && npm run build 2>&1 | tail -3
cd ../backend && cargo test 2>&1 | tail -3
```
Expected: all frontend tests pass, typecheck clean, production build succeeds; backend suite 0 failures.

- [ ] **Step 7: Commit**
```bash
git add frontend/src/components/DashboardAgendaCard.tsx frontend/src/components/DashboardAgendaCard.test.tsx frontend/src/pages/DashboardPage.tsx
git commit -m "feat(agenda): Dashboard agenda widget (today + upcoming)"
```

---

## Self-Review Notes

- **Spec coverage:** GET/POST/PATCH/cancel endpoints (Task 2) · `repo::update` + foreign guard (Task 1) · Agenda page month grid + day CRUD (Tasks 5-8) · Dashboard widget (Task 9) · WIB handling (Task 3, used throughout) · Google read-only in UI (Task 7) + backend (Tasks 1-2) · nav/route (Task 8) · testing backend (Tasks 1-2) + frontend (Tasks 3, 8, 9). All spec sections map to a task.
- **Mutations ride the existing sync loop:** create/update bump `updated_at`, cancel sets cancelled → the Google sync engine pushes/patches/deletes next tick. No sync code touched (per spec).
- **Type consistency:** `EventItem` (Task 4) is the single FE event type used by all components; `EventSchema` validates the backend `EventRow` subset (extra keys stripped by zod). The four hook names (`useEvents`/`useCreateEvent`/`useUpdateEvent`/`useCancelEvent`) are used identically in Tasks 5/8/9. `wib.ts` exports (Task 3) are consumed unchanged in Tasks 5-9.
- **Adaptation notes flagged inline:** `Dialog` props, `QueryState` prop shape, and a few utility class names (`btn-*`) must be confirmed against the actual files during implementation — each is called out in the relevant task.
