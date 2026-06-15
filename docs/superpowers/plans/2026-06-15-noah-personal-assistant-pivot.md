# Noah Personal Assistant Pivot — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebrand "Portfolio Tracker" into the personal assistant "Noah" — assistant-first navigation, a combined dashboard whose "Hari ini" section shows live todos/agenda/reminders/inbox, while all finance features stay intact.

**Architecture:** Add three thin read-only HTTP endpoints (`/todos`, `/reminders`, `/inbox`) that reuse existing repo functions, expose them through new React Query hooks, and render them as dashboard cards modeled on the existing `DashboardAgendaCard`. Branding and navigation changes are localized to `index.html`, `vite.config.ts`, `AppShell.tsx`, `ChatPage.tsx`, and the agent system prompt.

**Tech Stack:** Rust (axum + sqlx), React 18 + Vite + React Query + Zod, vitest + Testing Library.

**Reference spec:** `docs/superpowers/specs/2026-06-15-noah-personal-assistant-pivot-design.md`

**Branch:** `feat/noah-pivot` (already created; spec already committed there).

**Notes for the engineer:**
- Backend is a **bin-only crate**. Do NOT run `cargo test --lib` (errors) and do NOT run `cargo fmt` (rewrites ~600 files). Use `cargo test <test_name>` and `cargo check`.
- All datetimes are RFC3339 with `+07:00` (WIB). Existing helpers in `frontend/src/lib/wib.ts`: `nextDaysRangeUtc`, `todayWibKey`, `wibDayKey`, `formatWibTime`.

---

## Task 1: Backend read endpoints (`/todos`, `/reminders`, `/inbox`)

**Files:**
- Create: `backend/src/api/todos.rs`
- Create: `backend/src/api/reminders.rs`
- Create: `backend/src/api/inbox.rs`
- Modify: `backend/src/api/mod.rs` (module declarations ~lines 1-13, route registration ~line 56, router_tests ~line 135+)

Existing repo functions to reuse (already present, no SQL changes):
- `repo::todos::list_open(db) -> Vec<TodoRow>`
- `repo::reminders::list_pending(db) -> Vec<ReminderRow>`
- `repo::inbox::list_pending(db) -> Vec<InboxRow>`

All three structs already `derive(Serialize)` in their repo modules (verify; `EventRow` does and is returned directly by `events::list`). If any lacks `Serialize`, add `#[derive(serde::Serialize)]` to it in the repo file.

- [ ] **Step 1: Write the failing router-protection test**

Add to the `router_tests` module in `backend/src/api/mod.rs` (after `events_routes_are_protected`):

```rust
    #[serial]
    #[tokio::test]
    async fn assistant_read_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-assistant");
        let app = router(test_state().await);
        for uri in ["/todos", "/reminders", "/inbox"] {
            let res = app.clone().oneshot(
                Request::builder().uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd backend && cargo test assistant_read_routes_are_protected`
