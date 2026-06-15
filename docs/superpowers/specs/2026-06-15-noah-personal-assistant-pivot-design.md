# Desain: Pivot "Portfolio Tracker" → Noah (personal assistant)

**Tanggal:** 2026-06-15
**Status:** Disetujui untuk implementasi

## Konteks

Repo ini dimulai sebagai portfolio tracker, tapi sudah tumbuh menjadi kira-kira
separuh personal assistant: chat agent Claude dengan 30+ tools, manajemen
todo/reminder/event, integrasi Telegram/WhatsApp/Google/Upwork/ClickUp, invoice,
dan pesan proaktif (briefing/recap). Pemilik ingin menggeser pusat gravitasi
produk: asisten — diberi nama **Noah** (nama anak pemilik) — menjadi muka depan,
sementara fitur keuangan turun menjadi salah satu grup fitur.

Pendekatan yang dipilih: **rebrand + reposisi Information Architecture**. Tidak
ada fitur portfolio yang dibuang; tidak ada rename folder/repo. Yang berubah:
identitas/branding, susunan navigasi, komposisi dashboard, plus penambahan tiga
read-endpoint tipis agar section asisten di dashboard menampilkan data live.

## Keputusan desain (hasil brainstorming)

- Kedalaman pivot: rebrand + reposisi IA (bukan restruktur arsitektur, bukan pangkas portfolio).
- Home (`/`): dashboard gabungan — kartu asisten di atas, kartu keuangan di bawah.
- Nama: **Noah**. Brand sidebar & title jadi "Noah".
- Nav: entri Noah di puncak, menu dikelompokkan dua grup (Asisten/Harian + Keuangan).
- Section asisten dashboard: lengkap — Agenda + Todo + Reminder + Inbox.
- Ikon brand default: `Sparkles` (alternatif `Bot`). Grup nav memakai label ("Asisten" / "Keuangan").

## Lingkup perubahan

### 1. Identitas & branding (frontend)
- `frontend/index.html`: `<title>` `Portfolio Tracker` → `Noah`; meta description →
  "Noah — asisten pribadi: tugas, agenda, & keuangan."; `apple-mobile-web-app-title`
  `Portfolio` → `Noah`. Warna tema `#2977f5` tetap.
- `frontend/vite.config.ts`: manifest `name` & `short_name` → `Noah`.
- `frontend/src/components/AppShell.tsx`: brand-name `Portfolio` → `Noah`; ikon
  brand-mark `PieChart` → `Sparkles`; fallback `usePageTitle()` → `Noah`; label
  footer "Kunci portofolio" → "Kunci".
- Aset favicon/apple-touch = **follow-up opsional** (perlu desain aset), tidak masuk scope ini.

### 2. Navigasi & reposisi IA (frontend, `AppShell.tsx`)
- Tambah/ubah entri nav: **Noah** → `/chat`, ikon `Sparkles`, posisi pertama.
  Entri "Chat" lama digantikan oleh Noah (route `/chat` tetap sama).
- `NavList` dikelompokkan dua grup dengan label kecil:
  - **Asisten:** Noah · Dashboard · Agenda · Rencana · Budget
  - **Keuangan:** Portofolio · Data
- Footer (Pengaturan, Kunci, Ciutkan) tetap di bawah.
- Bottom nav mobile dipimpin Noah: `BOTTOM_KEYS = ["/chat", "/", "/agenda", "/budget"]` + "Lainnya".

### 3. Dashboard gabungan (frontend, `DashboardPage.tsx`)
- Tambah section **"Hari ini"** di atas (setelah `PendingReviewBanner`, sebelum hero
  keuangan), berisi grid kartu:
  - **Todo hari ini** (kartu baru) — dari `useTodos`.
  - **Agenda** — `DashboardAgendaCard` yang sudah ada, dipindah ke section ini.
  - **Reminder mendatang** (kartu baru) — dari `useReminders`.
  - **Inbox** (kartu baru) — dari `useInbox`.
- Section **"Keuangan"**: hero net-worth + Alokasi/Drift/Rebalancing/Kesehatan/Komposisi
  dan kartu lainnya — konten tetap, dipindah ke bawah section "Hari ini".
- Kartu baru mengikuti pola visual `DashboardAgendaCard` (header + daftar baris ringkas,
  state loading & empty).

### 4. Read-endpoint backend (tipis, reuse repo yang ada)
Pola: niru `backend/src/api/events.rs::list` (ekstraksi state/auth + error handling).
Daftarkan di `backend/src/api/mod.rs` pada protected router.
- `backend/src/api/todos.rs` → `GET /todos`, panggil `repo::todos::list_open(db)`,
  serialisasi `TodoRow`.
- `backend/src/api/reminders.rs` → `GET /reminders`, panggil `repo::reminders::list_pending(db)`,
  serialisasi `ReminderRow`.
- `backend/src/api/inbox.rs` → `GET /inbox`, panggil `repo::inbox::list_pending(db)`,
  serialisasi `InboxRow`.
- Tidak ada SQL/migrasi baru — fungsi repo sudah tersedia.

### 5. Data layer frontend
- `frontend/src/api/schemas.ts`: tambah tipe/zod `Todo`, `Reminder`, `InboxItem`
  (mengikuti bentuk `TodoRow`/`ReminderRow`/`InboxRow`).
- `frontend/src/api/hooks.ts`: tambah `useTodos`, `useReminders`, `useInbox` (niru `useEvents`).

### 6. Identitas Noah di agent (backend, `agent.rs`)
- `backend/src/assistant/agent.rs`: konstanta `SYSTEM` diawali `"You are Noah, a
  personal assistant for the app owner…"` (sebelumnya `"You are a personal
  assistant…"`). Tujuan: Noah menyebut dirinya "Noah" di chat & pesan proaktif.
  Perubahan teks satu baris; perilaku lain tetap.

### 7. Suggested prompts chat (frontend, `ChatPage.tsx`)
- Ganti tiga prompt portfolio-only menjadi campuran tugas asisten, mis.:
  "Apa agenda saya hari ini?", "Ingetin meeting jam 3 sore", "Catat todo: bayar
  internet", "Berapa net worth saya?".

## Pengetesan / verifikasi

- **Backend:** `cargo check` lalu `cargo build` di `backend/` (crate bin-only —
  `cargo test --lib` memang error, jangan dipakai; jangan jalankan `cargo fmt`).
  Bila ada pola test untuk handler `events`, tambah test serupa untuk handler baru.
- **Frontend:** `npm run build` + `vitest`; tambah test untuk kartu & hook baru
  meniru `DashboardAgendaCard.test.tsx`.
- **Manual (end-to-end):** jalankan app, verifikasi:
  - title/brand/manifest menampilkan "Noah", ikon Sparkles.
  - nav terbagi dua grup, Noah di puncak, bottom nav dipimpin Noah.
  - section "Hari ini" menampilkan data live dari `/todos`, `/events`, `/reminders`, `/inbox`.
  - section keuangan tetap berfungsi normal di bawahnya.
  - Noah menyebut dirinya "Noah" saat chat.

## Di luar scope

- Tidak ada fitur portfolio yang dibuang atau diarsipkan.
- Tidak ada rename folder/repo (tetap `portfolio-tracker`).
- Redesign aset ikon (favicon, apple-touch, brand-mark kustom) = follow-up terpisah.
- Tidak menambah aksi tulis (create/update) untuk todo/reminder/inbox dari UI — hanya
  read; manajemen tetap via chat seperti sekarang.
