# Noah Inline Write Actions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the user complete a todo, quick-add a todo, cancel a reminder, and resolve an inbox item directly from the dashboard "Hari ini" cards — no chat required.

**Architecture:** Four new write endpoints reuse existing repo functions and mirror the `events::create`/`events::cancel` handlers; four React Query mutation hooks use the existing `useInvalidatingMutation` helper to refresh the relevant card after each action; the three existing dashboard cards gain inline controls (a complete button + quick-add form on Todo, a cancel button on Reminder, a resolve button on Inbox). No new migrations.

**Tech Stack:** Rust (axum + sqlx), React 18 + React Query + Zod, sonner toasts, vitest + Testing Library.

**Reference spec:** `docs/superpowers/specs/2026-06-15-noah-write-actions-design.md`

**Branch:** `feat/noah-write-actions` (already created off `feat/noah-pivot`; spec already committed there).

**Notes for the engineer:**
- Backend is a **bin-only crate**. Do NOT run `cargo test --lib` (errors) and do NOT run `cargo fmt`. Use `cargo test <name>` and `cargo check`.
- The read endpoints/handlers (`api/todos.rs`, `api/reminders.rs`, `api/inbox.rs`, each with a `list` fn) and the read hooks/cards already exist from the pivot branch — you are ADDING to them.
- Repo write fns already exist: `todos::create(db, title, notes, due_at, priority, estimate_minutes)`, `todos::complete(db, id) -> bool`, `reminders::cancel(db, id) -> bool`, `inbox::resolve(db, ids: &[i64], status) -> u64`.
- Existing FE helpers: `useInvalidatingMutation(fn, keys)` (returns a useMutation result), `api.post(path, schema, body)`. Pattern reference: `useCreateEvent`/`useCancelEvent` in `hooks.ts`. CSS classes `input` and `pt-icon-btn` are defined in `index.css`. Toast: `import { toast } from "sonner"`.

---

## Task 1: Backend write endpoints

**Files:**
- Modify: `backend/src/api/todos.rs` (add `create` + `complete`)
- Modify: `backend/src/api/reminders.rs` (add `cancel`)
- Modify: `backend/src/api/inbox.rs` (add `resolve`)
- Modify: `backend/src/api/mod.rs` (register 4 routes + protection test)