Expected: FAIL — either a compile error (handlers/modules don't exist yet) or `assertion failed: left == right` with `404` instead of `401` for the unregistered routes.

- [ ] **Step 3: Create the three handler files**

`backend/src/api/todos.rs`:

```rust
use crate::error::AppError;
use crate::repo::todos::{self, TodoRow};
use crate::AppState;
use axum::{extract::State, Json};

/// Open todos (status = open), ordered as the repo returns them.
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<TodoRow>>, AppError> {
    let rows = todos::list_open(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
```

`backend/src/api/reminders.rs`:

```rust
use crate::error::AppError;
use crate::repo::reminders::{self, ReminderRow};
use crate::AppState;
use axum::{extract::State, Json};

/// Pending reminders (not yet sent), ordered by remind_at.
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<ReminderRow>>, AppError> {
    let rows = reminders::list_pending(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
```

`backend/src/api/inbox.rs`:

```rust
use crate::error::AppError;
use crate::repo::inbox::{self, InboxRow};
use crate::AppState;
use axum::{extract::State, Json};

/// Pending inbox items (status = pending), newest first as the repo returns them.
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<InboxRow>>, AppError> {
    let rows = inbox::list_pending(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
```

- [ ] **Step 4: Declare modules and register routes in `backend/src/api/mod.rs`**

Add module declarations alongside the others (keep alphabetical-ish grouping near line 1-13):

```rust
pub mod inbox;
pub mod reminders;
pub mod todos;
```

In the `protected` router, right after the `/events/:id/cancel` route (line ~58), add:

```rust
        .route("/todos", get(todos::list))
        .route("/reminders", get(reminders::list))
        .route("/inbox", get(inbox::list))
```

(`get` is already imported — it's used by `/events`.)

- [ ] **Step 5: Run the protection test to verify it passes**

Run: `cd backend && cargo test assistant_read_routes_are_protected`
Expected: PASS.

- [ ] **Step 6: Compile-check the whole backend**

Run: `cd backend && cargo check`
Expected: finishes with no errors (warnings OK). If a repo struct is missing `Serialize`, add the derive in its repo file and re-run.

- [ ] **Step 7: Commit**

```bash
git add backend/src/api/todos.rs backend/src/api/reminders.rs backend/src/api/inbox.rs backend/src/api/mod.rs
git commit -m "feat(api): add read-only /todos /reminders /inbox endpoints"
```

---

## Task 2: Frontend data layer — schemas + hooks

**Files:**
- Modify: `frontend/src/api/schemas.ts` (append new schemas near the bottom)
- Modify: `frontend/src/api/hooks.ts` (append new hooks near `useEvents`, ~line 259)
- Create: `frontend/src/api/assistant-schemas.test.ts`

Schema fields mirror the Rust repo structs:
- `TodoRow`: id, title, notes?, due_at?, status, created_at, completed_at?, priority?, estimate_minutes?
- `ReminderRow`: id, todo_id?, message, remind_at, recurrence, status, sent_at?, event_id?
- `InboxRow`: id, content, status, created_at, resolved_at?

- [ ] **Step 1: Write the failing schema test**

`frontend/src/api/assistant-schemas.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import { TodoSchema, ReminderSchema, InboxItemSchema } from "./schemas";

describe("assistant schemas", () => {
  it("parses a todo row", () => {
    const t = TodoSchema.parse({
      id: 1, title: "Bayar internet", notes: null, due_at: "2026-06-15T10:00:00+07:00",
      status: "open", completed_at: null, priority: "high", estimate_minutes: 15,
      created_at: "2026-06-15T08:00:00+07:00",
    });
    expect(t.title).toBe("Bayar internet");
  });

  it("parses a reminder row", () => {
    const r = ReminderSchema.parse({
      id: 2, todo_id: null, message: "Meeting", remind_at: "2026-06-15T15:00:00+07:00",
      recurrence: "none", status: "pending", sent_at: null, event_id: null,
    });
    expect(r.message).toBe("Meeting");
  });

  it("parses an inbox row", () => {
    const i = InboxItemSchema.parse({
      id: 3, content: "Ide produk", status: "pending",
      created_at: "2026-06-15T08:00:00+07:00", resolved_at: null,
    });
    expect(i.content).toBe("Ide produk");
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/api/assistant-schemas.test.ts`
Expected: FAIL — `TodoSchema`/`ReminderSchema`/`InboxItemSchema` are not exported.

- [ ] **Step 3: Add the schemas to `frontend/src/api/schemas.ts`**

Append (after the `EventSchema` block):

```ts
export const TodoSchema = z.object({
  id: z.number(),
  title: z.string(),
  notes: z.string().nullable().optional(),
  due_at: z.string().nullable().optional(),
  status: z.string(),
  created_at: z.string(),
  completed_at: z.string().nullable().optional(),
  priority: z.string().nullable().optional(),
  estimate_minutes: z.number().nullable().optional(),
});
export type Todo = z.infer<typeof TodoSchema>;

export const ReminderSchema = z.object({
  id: z.number(),
  todo_id: z.number().nullable().optional(),
  message: z.string(),
  remind_at: z.string(),
  recurrence: z.string(),
  status: z.string(),
  sent_at: z.string().nullable().optional(),
  event_id: z.number().nullable().optional(),
});
export type Reminder = z.infer<typeof ReminderSchema>;

export const InboxItemSchema = z.object({
  id: z.number(),
  content: z.string(),
  status: z.string(),
  created_at: z.string(),
  resolved_at: z.string().nullable().optional(),
});
export type InboxItem = z.infer<typeof InboxItemSchema>;
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/api/assistant-schemas.test.ts`
Expected: PASS.

- [ ] **Step 5: Add the hooks to `frontend/src/api/hooks.ts`**

Append after the `useEvents` block (~line 264). Confirm `TodoSchema`, `ReminderSchema`, `InboxItemSchema` are imported from `./schemas` (the file imports schema names at the top — add these three to that import list):

```ts
export const useTodos = () =>
  useQuery({ queryKey: ["todos"], queryFn: () => api.get("/todos", z.array(TodoSchema)) });

export const useReminders = () =>
  useQuery({ queryKey: ["reminders"], queryFn: () => api.get("/reminders", z.array(ReminderSchema)) });

export const useInbox = () =>
  useQuery({ queryKey: ["inbox"], queryFn: () => api.get("/inbox", z.array(InboxItemSchema)) });
```

- [ ] **Step 6: Type-check + full unit test run**

Run: `cd frontend && npx tsc --noEmit && npx vitest run src/api/assistant-schemas.test.ts`
Expected: tsc clean; tests PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts frontend/src/api/assistant-schemas.test.ts
git commit -m "feat(web): add todo/reminder/inbox schemas and query hooks"
```

---

## Task 3: Dashboard cards — Todo, Reminder, Inbox

**Files:**
- Create: `frontend/src/components/DashboardTodoCard.tsx`
- Create: `frontend/src/components/DashboardReminderCard.tsx`
- Create: `frontend/src/components/DashboardInboxCard.tsx`
- Create: `frontend/src/components/DashboardTodoCard.test.tsx`
- Create: `frontend/src/components/DashboardReminderCard.test.tsx`
- Create: `frontend/src/components/DashboardInboxCard.test.tsx`

Each card mirrors `DashboardAgendaCard.tsx`: a `.card` with a header (title + "Lihat semua →" link to `/chat`), an empty state, and up to 5 rows.

- [ ] **Step 1: Write the failing Todo card test**

`frontend/src/components/DashboardTodoCard.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardTodoCard } from "./DashboardTodoCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardTodoCard /></MemoryRouter>);
}

