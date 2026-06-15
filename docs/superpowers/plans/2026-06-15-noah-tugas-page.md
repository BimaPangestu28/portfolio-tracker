# Tugas Page Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A tabbed `/tugas` page (Todo · Reminder · Inbox) with full list-by-status, todo edit/reopen, inbox unresolve, and reminder create — backed by new endpoints, reusing the existing `events`/dashboard-card patterns.

**Architecture:** Backend gains repo `list_by_status`/`update`/`reopen`/`unresolve` fns and matching endpoints (list endpoints take an optional `?status=` query, defaulting to today's behaviour so the dashboard cards are untouched). Frontend list hooks take an optional status arg (status in the query key); new mutation hooks reuse `useInvalidatingMutation`. A `TugasPage` hosts three focused tab components.

**Tech Stack:** Rust (axum + sqlx), React 18 + React Query + Zod, sonner toasts, vitest + Testing Library.

**Reference spec:** `docs/superpowers/specs/2026-06-15-noah-tugas-page-design.md`

**Branch:** `feat/noah-tugas-page` (already created off `main`; spec already committed).

**Notes for the engineer:**
- Backend is a **bin-only crate**. Do NOT run `cargo test --lib`. Use `cargo test <name>` and `cargo check`. Do NOT run `cargo fmt`.
- ⚠️ **Disk is near-full (~809 MB).** Before starting, confirm free space (`df -h /`); if a backend build fails with `ENOSPC`, stop and free space (e.g. `cargo clean`) — do not fight it.
- Repo column names: `todos(title, notes, due_at, status, created_at, completed_at, priority, estimate_minutes)`, `reminders(todo_id, message, remind_at, recurrence, status, sent_at, event_id)`, `inbox(content, status, created_at, resolved_at)`.
- Existing helpers: `api.get/post/patch(path, schema, body?)`, `useInvalidatingMutation(fn, keys)`. Handler pattern: see `backend/src/api/events.rs` (`list` with `Query`, `update` full-replace then `get`, `cancel` returns `NotFound` on false). Tab CSS: `.ptabs`/`.ptab`/`.ptab.active` (see `PortfolioPage.tsx`).
- Todo edit is **full-replace** (modal pre-fills current values and sends them all), mirroring `events::update`.

---

## Task 1: Backend — todos list-by-status, update, reopen

**Files:**
- Modify: `backend/src/repo/todos.rs`
- Modify: `backend/src/api/todos.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Add a failing protection test**

In the `router_tests` module of `backend/src/api/mod.rs`, add:
```rust
    #[serial]
    #[tokio::test]
    async fn todo_edit_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-todo-edit");
        let app = router(test_state().await);
        let cases = [("/todos/1", "PATCH"), ("/todos/1/reopen", "POST")];
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

Run: `cd backend && cargo test todo_edit_routes_are_protected`
Expected: `404 != 401` (routes not registered).

- [ ] **Step 3: Add repo fns to `backend/src/repo/todos.rs`**

Append:
```rust
/// List todos by status: "open", "done", or "all".
pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<TodoRow>> {
    let rows = match status {
        "all" => {
            sqlx::query_as::<_, TodoRow>(
                "SELECT * FROM todos ORDER BY (status = 'done'), due_at IS NULL, due_at, id",
            )
            .fetch_all(db)
            .await?
        }
        other => {
            sqlx::query_as::<_, TodoRow>(
                "SELECT * FROM todos WHERE status = ? ORDER BY due_at IS NULL, due_at, id",
            )
            .bind(other)
            .fetch_all(db)
            .await?
        }
    };
    Ok(rows)
}

/// Full-replace editable fields of a todo. Returns false if the id is absent.
pub async fn update(
    db: &Db,
    id: i64,
    title: &str,
    notes: Option<&str>,
    due_at: Option<&str>,
    priority: Option<&str>,
    estimate_minutes: Option<i64>,
) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE todos SET title = ?, notes = ?, due_at = ?, priority = ?, estimate_minutes = ? WHERE id = ?",
    )
    .bind(title)
    .bind(notes)
    .bind(due_at)
    .bind(priority)
    .bind(estimate_minutes)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Reopen a done todo (done -> open, clear completed_at). False if not currently done.
pub async fn reopen(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE todos SET status = 'open', completed_at = NULL WHERE id = ? AND status = 'done'",
    )
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 4: Update `backend/src/api/todos.rs`**

Replace the file so `list` reads a status query and `update`/`reopen` are added (keep `create`/`complete`/`TodoIn`):
```rust
use crate::error::AppError;
use crate::repo::todos::{self, TodoRow};
use crate::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// Todos filtered by status (?status=open|done|all); defaults to open.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<TodoRow>>, AppError> {
    let status = q.status.as_deref().unwrap_or("open");
    let rows = todos::list_by_status(&s.db, status).await.map_err(AppError::Other)?;
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

#[derive(Deserialize)]
pub struct TodoUpdateIn {
    pub title: String,
    pub notes: Option<String>,
    pub due_at: Option<String>,
    pub priority: Option<String>,
    pub estimate_minutes: Option<i64>,
}

/// Edit a todo (full replace of editable fields).
pub async fn update(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<TodoUpdateIn>,
) -> Result<Json<TodoRow>, AppError> {
    if b.title.trim().is_empty() {
        return Err(AppError::BadRequest("judul tidak boleh kosong".into()));
    }
    let ok = todos::update(
        &s.db, id, b.title.trim(), b.notes.as_deref(), b.due_at.as_deref(),
        b.priority.as_deref(), b.estimate_minutes,
    )
    .await
    .map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    let row = todos::get(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(row))
}

/// Reopen a done todo.
pub async fn reopen(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = todos::reopen(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 5: Register routes in `backend/src/api/mod.rs`**

Change the todo routes to:
```rust
        .route("/todos", get(todos::list).post(todos::create))
        .route("/todos/:id", axum::routing::patch(todos::update))
        .route("/todos/:id/complete", post(todos::complete))
        .route("/todos/:id/reopen", post(todos::reopen))
```

- [ ] **Step 6: Run the test + compile**

Run: `cd backend && cargo test todo_edit_routes_are_protected && cargo check`
Expected: test PASS; check clean (pre-existing upwork warning ok).

- [ ] **Step 7: Commit**

```bash
git add backend/src/repo/todos.rs backend/src/api/todos.rs backend/src/api/mod.rs
git commit -m "feat(api): todos list-by-status, edit (PATCH), and reopen"
```

---

## Task 2: Backend — reminders list-by-status + create, inbox list-by-status + unresolve

**Files:**
- Modify: `backend/src/repo/reminders.rs`, `backend/src/repo/inbox.rs`
- Modify: `backend/src/api/reminders.rs`, `backend/src/api/inbox.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Add a failing protection test**

In `router_tests` of `backend/src/api/mod.rs`:
```rust
    #[serial]
    #[tokio::test]
    async fn reminder_create_inbox_unresolve_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-rem-create");
        let app = router(test_state().await);
        let cases = [("/reminders", "POST"), ("/inbox/1/unresolve", "POST")];
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

Run: `cd backend && cargo test reminder_create_inbox_unresolve_protected`
Expected: `404 != 401`.

- [ ] **Step 3: Add repo fns**

`backend/src/repo/reminders.rs` append:
```rust
/// List reminders by status: "pending", "sent", "cancelled", or "all".
pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<ReminderRow>> {
    let rows = match status {
        "all" => {
            sqlx::query_as::<_, ReminderRow>("SELECT * FROM reminders ORDER BY remind_at")
                .fetch_all(db)
                .await?
        }
        other => {
            sqlx::query_as::<_, ReminderRow>(
                "SELECT * FROM reminders WHERE status = ? ORDER BY remind_at",
            )
            .bind(other)
            .fetch_all(db)
            .await?
        }
    };
    Ok(rows)
}
```

`backend/src/repo/inbox.rs` append:
```rust
/// List inbox items by status: "pending", "sorted", or "all".
pub async fn list_by_status(db: &Db, status: &str) -> anyhow::Result<Vec<InboxRow>> {
    let rows = match status {
        "all" => {
            sqlx::query_as::<_, InboxRow>("SELECT * FROM inbox ORDER BY id DESC")
                .fetch_all(db)
                .await?
        }
        other => {
            sqlx::query_as::<_, InboxRow>("SELECT * FROM inbox WHERE status = ? ORDER BY id DESC")
                .bind(other)
                .fetch_all(db)
                .await?
        }
    };
    Ok(rows)
}

/// Move a sorted inbox item back to pending. False if not currently sorted.
pub async fn unresolve(db: &Db, id: i64) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "UPDATE inbox SET status = 'pending', resolved_at = NULL WHERE id = ? AND status = 'sorted'",
    )
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}
```

- [ ] **Step 4: Update `backend/src/api/reminders.rs`**

Make `list` read a status query and add `create` (keep `cancel`). Set imports to `use axum::{extract::{Path, Query, State}, Json};` and `use serde::Deserialize;`:
```rust
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// Reminders by status (?status=pending|sent|cancelled|all); defaults to pending.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ReminderRow>>, AppError> {
    let status = q.status.as_deref().unwrap_or("pending");
    let rows = reminders::list_by_status(&s.db, status).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct ReminderIn {
    pub message: String,
    pub remind_at: String,
    pub recurrence: Option<String>,
}

/// Create a standalone reminder.
pub async fn create(State(s): State<AppState>, Json(b): Json<ReminderIn>) -> Result<Json<ReminderRow>, AppError> {
    if b.message.trim().is_empty() {
        return Err(AppError::BadRequest("pesan tidak boleh kosong".into()));
    }
    chrono::DateTime::parse_from_rfc3339(&b.remind_at)
        .map_err(|_| AppError::BadRequest("remind_at bukan RFC3339 valid".into()))?;
    let recurrence = b.recurrence.as_deref().unwrap_or("none");
    let row = reminders::create(&s.db, None, b.message.trim(), &b.remind_at, recurrence)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(row))
}
```
(The existing `cancel` handler stays as-is. If `list` previously had no `Query` import, the new import line covers it.)

- [ ] **Step 5: Update `backend/src/api/inbox.rs`**

Make `list` read a status query and add `unresolve` (keep `resolve`). Imports: `use axum::{extract::{Path, Query, State}, Json};` and `use serde::Deserialize;`:
```rust
#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub status: Option<String>,
}

/// Inbox by status (?status=pending|sorted|all); defaults to pending.
pub async fn list(
    State(s): State<AppState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<InboxRow>>, AppError> {
    let status = q.status.as_deref().unwrap_or("pending");
    let rows = inbox::list_by_status(&s.db, status).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

/// Move a sorted inbox item back to pending.
pub async fn unresolve(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<serde_json::Value>, AppError> {
    let ok = inbox::unresolve(&s.db, id).await.map_err(AppError::Other)?;
    if !ok {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "ok": true })))
}
```

- [ ] **Step 6: Register routes in `backend/src/api/mod.rs`**

Update reminder/inbox routes to:
```rust
        .route("/reminders", get(reminders::list).post(reminders::create))
        .route("/reminders/:id/cancel", post(reminders::cancel))
        .route("/inbox", get(inbox::list))
        .route("/inbox/:id/resolve", post(inbox::resolve))
        .route("/inbox/:id/unresolve", post(inbox::unresolve))
```

- [ ] **Step 7: Run the test + compile**

Run: `cd backend && cargo test reminder_create_inbox_unresolve_protected && cargo check`
Expected: test PASS; check clean.

- [ ] **Step 8: Commit**

```bash
git add backend/src/repo/reminders.rs backend/src/repo/inbox.rs backend/src/api/reminders.rs backend/src/api/inbox.rs backend/src/api/mod.rs
git commit -m "feat(api): reminder create, inbox unresolve, list-by-status"
```

---

## Task 3: Frontend — list hooks accept status + new mutation hooks

**Files:**
- Modify: `frontend/src/api/hooks.ts`
- Create: `frontend/src/api/tugas-hooks.test.ts`

Current hooks (for reference):
```ts
export const useTodos = () =>
  useQuery({ queryKey: ["todos"], queryFn: () => api.get("/todos", z.array(TodoSchema)) });
```

- [ ] **Step 1: Write the failing test**

Create `frontend/src/api/tugas-hooks.test.ts`:
```ts
import { describe, it, expect } from "vitest";
import * as hooks from "./hooks";

describe("tugas hooks", () => {
  it("exports the new mutation hooks", () => {
    expect(typeof hooks.useUpdateTodo).toBe("function");
    expect(typeof hooks.useReopenTodo).toBe("function");
    expect(typeof hooks.useUnresolveInbox).toBe("function");
    expect(typeof hooks.useCreateReminder).toBe("function");
  });
});
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/api/tugas-hooks.test.ts`
Expected: FAIL — hooks undefined.

- [ ] **Step 3: Update the list hooks to accept status (in `frontend/src/api/hooks.ts`)**

Replace the three list hooks with:
```ts
export const useTodos = (status?: string) =>
  useQuery({ queryKey: ["todos", status ?? "open"], queryFn: () => api.get(`/todos${status ? `?status=${status}` : ""}`, z.array(TodoSchema)) });

export const useReminders = (status?: string) =>
  useQuery({ queryKey: ["reminders", status ?? "pending"], queryFn: () => api.get(`/reminders${status ? `?status=${status}` : ""}`, z.array(ReminderSchema)) });

export const useInbox = (status?: string) =>
  useQuery({ queryKey: ["inbox", status ?? "pending"], queryFn: () => api.get(`/inbox${status ? `?status=${status}` : ""}`, z.array(InboxItemSchema)) });
```
Note: existing callers (dashboard cards) call these with no argument → unchanged behaviour, query key `["todos","open"]` etc. The mutation hooks invalidate by the top key `["todos"]`, which React Query treats as a prefix, so all status variants refetch.

- [ ] **Step 4: Add the new mutation hooks (after the existing `useResolveInbox`)**

```ts
export const useUpdateTodo = () =>
  useInvalidatingMutation(
    (args: { id: number; body: { title: string; notes?: string | null; due_at?: string | null; priority?: string | null; estimate_minutes?: number | null } }) =>
      api.patch(`/todos/${args.id}`, TodoSchema, args.body),
    ["todos"],
  );

export const useReopenTodo = () =>
  useInvalidatingMutation((id: number) => api.post(`/todos/${id}/reopen`, z.unknown(), {}), ["todos"]);

export const useUnresolveInbox = () =>
  useInvalidatingMutation((id: number) => api.post(`/inbox/${id}/unresolve`, z.unknown(), {}), ["inbox"]);

export const useCreateReminder = () =>
  useInvalidatingMutation(
    (b: { message: string; remind_at: string; recurrence?: string }) => api.post("/reminders", ReminderSchema, b),
    ["reminders"],
  );
```

- [ ] **Step 5: Run test + type-check**

Run: `cd frontend && npx vitest run src/api/tugas-hooks.test.ts && npx tsc --noEmit`
Expected: test PASS; tsc clean (the dashboard card tests still pass because the no-arg calls are unchanged).

- [ ] **Step 6: Commit**

```bash
git add frontend/src/api/hooks.ts frontend/src/api/tugas-hooks.test.ts
git commit -m "feat(web): status-aware list hooks + todo edit/reopen, inbox unresolve, reminder create hooks"
```

---

## Task 4: Frontend — TugasPage shell + Todo tab

**Files:**
- Create: `frontend/src/pages/TugasPage.tsx`
- Create: `frontend/src/components/tugas/TodoTab.tsx`
- Create: `frontend/src/components/tugas/TodoTab.test.tsx`

- [ ] **Step 1: Write the failing TodoTab test**

Create `frontend/src/components/tugas/TodoTab.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { TodoTab } from "./TodoTab";
import * as hooks from "../../api/hooks";

vi.mock("../../api/hooks");

describe("TodoTab", () => {
  const completeMutate = vi.fn();
  const reopenMutate = vi.fn();
  const createMutate = vi.fn();
  const updateMutate = vi.fn();
  beforeEach(() => {
    [completeMutate, reopenMutate, createMutate, updateMutate].forEach((m) => m.mockReset());
    vi.mocked(hooks.useCompleteTodo).mockReturnValue({ mutate: completeMutate, isPending: false } as any);
    vi.mocked(hooks.useReopenTodo).mockReturnValue({ mutate: reopenMutate, isPending: false } as any);
    vi.mocked(hooks.useCreateTodo).mockReturnValue({ mutate: createMutate, isPending: false } as any);
    vi.mocked(hooks.useUpdateTodo).mockReturnValue({ mutate: updateMutate, isPending: false } as any);
    vi.mocked(hooks.useTodos).mockReturnValue({
      data: [{ id: 7, title: "Bayar internet", notes: null, due_at: null, status: "open", created_at: "2026-06-15T08:00:00+07:00", completed_at: null, priority: null, estimate_minutes: null }],
      isLoading: false, isError: false,
    } as any);
  });

  it("changes the status filter (calls useTodos with the chosen status)", () => {
    render(<TodoTab />);
    fireEvent.click(screen.getByRole("button", { name: "Selesai" }));
    expect(hooks.useTodos).toHaveBeenCalledWith("done");
  });

  it("completes a todo", () => {
    render(<TodoTab />);
    fireEvent.click(screen.getByLabelText("Selesaikan Bayar internet"));
    expect(completeMutate).toHaveBeenCalledWith(7, expect.anything());
  });
});
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/components/tugas/TodoTab.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Create `frontend/src/components/tugas/TodoTab.tsx`**

```tsx
import { useState } from "react";
import { Check, RotateCcw, Plus, Pencil } from "lucide-react";
import { toast } from "sonner";
import { useTodos, useCompleteTodo, useReopenTodo, useCreateTodo, useUpdateTodo } from "../../api/hooks";
import type { Todo } from "../../api/schemas";

const STATUSES: { key: string; label: string }[] = [
  { key: "open", label: "Terbuka" },
  { key: "done", label: "Selesai" },
  { key: "all", label: "Semua" },
];

export function TodoTab() {
  const [status, setStatus] = useState("open");
  const todos = useTodos(status);
  const complete = useCompleteTodo();
  const reopen = useReopenTodo();
  const create = useCreateTodo();
  const update = useUpdateTodo();
  const [title, setTitle] = useState("");
  const [editing, setEditing] = useState<Todo | null>(null);

  const rows = todos.data ?? [];

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed) return;
    create.mutate({ title: trimmed }, {
      onSuccess: () => { setTitle(""); toast.success("Todo ditambahkan"); },
      onError: (err) => toast.error((err as Error).message),
    });
  }

  function handleSaveEdit(e: React.FormEvent) {
    e.preventDefault();
    if (!editing) return;
    update.mutate(
      { id: editing.id, body: { title: editing.title, notes: editing.notes ?? null, due_at: editing.due_at ?? null, priority: editing.priority ?? null, estimate_minutes: editing.estimate_minutes ?? null } },
      {
        onSuccess: () => { setEditing(null); toast.success("Todo disimpan"); },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  }

  return (
    <div className="flex col gap-3">
      <div className="ptabs" role="group" aria-label="Filter status">
        {STATUSES.map((st) => (
          <button key={st.key} className={`ptab${status === st.key ? " active" : ""}`} onClick={() => setStatus(st.key)}>
            {st.label}
          </button>
        ))}
      </div>

      <form onSubmit={handleAdd} className="flex items-center gap-2">
        <input className="input flex-1" placeholder="Tambah todo…" aria-label="Tambah todo" value={title} onChange={(e) => setTitle(e.target.value)} />
        <button type="submit" aria-label="Simpan todo baru" className="pt-icon-btn shrink-0" disabled={create.isPending}><Plus size={15} /></button>
      </form>

      {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada todo.</p>}
      {rows.map((t) => (
        <div key={t.id} className="flex items-center gap-2 text-sm">
          {t.status === "done" ? (
            <button type="button" aria-label={`Buka lagi ${t.title}`} className="pt-icon-btn shrink-0" disabled={reopen.isPending}
              onClick={() => reopen.mutate(t.id, { onSuccess: () => toast.success("Todo dibuka lagi"), onError: (e) => toast.error((e as Error).message) })}>
              <RotateCcw size={15} />
            </button>
          ) : (
            <button type="button" aria-label={`Selesaikan ${t.title}`} className="pt-icon-btn shrink-0" disabled={complete.isPending}
              onClick={() => complete.mutate(t.id, { onSuccess: () => toast.success("Todo selesai"), onError: (e) => toast.error((e as Error).message) })}>
              <Check size={15} />
            </button>
          )}
          <span className={`flex-1 truncate${t.status === "done" ? " line-through text-muted-foreground" : ""}`}>{t.title}</span>
          {t.priority && <span className="text-xs text-muted-foreground">{t.priority}</span>}
          <button type="button" aria-label={`Edit ${t.title}`} className="pt-icon-btn shrink-0" onClick={() => setEditing(t)}><Pencil size={14} /></button>
        </div>
      ))}

      {editing && (
        <form onSubmit={handleSaveEdit} className="card card-pad flex col gap-2" style={{ padding: 14 }}>
          <div className="card-title">Edit todo</div>
          <input className="input" aria-label="Judul" value={editing.title} onChange={(e) => setEditing({ ...editing, title: e.target.value })} />
          <input className="input" aria-label="Catatan" placeholder="Catatan" value={editing.notes ?? ""} onChange={(e) => setEditing({ ...editing, notes: e.target.value })} />
          <input className="input" aria-label="Prioritas" placeholder="Prioritas (low/med/high)" value={editing.priority ?? ""} onChange={(e) => setEditing({ ...editing, priority: e.target.value })} />
          <div className="flex items-center gap-2">
            <button type="submit" className="btn btn-primary" disabled={update.isPending}>Simpan</button>
            <button type="button" className="btn" onClick={() => setEditing(null)}>Batal</button>
          </div>
        </form>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Create `frontend/src/pages/TugasPage.tsx`**

```tsx
import { useState } from "react";
import { TodoTab } from "../components/tugas/TodoTab";

type Tab = "todo" | "reminder" | "inbox";

export default function TugasPage() {
  const [tab, setTab] = useState<Tab>("todo");

  return (
    <div className="flex col gap-5">
      <div>
        <h1 className="t-h1">Tugas</h1>
        <div className="t-sm t-muted">Kelola todo, reminder, dan inbox</div>
      </div>
      <div className="ptabs">
        <button className={`ptab${tab === "todo" ? " active" : ""}`} onClick={() => setTab("todo")}>Todo</button>
        <button className={`ptab${tab === "reminder" ? " active" : ""}`} onClick={() => setTab("reminder")}>Reminder</button>
        <button className={`ptab${tab === "inbox" ? " active" : ""}`} onClick={() => setTab("inbox")}>Inbox</button>
      </div>
      <div className="card card-pad" style={{ padding: 18 }}>
        {tab === "todo" && <TodoTab />}
        {tab === "reminder" && <p className="text-sm text-muted-foreground">Segera.</p>}
        {tab === "inbox" && <p className="text-sm text-muted-foreground">Segera.</p>}
      </div>
    </div>
  );
}
```
(The reminder/inbox placeholders are replaced in Task 5.)

- [ ] **Step 5: Run the tab tests + type-check**

Run: `cd frontend && npx vitest run src/components/tugas/TodoTab.test.tsx && npx tsc --noEmit`
Expected: both TodoTab tests pass; tsc clean.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/TugasPage.tsx frontend/src/components/tugas/TodoTab.tsx frontend/src/components/tugas/TodoTab.test.tsx
git commit -m "feat(web): Tugas page shell + Todo tab"
```

---

## Task 5: Frontend — Reminder tab + Inbox tab

**Files:**
- Create: `frontend/src/components/tugas/ReminderTab.tsx`, `ReminderTab.test.tsx`
- Create: `frontend/src/components/tugas/InboxTab.tsx`, `InboxTab.test.tsx`
- Modify: `frontend/src/pages/TugasPage.tsx` (swap placeholders for the real tabs)

- [ ] **Step 1: Write the failing ReminderTab test**

Create `frontend/src/components/tugas/ReminderTab.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ReminderTab } from "./ReminderTab";
import * as hooks from "../../api/hooks";

vi.mock("../../api/hooks");

describe("ReminderTab", () => {
  const cancelMutate = vi.fn();
  const createMutate = vi.fn();
  beforeEach(() => {
    cancelMutate.mockReset();
    createMutate.mockReset();
    vi.mocked(hooks.useCancelReminder).mockReturnValue({ mutate: cancelMutate, isPending: false } as any);
    vi.mocked(hooks.useCreateReminder).mockReturnValue({ mutate: createMutate, isPending: false } as any);
    vi.mocked(hooks.useReminders).mockReturnValue({
      data: [{ id: 9, todo_id: null, message: "Meeting jam 3", remind_at: "2026-06-15T15:00:00+07:00", recurrence: "none", status: "pending", sent_at: null, event_id: null }],
      isLoading: false, isError: false,
    } as any);
  });

  it("cancels a pending reminder", () => {
    render(<ReminderTab />);
    fireEvent.click(screen.getByLabelText("Batalkan Meeting jam 3"));
    expect(cancelMutate).toHaveBeenCalledWith(9, expect.anything());
  });

  it("creates a reminder from the form", () => {
    render(<ReminderTab />);
    fireEvent.change(screen.getByLabelText("Pesan reminder"), { target: { value: "Telepon klien" } });
    fireEvent.change(screen.getByLabelText("Waktu reminder"), { target: { value: "2026-06-20T09:00" } });
    fireEvent.click(screen.getByRole("button", { name: "Buat reminder" }));
    expect(createMutate).toHaveBeenCalled();
    expect(createMutate.mock.calls[0][0].message).toBe("Telepon klien");
  });
});
```

- [ ] **Step 2: Run it, confirm FAIL**

Run: `cd frontend && npx vitest run src/components/tugas/ReminderTab.test.tsx`
Expected: FAIL — module not found.

- [ ] **Step 3: Create `frontend/src/components/tugas/ReminderTab.tsx`**

```tsx
import { useState } from "react";
import { X } from "lucide-react";
import { toast } from "sonner";
import { useReminders, useCancelReminder, useCreateReminder } from "../../api/hooks";
import { wibDayKey, formatWibTime } from "../../lib/wib";

const STATUSES: { key: string; label: string }[] = [
  { key: "pending", label: "Aktif" },
  { key: "sent", label: "Terkirim" },
  { key: "cancelled", label: "Dibatalkan" },
  { key: "all", label: "Semua" },
];

export function ReminderTab() {
  const [status, setStatus] = useState("pending");
  const reminders = useReminders(status);
  const cancel = useCancelReminder();
  const create = useCreateReminder();
  const [message, setMessage] = useState("");
  const [when, setWhen] = useState("");

  const rows = (reminders.data ?? []).slice().sort((a, b) => a.remind_at.localeCompare(b.remind_at));

  function handleCreate(e: React.FormEvent) {
    e.preventDefault();
    if (!message.trim() || !when) return;
    // datetime-local has no timezone; treat as WIB (+07:00).
    const remind_at = `${when}:00+07:00`;
    create.mutate({ message: message.trim(), remind_at }, {
      onSuccess: () => { setMessage(""); setWhen(""); toast.success("Reminder dibuat"); },
      onError: (err) => toast.error((err as Error).message),
    });
  }

  return (
    <div className="flex col gap-3">
      <div className="ptabs" role="group" aria-label="Filter status">
        {STATUSES.map((st) => (
          <button key={st.key} className={`ptab${status === st.key ? " active" : ""}`} onClick={() => setStatus(st.key)}>{st.label}</button>
        ))}
      </div>

      <form onSubmit={handleCreate} className="flex items-center gap-2 flex-wrap">
        <input className="input flex-1" placeholder="Pesan reminder…" aria-label="Pesan reminder" value={message} onChange={(e) => setMessage(e.target.value)} />
        <input className="input" type="datetime-local" aria-label="Waktu reminder" value={when} onChange={(e) => setWhen(e.target.value)} />
        <button type="submit" className="btn btn-primary" disabled={create.isPending}>Buat reminder</button>
      </form>

      {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada reminder.</p>}
      {rows.map((r) => (
        <div key={r.id} className="flex items-center gap-2 text-sm">
          <span className="text-muted-foreground w-28 shrink-0">{wibDayKey(r.remind_at).slice(5)} · {formatWibTime(r.remind_at)}</span>
          <span className="flex-1 truncate">{r.message}</span>
          {r.status !== "pending" && <span className="text-xs text-muted-foreground">{r.status}</span>}
          {r.status === "pending" && (
            <button type="button" aria-label={`Batalkan ${r.message}`} className="pt-icon-btn shrink-0" disabled={cancel.isPending}
              onClick={() => cancel.mutate(r.id, { onSuccess: () => toast.success("Reminder dibatalkan"), onError: (e) => toast.error((e as Error).message) })}>
              <X size={15} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Run ReminderTab test, confirm PASS**

Run: `cd frontend && npx vitest run src/components/tugas/ReminderTab.test.tsx`
Expected: both tests pass.

- [ ] **Step 5: Write the failing InboxTab test**

Create `frontend/src/components/tugas/InboxTab.test.tsx`:
```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { InboxTab } from "./InboxTab";
import * as hooks from "../../api/hooks";

vi.mock("../../api/hooks");

describe("InboxTab", () => {
  const resolveMutate = vi.fn();
  const unresolveMutate = vi.fn();
  beforeEach(() => {
    resolveMutate.mockReset();
    unresolveMutate.mockReset();
    vi.mocked(hooks.useResolveInbox).mockReturnValue({ mutate: resolveMutate, isPending: false } as any);
    vi.mocked(hooks.useUnresolveInbox).mockReturnValue({ mutate: unresolveMutate, isPending: false } as any);
    vi.mocked(hooks.useInbox).mockReturnValue({
      data: [{ id: 4, content: "Ide produk baru", status: "pending", created_at: "2026-06-15T08:00:00+07:00", resolved_at: null }],
      isLoading: false, isError: false,
    } as any);
  });

  it("resolves a pending item", () => {
    render(<InboxTab />);
    fireEvent.click(screen.getByLabelText("Selesaikan Ide produk baru"));
    expect(resolveMutate).toHaveBeenCalledWith(4, expect.anything());
  });

  it("switches to the sorted filter", () => {
    render(<InboxTab />);
    fireEvent.click(screen.getByRole("button", { name: "Selesai" }));
    expect(hooks.useInbox).toHaveBeenCalledWith("sorted");
  });
});
```

- [ ] **Step 6: Create `frontend/src/components/tugas/InboxTab.tsx`**

```tsx
import { useState } from "react";
import { Check, RotateCcw } from "lucide-react";
import { toast } from "sonner";
import { useInbox, useResolveInbox, useUnresolveInbox } from "../../api/hooks";

const STATUSES: { key: string; label: string }[] = [
  { key: "pending", label: "Pending" },
  { key: "sorted", label: "Selesai" },
  { key: "all", label: "Semua" },
];

export function InboxTab() {
  const [status, setStatus] = useState("pending");
  const inbox = useInbox(status);
  const resolve = useResolveInbox();
  const unresolve = useUnresolveInbox();

  const rows = inbox.data ?? [];

  return (
    <div className="flex col gap-3">
      <div className="ptabs" role="group" aria-label="Filter status">
        {STATUSES.map((st) => (
          <button key={st.key} className={`ptab${status === st.key ? " active" : ""}`} onClick={() => setStatus(st.key)}>{st.label}</button>
        ))}
      </div>

      {rows.length === 0 && <p className="text-sm text-muted-foreground">Inbox kosong.</p>}
      {rows.map((i) => (
        <div key={i.id} className="flex items-center gap-2 text-sm">
          <span className="flex-1 truncate">{i.content}</span>
          {i.status === "pending" ? (
            <button type="button" aria-label={`Selesaikan ${i.content}`} className="pt-icon-btn shrink-0" disabled={resolve.isPending}
              onClick={() => resolve.mutate(i.id, { onSuccess: () => toast.success("Inbox ditangani"), onError: (e) => toast.error((e as Error).message) })}>
              <Check size={15} />
            </button>
          ) : (
            <button type="button" aria-label={`Buka lagi ${i.content}`} className="pt-icon-btn shrink-0" disabled={unresolve.isPending}
              onClick={() => unresolve.mutate(i.id, { onSuccess: () => toast.success("Dikembalikan ke pending"), onError: (e) => toast.error((e as Error).message) })}>
              <RotateCcw size={15} />
            </button>
          )}
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 7: Wire the real tabs into `frontend/src/pages/TugasPage.tsx`**

Add imports and replace the two placeholders:
```tsx
import { ReminderTab } from "../components/tugas/ReminderTab";
import { InboxTab } from "../components/tugas/InboxTab";
```
```tsx
        {tab === "todo" && <TodoTab />}
        {tab === "reminder" && <ReminderTab />}
        {tab === "inbox" && <InboxTab />}
```

- [ ] **Step 8: Run both tab tests + type-check**

Run: `cd frontend && npx vitest run src/components/tugas/ReminderTab.test.tsx src/components/tugas/InboxTab.test.tsx && npx tsc --noEmit`
Expected: all pass; tsc clean.

- [ ] **Step 9: Commit**

```bash
git add frontend/src/components/tugas/ReminderTab.tsx frontend/src/components/tugas/ReminderTab.test.tsx frontend/src/components/tugas/InboxTab.tsx frontend/src/components/tugas/InboxTab.test.tsx frontend/src/pages/TugasPage.tsx
git commit -m "feat(web): Reminder and Inbox tabs on Tugas page"
```

---

## Task 6: Route + nav + dashboard links + final verification

**Files:**
- Modify: `frontend/src/App.tsx` (add `/tugas` route)
- Modify: `frontend/src/components/AppShell.tsx` (add "Tugas" nav item)
- Modify: `frontend/src/components/DashboardTodoCard.tsx`, `DashboardReminderCard.tsx`, `DashboardInboxCard.tsx` (link "Lihat semua")

- [ ] **Step 1: Add the route in `frontend/src/App.tsx`**

Add the lazy/import alongside the other page imports (match the existing import style; if pages are imported directly, add `import TugasPage from "./pages/TugasPage";`), then add inside the `AppShell` route group, after the `agenda` route:
```tsx
        <Route path="tugas" element={<TugasPage />} />
```

- [ ] **Step 2: Add the nav item in `frontend/src/components/AppShell.tsx`**

Add `ListChecks` to the lucide-react import, then in `NAV_GROUPS` "Asisten" group, after the Agenda entry:
```tsx
      { to: "/tugas", label: "Tugas", icon: ListChecks },
```

- [ ] **Step 3: Point the dashboard cards at the Tugas tabs**

In each of `DashboardTodoCard.tsx`, `DashboardReminderCard.tsx`, `DashboardInboxCard.tsx`, change the header `Link` target from `/chat` ("Tanya Noah →") to `/tugas` with text "Lihat semua →". Example for the Todo card:
```tsx
        <Link to="/tugas" className="text-sm text-primary hover:underline">Lihat semua →</Link>
```
(Apply the same change in all three cards.)

- [ ] **Step 4: Update affected card tests**

The three dashboard card tests do not assert on the link text, so they should still pass. Run them to confirm:
Run: `cd frontend && npx vitest run src/components/DashboardTodoCard.test.tsx src/components/DashboardReminderCard.test.tsx src/components/DashboardInboxCard.test.tsx`
Expected: all pass. If any asserts on "Tanya Noah", update that assertion to "Lihat semua" and report it.

- [ ] **Step 5: Full verification**

Run: `cd frontend && npx tsc --noEmit && npx vitest run && npm run build`
Expected: tsc clean, all tests pass, build succeeds.
Run: `cd backend && cargo test todo_edit_routes_are_protected reminder_create_inbox_unresolve_protected && cargo check`
Expected: tests pass, check clean.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/App.tsx frontend/src/components/AppShell.tsx frontend/src/components/DashboardTodoCard.tsx frontend/src/components/DashboardReminderCard.tsx frontend/src/components/DashboardInboxCard.tsx
git commit -m "feat(web): route + nav for Tugas page, dashboard cards link to it"
```

---

## Final verification (end-to-end)

- [ ] **Backend:** `cd backend && cargo check && cargo test todo_edit_routes_are_protected reminder_create_inbox_unresolve_protected assistant_write_routes_are_protected` — all pass.
- [ ] **Frontend:** `cd frontend && npx tsc --noEmit && npx vitest run && npm run build` — clean, all tests pass, build succeeds.
- [ ] **Manual:** open `/tugas`; Todo tab: filter open/done/all, complete, reopen, quick-add, edit; Reminder tab: filter, cancel, create via form; Inbox tab: filter, resolve, unresolve. Confirm dashboard cards still populate (default status unchanged) and their "Lihat semua →" links land on `/tugas`.

---

## Self-review notes

- **Spec coverage:** tabbed page (T4 shell + T5 tabs); list-by-status (T1/T2 backend `?status=` + T3 status-aware hooks + tab filter pills); edit todo (T1 PATCH + T3 `useUpdateTodo` + T4 modal); reopen todo + unresolve inbox (T1/T2 endpoints + T3 hooks + T4/T5 buttons); create reminder (T2 endpoint + T3 hook + T5 form); nav + dashboard links (T6). All spec items covered.
- **Out-of-scope respected:** no delete, no bulk, no pagination, no recurring editor, no invoice.
- **Type consistency:** hook names (`useUpdateTodo`/`useReopenTodo`/`useUnresolveInbox`/`useCreateReminder`) defined in T3, consumed identically in T4/T5; `useUpdateTodo` takes `{ id, body }` matching `PATCH /todos/:id` `TodoUpdateIn`; list-hook status arg defaults preserve dashboard behaviour; status keys (`open/done/all`, `pending/sent/cancelled/all`, `pending/sorted/all`) match backend `list_by_status` arms.
- **Note:** backend behaviour (not just route protection) is verified manually per the established test pattern in this crate (router tests are protection-only); `cargo check` guards compilation.