Pattern reference — `backend/src/api/events.rs`:
```rust
#[derive(Deserialize)]
pub struct EventIn { pub title: String, /* ... */ }

fn validate(b: &EventIn) -> Result<(), AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    Ok(())
}

pub async fn cancel(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = events::cancel(&s.db, id).await.map_err(AppError::Other)?;
    if !ok { return Err(AppError::NotFound); }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 1: Write the failing protection test**

In the `router_tests` module of `backend/src/api/mod.rs`, add (the existing `assistant_read_routes_are_protected` test is nearby — note POST routes must be requested with `.method("POST")`):

```rust
    #[serial]
    #[tokio::test]
    async fn assistant_write_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-assistant-write");
        let app = router(test_state().await);
        let cases = [
            ("/todos", "POST"),
            ("/todos/1/complete", "POST"),
            ("/reminders/1/cancel", "POST"),
            ("/inbox/1/resolve", "POST"),
        ];
        for (uri, method) in cases {
            let res = app.clone().oneshot(
                Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd backend && cargo test assistant_write_routes_are_protected`
Expected: compile error or `404 != 401` (routes not registered yet).

- [ ] **Step 3: Add `create` + `complete` to `backend/src/api/todos.rs`**

Update the file so it reads (the existing `list` fn stays):

```rust
use crate::error::AppError;
use crate::repo::todos::{self, TodoRow};
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde::Deserialize;

/// Open todos (status = open).
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<TodoRow>>, AppError> {
    let rows = todos::list_open(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct TodoIn {
    pub title: String,
}

/// Quick-add a todo (title only; other fields default).
pub async fn create(State(s): State<AppState>, Json(b): Json<TodoIn>) -> Result<Json<TodoRow>, AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    let row = todos::create(&s.db, b.title.trim(), None, None, None, None)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(row))
}

/// Mark an open todo done.
pub async fn complete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = todos::complete(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 4: Add `cancel` to `backend/src/api/reminders.rs`**

Add the imports `use axum::extract::Path;` (extend the existing `use axum::{extract::State, Json};` to `use axum::{extract::{Path, State}, Json};`) and append:

```rust
/// Cancel a pending reminder.
pub async fn cancel(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = reminders::cancel(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 5: Add `resolve` to `backend/src/api/inbox.rs`**

Extend the import to `use axum::{extract::{Path, State}, Json};` and append:

```rust
/// Mark a pending inbox item as handled (status = sorted).
pub async fn resolve(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let affected = inbox::resolve(&s.db, &[id], "sorted").await.map_err(AppError::Other)?;
    if affected == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 6: Register routes in `backend/src/api/mod.rs`**

The protected router currently has:
```rust
        .route("/todos", get(todos::list))
        .route("/reminders", get(reminders::list))
        .route("/inbox", get(inbox::list))
```
Change them to add the write routes (`post` is already imported):
```rust
        .route("/todos", get(todos::list).post(todos::create))
        .route("/todos/:id/complete", post(todos::complete))
        .route("/reminders", get(reminders::list))
        .route("/reminders/:id/cancel", post(reminders::cancel))
        .route("/inbox", get(inbox::list))
        .route("/inbox/:id/resolve", post(inbox::resolve))
```

- [ ] **Step 7: Run the protection test, confirm PASS**

Run: `cd backend && cargo test assistant_write_routes_are_protected`
Expected: PASS.

- [ ] **Step 8: Compile-check**

Run: `cd backend && cargo check`
Expected: no errors (pre-existing `next_cursor` warning in upwork is unrelated).

- [ ] **Step 9: Commit**

```bash
git add backend/src/api/todos.rs backend/src/api/reminders.rs backend/src/api/inbox.rs backend/src/api/mod.rs
git commit -m "feat(api): add todo create/complete, reminder cancel, inbox resolve endpoints"
```

---

## Task 2: Frontend mutation hooks

**Files:**
- Modify: `frontend/src/api/hooks.ts` (add 4 hooks near `useTodos`/`useReminders`/`useInbox`)
- Create: `frontend/src/api/assistant-mutations.test.ts`

`useInvalidatingMutation(fn, keys)` and `api.post(path, schema, body)` already exist. `TodoSchema` is already imported in this file.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/api/assistant-mutations.test.ts`:

```ts
import { describe, it, expect } from "vitest";
import * as hooks from "./hooks";

describe("assistant mutation hooks", () => {
  it("exports the four write hooks", () => {
    expect(typeof hooks.useCreateTodo).toBe("function");
    expect(typeof hooks.useCompleteTodo).toBe("function");
    expect(typeof hooks.useCancelReminder).toBe("function");
    expect(typeof hooks.useResolveInbox).toBe("function");
  });
});
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/api/assistant-mutations.test.ts`
Expected: FAIL — hooks are undefined.

- [ ] **Step 3: Add the hooks to `frontend/src/api/hooks.ts`**

Append after the `useTodos`/`useReminders`/`useInbox` block:

```ts
export const useCreateTodo = () =>
  useInvalidatingMutation((b: { title: string }) => api.post("/todos", TodoSchema, b), ["todos"]);

export const useCompleteTodo = () =>
  useInvalidatingMutation((id: number) => api.post(`/todos/${id}/complete`, z.unknown(), {}), ["todos"]);

export const useCancelReminder = () =>
  useInvalidatingMutation((id: number) => api.post(`/reminders/${id}/cancel`, z.unknown(), {}), ["reminders"]);

export const useResolveInbox = () =>
  useInvalidatingMutation((id: number) => api.post(`/inbox/${id}/resolve`, z.unknown(), {}), ["inbox"]);
```

- [ ] **Step 4: Run the test + type-check**

Run: `cd frontend && npx vitest run src/api/assistant-mutations.test.ts && npx tsc --noEmit`
Expected: test PASS; tsc clean.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/hooks.ts frontend/src/api/assistant-mutations.test.ts
git commit -m "feat(web): add todo create/complete, reminder cancel, inbox resolve hooks"
```

---

## Task 3: Todo card — complete button + quick-add form

**Files:**
- Modify: `frontend/src/components/DashboardTodoCard.tsx`
- Modify: `frontend/src/components/DashboardTodoCard.test.tsx`

- [ ] **Step 1: Add failing interaction tests**

Append two tests inside the existing `describe("DashboardTodoCard", ...)` block in `DashboardTodoCard.test.tsx`. Also add a `beforeEach` that gives the mutation hooks a default mock (so existing render tests don't break). Update the top of the file to import what's needed and stub the mutation hooks:

```tsx
import { fireEvent } from "@testing-library/react";
```
Add inside the describe block:
```tsx
  const completeMutate = vi.fn();
  const createMutate = vi.fn();
  beforeEach(() => {
    completeMutate.mockReset();
    createMutate.mockReset();
    vi.mocked(hooks.useCompleteTodo).mockReturnValue({ mutate: completeMutate, isPending: false } as any);
    vi.mocked(hooks.useCreateTodo).mockReturnValue({ mutate: createMutate, isPending: false } as any);
  });

  it("completes a todo when its check button is clicked", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({
      data: [{ id: 7, title: "Bayar internet", notes: null, due_at: null, status: "open", created_at: "2026-06-15T08:00:00+07:00", completed_at: null, priority: null, estimate_minutes: null }],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    fireEvent.click(screen.getByLabelText("Selesaikan Bayar internet"));
    expect(completeMutate).toHaveBeenCalledWith(7, expect.anything());
  });

  it("creates a todo from the quick-add form", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    fireEvent.change(screen.getByLabelText("Tambah todo"), { target: { value: "Beli kopi" } });
    fireEvent.click(screen.getByLabelText("Simpan todo baru"));
    expect(createMutate).toHaveBeenCalledWith({ title: "Beli kopi" }, expect.anything());
  });
```
Note: the existing two render tests set `useTodos` themselves; the `beforeEach` only stubs the mutation hooks, so they keep working.

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/components/DashboardTodoCard.test.tsx`
Expected: FAIL — `useCompleteTodo`/`useCreateTodo` not exported as mockable / labels not found.

- [ ] **Step 3: Rewrite `DashboardTodoCard.tsx` with the controls**

```tsx
import { useState } from "react";
import { Link } from "react-router-dom";
import { Check, Plus } from "lucide-react";
import { toast } from "sonner";
import { useTodos, useCompleteTodo, useCreateTodo } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardTodoCard() {
  const todos = useTodos();
  const completeTodo = useCompleteTodo();
  const createTodo = useCreateTodo();
  const [title, setTitle] = useState("");

  const rows = (todos.data ?? []).slice(0, MAX_ROWS);

  function handleComplete(id: number) {
    completeTodo.mutate(id, {
      onSuccess: () => toast.success("Todo selesai"),
      onError: (err) => toast.error((err as Error).message),
    });
  }

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed) return;
    createTodo.mutate(
      { title: trimmed },
      {
        onSuccess: () => { setTitle(""); toast.success("Todo ditambahkan"); },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  }

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
            <button
              type="button"
              aria-label={`Selesaikan ${t.title}`}
              className="pt-icon-btn shrink-0"
              disabled={completeTodo.isPending}
              onClick={() => handleComplete(t.id)}
            >
              <Check size={15} />
            </button>
            <span className="text-muted-foreground w-24 shrink-0">
              {t.due_at
                ? (wibDayKey(t.due_at) === todayWibKey() ? "Hari ini" : wibDayKey(t.due_at).slice(5)) + " · " + formatWibTime(t.due_at)
                : "—"}
            </span>
            <span className="flex-1 truncate">{t.title}</span>
          </div>
        ))}
        <form onSubmit={handleAdd} className="flex items-center gap-2" style={{ paddingTop: 8 }}>
          <input
            type="text"
            className="input flex-1"
            placeholder="Tambah todo…"
            aria-label="Tambah todo"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <button type="submit" aria-label="Simpan todo baru" className="pt-icon-btn shrink-0" disabled={createTodo.isPending}>
            <Plus size={15} />
          </button>
        </form>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run the tests, confirm PASS**

Run: `cd frontend && npx vitest run src/components/DashboardTodoCard.test.tsx`
Expected: all 4 tests PASS (2 original render + 2 new interaction).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/DashboardTodoCard.tsx frontend/src/components/DashboardTodoCard.test.tsx
git commit -m "feat(web): complete + quick-add todo from dashboard card"
```

---

## Task 4: Reminder card — cancel button

**Files:**
- Modify: `frontend/src/components/DashboardReminderCard.tsx`
- Modify: `frontend/src/components/DashboardReminderCard.test.tsx`

- [ ] **Step 1: Add a failing interaction test**

In `DashboardReminderCard.test.tsx`, add `import { fireEvent } from "@testing-library/react";`, then inside the describe block add a `beforeEach` stubbing the mutation hook plus a test:

```tsx
  const cancelMutate = vi.fn();
  beforeEach(() => {
    cancelMutate.mockReset();
    vi.mocked(hooks.useCancelReminder).mockReturnValue({ mutate: cancelMutate, isPending: false } as any);
  });

  it("cancels a reminder when its cancel button is clicked", async () => {
    vi.mocked(hooks.useReminders).mockReturnValue({
      data: [{ id: 9, todo_id: null, message: "Meeting jam 3", remind_at: "2026-06-15T15:00:00+07:00", recurrence: "none", status: "pending", sent_at: null, event_id: null }],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    fireEvent.click(screen.getByLabelText("Batalkan Meeting jam 3"));
    expect(cancelMutate).toHaveBeenCalledWith(9, expect.anything());
  });
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/components/DashboardReminderCard.test.tsx`
Expected: FAIL — `useCancelReminder` not mockable / label not found.

- [ ] **Step 3: Rewrite `DashboardReminderCard.tsx` with the cancel control**

```tsx
import { Link } from "react-router-dom";
import { X } from "lucide-react";
import { toast } from "sonner";
import { useReminders, useCancelReminder } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardReminderCard() {
  const reminders = useReminders();
  const cancelReminder = useCancelReminder();
  const rows = (reminders.data ?? [])
    .slice()
    .sort((a, b) => a.remind_at.localeCompare(b.remind_at))
    .slice(0, MAX_ROWS);

  function handleCancel(id: number) {
    cancelReminder.mutate(id, {
      onSuccess: () => toast.success("Reminder dibatalkan"),
      onError: (err) => toast.error((err as Error).message),
    });
  }

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
            <button
              type="button"
              aria-label={`Batalkan ${r.message}`}
              className="pt-icon-btn shrink-0"
              disabled={cancelReminder.isPending}
              onClick={() => handleCancel(r.id)}
            >
              <X size={15} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run the tests, confirm PASS**

Run: `cd frontend && npx vitest run src/components/DashboardReminderCard.test.tsx`
Expected: all tests PASS (2 original + 1 new).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/DashboardReminderCard.tsx frontend/src/components/DashboardReminderCard.test.tsx
git commit -m "feat(web): cancel reminder from dashboard card"
```

---

## Task 5: Inbox card — resolve button

**Files:**
- Modify: `frontend/src/components/DashboardInboxCard.tsx`
- Modify: `frontend/src/components/DashboardInboxCard.test.tsx`

- [ ] **Step 1: Add a failing interaction test**

In `DashboardInboxCard.test.tsx`, add `import { fireEvent } from "@testing-library/react";`, then inside the describe block add a `beforeEach` stubbing the mutation hook plus a test:

```tsx
  const resolveMutate = vi.fn();
  beforeEach(() => {
    resolveMutate.mockReset();
    vi.mocked(hooks.useResolveInbox).mockReturnValue({ mutate: resolveMutate, isPending: false } as any);
  });

  it("resolves an inbox item when its done button is clicked", async () => {
    vi.mocked(hooks.useInbox).mockReturnValue({
      data: [{ id: 4, content: "Ide produk baru", status: "pending", created_at: "2026-06-15T08:00:00+07:00", resolved_at: null }],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    fireEvent.click(screen.getByLabelText("Selesaikan Ide produk baru"));
    expect(resolveMutate).toHaveBeenCalledWith(4, expect.anything());
  });
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/components/DashboardInboxCard.test.tsx`
Expected: FAIL — `useResolveInbox` not mockable / label not found.

- [ ] **Step 3: Rewrite `DashboardInboxCard.tsx` with the resolve control**

```tsx
import { Link } from "react-router-dom";
import { Check } from "lucide-react";
import { toast } from "sonner";
import { useInbox, useResolveInbox } from "../api/hooks";

const MAX_ROWS = 5;

export function DashboardInboxCard() {
  const inbox = useInbox();
  const resolveInbox = useResolveInbox();
  const rows = (inbox.data ?? []).slice(0, MAX_ROWS);

  function handleResolve(id: number) {
    resolveInbox.mutate(id, {
      onSuccess: () => toast.success("Inbox ditangani"),
      onError: (err) => toast.error((err as Error).message),
    });
  }

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
            <button
              type="button"
              aria-label={`Selesaikan ${i.content}`}
              className="pt-icon-btn shrink-0"
              disabled={resolveInbox.isPending}
              onClick={() => handleResolve(i.id)}
            >
              <Check size={15} />
            </button>
          </div>
        ))}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run the tests, confirm PASS**

Run: `cd frontend && npx vitest run src/components/DashboardInboxCard.test.tsx`
Expected: all tests PASS (2 original + 1 new).

- [ ] **Step 5: Full suite + build + commit**

```bash
cd frontend && npx tsc --noEmit && npx vitest run && npm run build
```
Expected: tsc clean, all tests pass, build succeeds. Then:
```bash
git add frontend/src/components/DashboardInboxCard.tsx frontend/src/components/DashboardInboxCard.test.tsx
git commit -m "feat(web): resolve inbox item from dashboard card"
```

---

## Final verification (end-to-end)

- [ ] **Backend:** `cd backend && cargo check && cargo test assistant_write_routes_are_protected` — passes.
- [ ] **Frontend:** `cd frontend && npx tsc --noEmit && npx vitest run && npm run build` — type-clean, all tests pass, build succeeds.
- [ ] **Manual:** start the stack, log in, open the dashboard:
  - Click a todo's check → it disappears from the card + success toast.
  - Type in the quick-add field, submit → new todo appears.
  - Click a reminder's ✕ → it disappears.
  - Click an inbox item's ✓ → it disappears.
  - Confirm finance pages still work unchanged.

---

## Self-review notes

- **Spec coverage:** complete todo (Task 1 `complete` + Task 3 button), quick-add todo (Task 1 `create` + Task 3 form), cancel reminder (Task 1 `cancel` + Task 4 button), resolve inbox (Task 1 `resolve` + Task 5 button); mutation hooks (Task 2). All four spec actions + hooks covered.
- **Out-of-scope respected:** no edit/undo, no reminder/inbox create from UI, no dedicated pages, no migrations.
- **Type consistency:** hook names `useCreateTodo`/`useCompleteTodo`/`useCancelReminder`/`useResolveInbox` are defined in Task 2 and consumed in Tasks 3-5 identically; mutation `.mutate(id, opts)` / `.mutate({title}, opts)` shapes match the handlers' expected bodies (`TodoIn { title }`, path `:id`); resolve status `"sorted"` matches `inbox::resolve` contract.
