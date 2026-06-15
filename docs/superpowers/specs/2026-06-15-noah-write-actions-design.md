# Desain: Aksi tulis inline untuk Todo / Reminder / Inbox

**Tanggal:** 2026-06-15
**Status:** Disetujui untuk implementasi
**Branch:** `feat/noah-write-actions` (dicabang dari `feat/noah-pivot`, yang berisi read-endpoint + kartu dashboard)

## Konteks

Pivot Noah (PR #64) menambah read-endpoint `/todos` `/reminders` `/inbox` dan kartu
dashboard "Hari ini", tapi manajemen item masih hanya lewat chat agent. Pemilik ingin
aksi tulis dasar bisa dilakukan langsung dari UI tanpa membuka chat: menyelesaikan
todo, menambah todo cepat, membatalkan reminder, dan menandai inbox selesai.

Pendekatan: semua inline di kartu "Hari ini", meniru pola `events`
(`POST`/`cancel` + `useInvalidatingMutation`), me-reuse fungsi repo yang sudah ada.
Setelah mutasi sukses, query di-invalidate sehingga item yang selesai/dibatalkan
otomatis hilang dari kartu (list-nya `list_open`/`list_pending`). Tanpa migrasi/SQL baru.

## Keputusan desain (hasil brainstorming)

- Aksi yang diaktifkan: complete todo, tambah todo cepat, resolve inbox, cancel reminder (keempatnya).
- Penempatan: inline di kartu dashboard (tanpa halaman khusus, tanpa quick-add global).
- Quick-add todo: **title-only** (notes/due/priority tetap lewat chat).
- Feedback: toast (`sonner`, sudah dipakai di app).

## Lingkup perubahan

### 1. Backend — 4 endpoint baru
Daftarkan di protected router `backend/src/api/mod.rs`, meniru `events::create`/`events::cancel`.

- `backend/src/api/todos.rs`:
  - `create(State, Json<TodoIn>)` → validasi `title` tidak kosong (niru `events::validate`),
    panggil `todos::create(&db, &title, None, None, None, None)`, balikan `Json<TodoRow>`.
    `TodoIn { title: String }`.
  - `complete(State, Path<i64>)` → `todos::complete(&db, id)`; jika `false` → `AppError::NotFound`;
    balikan `Json(json!({ "ok": true }))` (niru `events::cancel`).
- `backend/src/api/reminders.rs`:
  - `cancel(State, Path<i64>)` → `reminders::cancel(&db, id)`; `false` → `NotFound`; `{ "ok": true }`.
- `backend/src/api/inbox.rs`:
  - `resolve(State, Path<i64>)` → `inbox::resolve(&db, &[id], "sorted")`; jika `0` baris → `NotFound`;
    `{ "ok": true }`. (Status `"sorted"` = ditangani; `inbox::resolve` hanya mengubah baris pending.)

Rute baru (protected):
- `.route("/todos", get(todos::list).post(todos::create))`
- `.route("/todos/:id/complete", post(todos::complete))`
- `.route("/reminders/:id/cancel", post(reminders::cancel))`
- `.route("/inbox/:id/resolve", post(inbox::resolve))`

### 2. Frontend — hooks (`frontend/src/api/hooks.ts`)
Pakai `useInvalidatingMutation` yang sudah ada (lihat `useCreateEvent`/`useCancelEvent`):
- `useCreateTodo()` → `api.post("/todos", TodoSchema, { title })`, invalidate `["todos"]`.
- `useCompleteTodo()` → `api.post("/todos/${id}/complete", z.unknown(), {})`, invalidate `["todos"]`.
- `useCancelReminder()` → `api.post("/reminders/${id}/cancel", z.unknown(), {})`, invalidate `["reminders"]`.
- `useResolveInbox()` → `api.post("/inbox/${id}/resolve", z.unknown(), {})`, invalidate `["inbox"]`.

### 3. Frontend — kontrol di kartu
- `DashboardTodoCard`: tombol/checkbox ✓ di tiap baris → `completeTodo(id)`; footer kartu berisi
  input teks + submit (Enter) → `createTodo({ title })`, kosongkan input setelah sukses.
- `DashboardReminderCard`: tombol ✕ per baris → `cancelReminder(id)`.
- `DashboardInboxCard`: tombol ✓ per baris → `resolveInbox(id)`.
- Tombol disabled saat mutation `isPending`; `toast.success`/`toast.error` pada hasil.
- Ikon dari `lucide-react` (mis. `Check`, `X`, `Plus`) — sesuaikan dengan yang sudah dipakai.

## Pengetesan / verifikasi

- **Backend:** tambah test proteksi route (niru `assistant_read_routes_are_protected`) untuk
  `POST /todos`, `/todos/:id/complete`, `/reminders/:id/cancel`, `/inbox/:id/resolve` → semua `401`
  tanpa auth. `cargo check`. (Crate bin-only: jangan `cargo test --lib`; jangan `cargo fmt`.)
- **Frontend:** test interaksi tiap kartu — mock hook mutation, render kartu, klik tombol/submit input,
  assert `mutate` terpanggil dengan argumen benar (niru gaya `DashboardAgendaCard.test.tsx`).
  `npx tsc --noEmit`, `vitest run`, `npm run build`.
- **Manual:** di dashboard, complete todo → hilang dari kartu + toast; tambah todo → muncul; cancel
  reminder → hilang; resolve inbox → hilang.

## Di luar scope

- Edit todo (judul/due/priority), undo/restore, reminder atau inbox create dari UI, halaman khusus.
- Aksi-aksi itu tetap lewat chat agent.