describe("DashboardTodoCard", () => {
  it("renders open todos", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({
      data: [
        { id: 1, title: "Bayar internet", notes: null, due_at: null, status: "open", created_at: "2026-06-15T08:00:00+07:00", completed_at: null, priority: "high", estimate_minutes: null },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Bayar internet")).toBeInTheDocument());
  });

  it("shows an empty state when there are no todos", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/tidak ada todo/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/components/DashboardTodoCard.test.tsx`
Expected: FAIL — module `./DashboardTodoCard` does not exist.

- [ ] **Step 3: Create `DashboardTodoCard.tsx`**

```tsx
import { Link } from "react-router-dom";
import { useTodos } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardTodoCard() {
  const todos = useTodos();
  const rows = (todos.data ?? []).slice(0, MAX_ROWS);

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Todo hari ini</div>
        <Link to="/chat" className="text-sm text-primary hover:underline">Tanya Noah →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada todo terbuka.</p>}
        {rows.map((t) => (
          <div key={t.id} className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground w-24 shrink-0">
              {t.due_at
                ? (wibDayKey(t.due_at) === todayWibKey() ? "Hari ini" : wibDayKey(t.due_at).slice(5)) + " · " + formatWibTime(t.due_at)
                : "—"}
            </span>
            <span className="flex-1 truncate">{t.title}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/components/DashboardTodoCard.test.tsx`
Expected: PASS.

- [ ] **Step 5: Write the failing Reminder card test**

`frontend/src/components/DashboardReminderCard.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardReminderCard } from "./DashboardReminderCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardReminderCard /></MemoryRouter>);
}

describe("DashboardReminderCard", () => {
  it("renders pending reminders", async () => {
    vi.mocked(hooks.useReminders).mockReturnValue({
      data: [
        { id: 1, todo_id: null, message: "Meeting jam 3", remind_at: "2026-06-15T15:00:00+07:00", recurrence: "none", status: "pending", sent_at: null, event_id: null },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Meeting jam 3")).toBeInTheDocument());
  });

  it("shows an empty state when there are no reminders", async () => {
    vi.mocked(hooks.useReminders).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/tidak ada reminder/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 6: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/components/DashboardReminderCard.test.tsx`
Expected: FAIL — module `./DashboardReminderCard` does not exist.

- [ ] **Step 7: Create `DashboardReminderCard.tsx`**

```tsx
import { Link } from "react-router-dom";
import { useReminders } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardReminderCard() {
  const reminders = useReminders();
  const rows = (reminders.data ?? [])
    .slice()
    .sort((a, b) => a.remind_at.localeCompare(b.remind_at))
    .slice(0, MAX_ROWS);

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Reminder mendatang</div>
        <Link to="/chat" className="text-sm text-primary hover:underline">Tanya Noah →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada reminder.</p>}
        {rows.map((r) => (
          <div key={r.id} className="flex items-center gap-2 text-sm">
            <span className="text-muted-foreground w-24 shrink-0">
              {wibDayKey(r.remind_at) === todayWibKey() ? "Hari ini" : wibDayKey(r.remind_at).slice(5)} · {formatWibTime(r.remind_at)}
            </span>
            <span className="flex-1 truncate">{r.message}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 8: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/components/DashboardReminderCard.test.tsx`
Expected: PASS.

- [ ] **Step 9: Write the failing Inbox card test**

`frontend/src/components/DashboardInboxCard.test.tsx`:

```tsx
import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardInboxCard } from "./DashboardInboxCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardInboxCard /></MemoryRouter>);
}

describe("DashboardInboxCard", () => {
  it("renders pending inbox items", async () => {
    vi.mocked(hooks.useInbox).mockReturnValue({
      data: [
        { id: 1, content: "Ide produk baru", status: "pending", created_at: "2026-06-15T08:00:00+07:00", resolved_at: null },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Ide produk baru")).toBeInTheDocument());
  });

  it("shows an empty state when the inbox is clear", async () => {
    vi.mocked(hooks.useInbox).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/inbox kosong/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 10: Run test to verify it fails**

Run: `cd frontend && npx vitest run src/components/DashboardInboxCard.test.tsx`
Expected: FAIL — module `./DashboardInboxCard` does not exist.

- [ ] **Step 11: Create `DashboardInboxCard.tsx`**

```tsx
import { Link } from "react-router-dom";
import { useInbox } from "../api/hooks";

const MAX_ROWS = 5;

export function DashboardInboxCard() {
  const inbox = useInbox();
  const rows = (inbox.data ?? []).slice(0, MAX_ROWS);

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Inbox</div>
        <Link to="/chat" className="text-sm text-primary hover:underline">Tanya Noah →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Inbox kosong.</p>}
        {rows.map((i) => (
          <div key={i.id} className="flex items-center gap-2 text-sm">
            <span className="flex-1 truncate">{i.content}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 12: Run test to verify it passes**

Run: `cd frontend && npx vitest run src/components/DashboardInboxCard.test.tsx`
Expected: PASS.

- [ ] **Step 13: Commit**

```bash
git add frontend/src/components/DashboardTodoCard.tsx frontend/src/components/DashboardReminderCard.tsx frontend/src/components/DashboardInboxCard.tsx frontend/src/components/DashboardTodoCard.test.tsx frontend/src/components/DashboardReminderCard.test.tsx frontend/src/components/DashboardInboxCard.test.tsx
git commit -m "feat(web): add Todo/Reminder/Inbox dashboard cards"
```

---

## Task 4: Compose the dashboard "Hari ini" section

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx` (imports near line 41; render block ~line 1127+)

The current render order: topbar actions → `PendingReviewBanner` → hero/KPI → Alokasi/Drift → Rebalancing/Kesehatan → Komposisi → (rest). Goal: insert a "Hari ini" section (the 4 assistant cards) immediately after `PendingReviewBanner` and before the finance hero, and add a "Keuangan" sub-heading above the finance hero.

- [ ] **Step 1: Add imports**

Near the existing `import { DashboardAgendaCard } from "../components/DashboardAgendaCard";` (line 41), add:

```tsx
import { DashboardTodoCard } from "../components/DashboardTodoCard";
import { DashboardReminderCard } from "../components/DashboardReminderCard";
import { DashboardInboxCard } from "../components/DashboardInboxCard";
```

- [ ] **Step 2: Insert the "Hari ini" section and "Keuangan" heading**

In the render block, replace the comment marker for the hero section. Find (around line 1155-1158):

```tsx
      {/* ── 0. Pending review banner ──────────────────────────────────────── */}
      <PendingReviewBanner count={pendingReviews.data?.length ?? 0} />

      {/* ── 1. Hero + KPI row ──────────────────────────────────────────────── */}
```

Replace with:

```tsx
      {/* ── 0. Pending review banner ──────────────────────────────────────── */}
      <PendingReviewBanner count={pendingReviews.data?.length ?? 0} />

      {/* ── Hari ini (assistant section) ──────────────────────────────────── */}
      <div>
        <div className="t-h3" style={{ marginBottom: 12 }}>Hari ini</div>
        <div className="grid gap-5" style={{ gridTemplateColumns: "repeat(2, minmax(0,1fr))" }}>
          <DashboardTodoCard />
          <DashboardAgendaCard />
          <DashboardReminderCard />
          <DashboardInboxCard />
        </div>
      </div>

      {/* ── Keuangan ──────────────────────────────────────────────────────── */}
      <div className="t-h3" style={{ marginTop: 4 }}>Keuangan</div>

      {/* ── 1. Hero + KPI row ──────────────────────────────────────────────── */}
```

(If `DashboardAgendaCard` was already rendered elsewhere lower in the file, remove that lower usage so it only appears once — search the file for `<DashboardAgendaCard` and delete the duplicate render and any now-unused surrounding wrapper.)

- [ ] **Step 3: Type-check**

Run: `cd frontend && npx tsc --noEmit`
Expected: clean.

- [ ] **Step 4: Build to verify the page compiles**

Run: `cd frontend && npm run build`
Expected: build succeeds.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx
git commit -m "feat(web): add 'Hari ini' assistant section above finance on dashboard"
```

---

## Task 5: Branding — Noah identity

**Files:**
- Modify: `frontend/index.html`
- Modify: `frontend/vite.config.ts`
- Modify: `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Update `frontend/index.html`**

- `<title>Portfolio Tracker</title>` → `<title>Noah</title>`
- `<meta name="description" content="Lacak portofolio investasi & tanya jawab dengan asisten." />` → `<meta name="description" content="Noah — asisten pribadi: tugas, agenda, & keuangan." />`
- `<meta name="apple-mobile-web-app-title" content="Portfolio" />` → `<meta name="apple-mobile-web-app-title" content="Noah" />`
- Leave `theme-color` `#2977f5` unchanged.

- [ ] **Step 2: Update `frontend/vite.config.ts` manifest**

- `name: "Portfolio Tracker"` → `name: "Noah"`
- `short_name: "Portfolio"` → `short_name: "Noah"`
- Leave `theme_color` unchanged.

- [ ] **Step 3: Update brand + page-title fallback in `AppShell.tsx`**

- In `Sidebar` and `MobileSheet`, change the brand-mark icon from `<PieChart size={18} strokeWidth={2} />` to `<Sparkles size={18} strokeWidth={2} />` (both occurrences). `Sparkles` is already imported.
- Change both `<span className="pt-brand-name ...">Portfolio</span>` occurrences to `Noah`.
- In `usePageTitle`, change the fallback `return item?.label ?? "Portfolio";` → `return item?.label ?? "Noah";`.
- Change the footer lock-button label `Kunci portofolio` → `Kunci` (both Sidebar and MobileSheet occurrences).
- Remove the now-unused `PieChart` import if no other usage remains (run `tsc --noEmit` to confirm; only remove if it reports unused or build warns).

- [ ] **Step 4: Type-check + build**

Run: `cd frontend && npx tsc --noEmit && npm run build`
Expected: clean build; title/manifest now read "Noah".

- [ ] **Step 5: Commit**

```bash
git add frontend/index.html frontend/vite.config.ts frontend/src/components/AppShell.tsx
git commit -m "feat(web): rebrand app shell + manifest to Noah"
```

---

## Task 6: Navigation repositioning — Noah-first, two groups

**Files:**
- Modify: `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Restructure `NAV_ITEMS` into two labeled groups**

Replace the `NavItem` interface usage and the flat `NAV_ITEMS` array (and add a grouped structure). Replace:

```tsx
const NAV_ITEMS: NavItem[] = [
  { to: "/",          label: "Dashboard",  icon: LayoutDashboard, end: true },
  { to: "/portfolio", label: "Portofolio", icon: Wallet },
  { to: "/planner",   label: "Rencana",    icon: Target },
  { to: "/agenda",   label: "Agenda",     icon: CalendarDays },
  { to: "/budget",    label: "Budget",     icon: Banknote },
  { to: "/data",      label: "Data",       icon: Inbox },
  { to: "/chat",      label: "Chat",       icon: MessageSquare },
];
```

with:

```tsx
interface NavGroup {
  title: string;
  items: NavItem[];
}

const NAV_GROUPS: NavGroup[] = [
  {
    title: "Asisten",
    items: [
      { to: "/chat",   label: "Noah",      icon: Sparkles },
      { to: "/",       label: "Dashboard", icon: LayoutDashboard, end: true },
      { to: "/agenda", label: "Agenda",    icon: CalendarDays },
      { to: "/planner",label: "Rencana",   icon: Target },
      { to: "/budget", label: "Budget",    icon: Banknote },
    ],
  },
  {
    title: "Keuangan",
    items: [
      { to: "/portfolio", label: "Portofolio", icon: Wallet },
      { to: "/data",      label: "Data",       icon: Inbox },
    ],
  },
];

/** Flat list of every nav item, for lookups (page title, bottom nav). */
const NAV_ITEMS: NavItem[] = NAV_GROUPS.flatMap((g) => g.items);
```

(`MessageSquare` may now be unused — remove it from the lucide import if `tsc` flags it.)

- [ ] **Step 2: Render groups in `NavList`**

Replace the `NavList` body's `<nav className="pt-nav">...</nav>` mapping with a grouped render:

```tsx
  return (
    <nav className="pt-nav">
      {NAV_GROUPS.map((group) => (
        <div key={group.title} className="pt-nav-group">
          {!collapsed && (
            <div className="pt-nav-group-label" style={{ padding: "10px 12px 4px", fontSize: 11, textTransform: "uppercase", letterSpacing: "0.04em", color: "hsl(var(--muted-foreground))" }}>
              {group.title}
            </div>
          )}
          {group.items.map((item) => {
            const active = isActive(item);
            return (
              <NavLink
                key={item.to}
                to={item.to}
                end={item.end}
                title={collapsed ? item.label : undefined}
                className={cn("pt-nav-item", active && "active")}
                onClick={onNavigate}
              >
                <item.icon size={18} strokeWidth={active ? 2.2 : 1.8} />
                <span className="pt-nav-label">{item.label}</span>
              </NavLink>
            );
          })}
        </div>
      ))}
    </nav>
  );
```

(Keep the existing `isActive` helper defined at the top of `NavList`.)

- [ ] **Step 3: Lead the mobile bottom nav with Noah**

Change:

```tsx
const BOTTOM_KEYS = ["/", "/portfolio", "/budget", "/chat"];
```

to:

```tsx
const BOTTOM_KEYS = ["/chat", "/", "/agenda", "/budget"];
```

- [ ] **Step 4: Type-check + build**

Run: `cd frontend && npx tsc --noEmit && npm run build`
Expected: clean. Sidebar shows two groups; "Noah" is the first item; bottom nav leads with Noah.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/AppShell.tsx
git commit -m "feat(web): Noah-first grouped navigation"
```

---

## Task 7: Agent self-identity — Noah

**Files:**
- Modify: `backend/src/assistant/agent.rs` (the `SYSTEM` const, line 18)

- [ ] **Step 1: Update the system prompt opener**

Change line 18 from:

```rust
const SYSTEM: &str = "You are a personal assistant for the app owner, reachable via Telegram. \
```

to:

```rust
const SYSTEM: &str = "You are Noah, a personal assistant for the app owner, reachable via Telegram. \
```

- [ ] **Step 2: Add a test asserting the name is present**

In the existing `#[cfg(test)]` module in `agent.rs` (near `system_prompt_embeds_the_current_time`, ~line 479), add:

```rust
    #[test]
    fn system_prompt_introduces_noah() {
        let prompt = system_prompt("2026-06-11T15:00:00+07:00");
        assert!(prompt.contains("You are Noah"));
    }
```

- [ ] **Step 3: Run the test**

Run: `cd backend && cargo test system_prompt_introduces_noah`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/src/assistant/agent.rs
git commit -m "feat(assistant): Noah introduces itself by name"
```

---

## Task 8: Chat suggested prompts — broaden beyond portfolio

**Files:**
- Modify: `frontend/src/pages/ChatPage.tsx` (the suggested-prompts array, lines ~7-11)

- [ ] **Step 1: Replace the portfolio-only prompts**

Change the array (currently):

```tsx
  "Apakah saya perlu rebalancing?",
  "Performa terbaik bulan ini?",
  "Berapa XIRR saya?",
```

to a mix of assistant tasks + one finance prompt:

```tsx
  "Apa agenda saya hari ini?",
  "Ingetin meeting jam 3 sore",
  "Catat todo: bayar internet",
  "Berapa net worth saya?",
```

- [ ] **Step 2: Type-check + build**

Run: `cd frontend && npx tsc --noEmit && npm run build`
Expected: clean.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/pages/ChatPage.tsx
git commit -m "feat(web): broaden chat suggested prompts to assistant tasks"
```

---

## Final verification (end-to-end)

- [ ] **Backend:** `cd backend && cargo check && cargo test assistant_read_routes_are_protected system_prompt_introduces_noah` — all pass.
- [ ] **Frontend:** `cd frontend && npx tsc --noEmit && npx vitest run && npm run build` — type-clean, all tests pass, build succeeds.
- [ ] **Manual:** start the stack (`docker-compose up` or the project's run flow), log in, and confirm:
  - Browser tab title and PWA install name read "Noah"; sidebar brand shows "Noah" with the Sparkles mark.
  - Sidebar nav shows two groups (Asisten / Keuangan) with "Noah" first; mobile bottom nav leads with Noah.
  - Dashboard shows a "Hari ini" section (Todo / Agenda / Reminder / Inbox cards) above the "Keuangan" section; cards populate from the live API (create a todo/reminder via chat and confirm it appears).
  - In chat, Noah refers to itself as "Noah"; suggested prompts show the new assistant-oriented set.
  - All finance pages (Portfolio, Data, Budget, Planner) still work unchanged.

---

## Self-review notes

- **Spec coverage:** Branding (Task 5), nav repositioning (Task 6), dashboard recompose (Tasks 3-4), backend read endpoints (Task 1), frontend data layer (Task 2), agent identity (Task 7), chat prompts (Task 8). All seven spec sections are covered.
- **Out-of-scope respected:** no portfolio features removed; no folder/repo rename; no write actions added for todo/reminder/inbox (read-only); favicon asset redesign deferred.
- **Type consistency:** schema names `TodoSchema`/`ReminderSchema`/`InboxItemSchema` and types `Todo`/`Reminder`/`InboxItem` are used consistently across Tasks 2-3; hooks `useTodos`/`useReminders`/`useInbox` match; card components `DashboardTodoCard`/`DashboardReminderCard`/`DashboardInboxCard` match across Tasks 3-4.
