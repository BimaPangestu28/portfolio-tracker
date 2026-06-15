# Desain: Halaman "Tugas" (Todo + Reminder + Inbox)

**Tanggal:** 2026-06-15
**Status:** Disetujui untuk implementasi
**Branch:** `feat/noah-tugas-page` (dari `main`, sesudah PR #64 & #65 merge)

## Konteks

Pivot Noah (#64) + aksi tulis inline (#65) memberi kartu dashboard "Hari ini" untuk
todo/reminder/inbox, tapi belum ada halaman manajemen penuh. Saat ini repo backend hanya
punya query "list open/pending" — tidak ada list semua-status maupun edit. Spec ini
menambah halaman **Tugas** bertab (Todo | Reminder | Inbox) beserta endpoint backend
secukupnya. Ini Grup A dari rencana 2-siklus; Invoice end-to-end menyusul sebagai spec terpisah.

## Keputusan desain (hasil brainstorming)

- Struktur: **satu halaman `/tugas` bertab** (Todo | Reminder | Inbox), mengikuti pola tab internal `PortfolioPage`.
- Operasi: filter & lihat semua status; edit todo; reopen todo & undo resolve inbox; buat reminder dari UI. (Plus aksi yang sudah ada: complete todo, quick-add todo, cancel reminder, resolve inbox.)
- List by status: **satu endpoint per entitas dengan query param `status`**, default = perilaku lama (supaya kartu dashboard tak berubah).
- Di luar scope: hapus permanen, bulk actions, pagination, editor recurring-reminder lanjutan.

## Lingkup perubahan

### 1. Backend (niru pola `events`/`crud`; tanpa migrasi)

**Repo (`backend/src/repo/`):**
- `todos.rs`: `list_by_status(db, status)` (status `open`/`done`/`all`); `update(db, id, fields)` (title/notes/due_at/priority/estimate_minutes); `reopen(db, id) -> bool` (done→open, kosongkan completed_at).
- `inbox.rs`: `list_by_status(db, status)` (pending/sorted/all); `unresolve(db, id) -> bool` (sorted→pending, kosongkan resolved_at).
- `reminders.rs`: `list_by_status(db, status)` (pending/sent/cancelled/all). `create` sudah ada.

**API (`backend/src/api/`):**
- `GET /todos?status=` → `todos::list` baca query, default `open` (panggil `list_by_status`).
- `PATCH /todos/:id` → `todos::update_handler` (body parsial; field kosong = tak diubah).
- `POST /todos/:id/reopen` → `404` kalau bukan done, else `{ "ok": true }`.
- `GET /reminders?status=` default `pending`.
- `POST /reminders` `{ message, remind_at, recurrence? }` → validasi message non-kosong + remind_at non-kosong; `recurrence` default `"none"`; balikan `ReminderRow`.
- `GET /inbox?status=` default `pending`.
- `POST /inbox/:id/unresolve` → `404` kalau bukan sorted, else `{ "ok": true }`.
- Route baru didaftarkan di protected router `mod.rs`. Tiap route baru dapat assertion di test proteksi (perluas/ tambah `assistant_*_routes_are_protected`).

### 2. Frontend

**Schemas/hooks (`frontend/src/api/`):**
- Hooks list diberi argumen status opsional: `useTodos(status?)`, `useReminders(status?)`, `useInbox(status?)` — query key menyertakan status (mis. `["todos", status ?? "open"]`); tanpa argumen tetap default lama agar kartu dashboard tak berubah.
- Hooks mutasi baru (pakai `useInvalidatingMutation`): `useUpdateTodo` (PATCH), `useReopenTodo`, `useUnresolveInbox`, `useCreateReminder`.

**Halaman & komponen (`frontend/src/`):**
- `pages/TugasPage.tsx` — route `/tugas`; state tab aktif; render satu dari tiga komponen tab.
- `components/tugas/TodoTab.tsx`, `ReminderTab.tsx`, `InboxTab.tsx` — tiap file fokus: pills filter status, list penuh, aksi inline.
  - TodoTab: filter open/done/all; complete; reopen (untuk done); quick-add; edit (modal kecil pakai `input`/`btn`).
  - ReminderTab: filter pending/sent/cancelled/all; cancel (untuk pending); form create (message + `datetime-local` + recurrence select).
  - InboxTab: filter pending/sorted/all; resolve (pending); unresolve (sorted).
- Edit todo: modal sederhana (judul/notes/due/priority/estimate) → `useUpdateTodo`.

### 3. Navigasi & dashboard
- `AppShell.tsx`: tambah item **"Tugas"** (ikon mis. `ListChecks`) di grup **Asisten**, setelah "Agenda".
- Kartu "Hari ini": tambah/ubah link kartu jadi "Lihat semua →" menuju `/tugas` (tab terkait via query/state). Kartu sendiri tetap.

### 4. Pengetesan / verifikasi

- **Backend:** test proteksi tiap route baru (401 tanpa auth, niru `assistant_write_routes_are_protected`); `cargo check`. (Crate bin-only: jangan `cargo test --lib`; jangan `cargo fmt`.)
- **Frontend:** test render+interaksi tiap tab — ganti filter memicu query baru; edit submit memanggil mutate; reopen/unresolve/create reminder memanggil mutate dengan argumen benar (niru gaya `DashboardTodoCard.test.tsx`). `tsc --noEmit`, `vitest run`, `npm run build`.
- **Manual:** buka `/tugas`, tiap tab: filter status, edit todo, reopen/undo, buat reminder; pastikan kartu dashboard masih jalan (default status tak berubah).

## ⚠️ Prasyarat operasional

Disk **~809 MB bebas (96% penuh)**. Kompilasi backend Rust untuk endpoint baru hampir pasti
butuh ruang lebih — **bereskan ruang disk dulu** sebelum implementasi (mis. `cargo clean`
pada target lama / hapus artefak besar), kalau tidak `cargo`/`vitest`/`build` bisa gagal `ENOSPC`.

## Di luar scope

Hapus permanen todo/inbox, bulk actions, pagination, editor recurring-reminder lanjutan,
serta Invoice (siklus/spec terpisah).
