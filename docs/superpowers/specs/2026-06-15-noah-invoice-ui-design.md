# Desain: Invoice UI (read + download PDF)

**Tanggal:** 2026-06-15
**Status:** Disetujui untuk implementasi
**Branch:** `feat/noah-invoice-ui` (dari `main`)

## Konteks

Domain invoice sudah hampir lengkap tapi cuma terhubung ke chat: `repo/clients.rs`
(`create`/`get`/`get_by_name`/`list`) + `repo/invoices.rs` (`insert`/`list_all`/`max_seq_for_prefix`)
+ modul `invoice/` (`number.rs`, `assemble.rs`, `model.rs`, `render.rs` Typst→PDF, `terbilang.rs`,
`config.rs`). Dispatcher chat membuat invoice end-to-end lalu kirim PDF via Telegram. Belum ada
HTTP API maupun UI. Spec ini = **Grup B**, MVP **read + download PDF** (pembuatan tetap lewat chat).

## Keputusan desain (hasil brainstorming)

- Scope: read-only — list invoice, detail, download PDF, list klien. **Tidak** ada buat/edit/hapus dari UI.
- PDF di-render ulang dari row tersimpan (tidak menyimpan PDF): `line_items_json` menyimpan `{title, body, qty, amount}` (integer) yang persis input `ParsedItem` untuk `assemble_invoice_data`.
- Nav: item "Invoice" di grup **Keuangan**.
- Out of scope: buat/edit invoice dari UI, kelola klien, hapus, filter/search, pagination.

## Lingkup perubahan

### 1. Backend (tanpa migrasi)

**Repo (`backend/src/repo/invoices.rs`):** tambah
```rust
pub async fn get(db: &Db, id: i64) -> anyhow::Result<Option<InvoiceRow>>
```
(`InvoiceRow` & `ClientRow` SUDAH derive `Serialize` — tidak perlu diubah.)

**Reconstruct helper (modul `invoice/`, mis. fungsi baru `rebuild::data_from_row`):**
```rust
pub fn data_from_row(row: &InvoiceRow, client: &ClientRow, config: &InvoiceConfig) -> anyhow::Result<InvoiceData>
```
- Parse `row.line_items_json` → `Vec<ParsedItem>` (`amount` JSON → `amount_idr`).
- Parse `row.issue_date` (ISO `%Y-%m-%d`) → `NaiveDate`.
- **Fidelitas due_date:** set `config.due_days = (due_date - issue_date).num_days()` dari tanggal tersimpan sebelum assemble, supaya due pada PDF sama persis dengan yang disimpan (bukan menghitung ulang dari default).
- Panggil `assemble::assemble_invoice_data(row.number, issue_date, &config, client, &items)`.

**API `backend/src/api/invoices.rs` (modul baru; niru pola `events.rs`):**
- `GET /invoices` → `invoices::list_all` → `Json<Vec<InvoiceRow>>`.
- `GET /invoices/:id` → `invoices::get`; `None` → `AppError::NotFound`; else `Json<InvoiceRow>`.
- `GET /invoices/:id/pdf` → `get` row (404), `clients::get(client_id)`, `config::from_env()` (map err → `AppError`), `rebuild::data_from_row`, `render::render_pdf` → `axum::response::Response` dengan `content-type: application/pdf` dan `content-disposition: attachment; filename="<number>.pdf"` (slash pada number diganti `-` untuk nama file).
- `GET /clients` → `clients::list` → `Json<Vec<ClientRow>>`.
- Daftarkan keempat route di protected router `mod.rs`. Tambah test proteksi `invoice_routes_are_protected` (4 route → 401 tanpa auth).

### 2. Frontend

**Schemas (`frontend/src/api/schemas.ts`):**
- `ClientSchema` `{ id:number, name:string, sub_name:string|null, website:string|null, created_at:string }`.
- `InvoiceSchema` `{ id, number, client_id:number, issue_date, due_date, subtotal:string, total:string, line_items_json:string, created_at }`.
- `InvoiceLineItemSchema` `{ title:string, body:string|null|optional, qty:number, amount:number }` — untuk parse `line_items_json`.

**Client (`frontend/src/api/client.ts`):** tambah `getBlob(path)` → `fetch(BASE+path, { headers: authHeader() })`, lempar error mirip `request` pada non-OK/401, return `res.blob()`.

**Hooks (`frontend/src/api/hooks.ts`):** `useInvoices()` (`["invoices"]`), `useClients()` (`["clients"]`), `useInvoice(id)` (`["invoice", id]`).

**Halaman `pages/InvoicesPage.tsx` (`/invoices`):**
- List invoice (nomor, nama klien via map dari `useClients`, terbit, jatuh tempo, total). Klik baris → set `selectedId` (state), detail tampil di panel.
- Detail: `useInvoice(selectedId)` → header (nomor, klien, terbit/jatuh tempo), tabel line item (parse `line_items_json` dengan `InvoiceLineItemSchema`, format `amount` integer → "Rp …" via helper FE), subtotal/total (string dari row), tombol **Download PDF** → `api.getBlob(`/invoices/${id}/pdf`)` → `URL.createObjectURL` → klik `<a download="<number>.pdf">` → revoke.
- Format rupiah: helper kecil `formatIdr(n)` (Intl `id-ID`) di page atau `lib/`.

**Nav (`components/AppShell.tsx`):** tambah `{ to: "/invoices", label: "Invoice", icon: <FileText> }` di grup **Keuangan** (setelah Data atau Portofolio). Route `/invoices` di `App.tsx`.

### 3. Pengetesan / verifikasi
- **Backend:** test proteksi `invoice_routes_are_protected` (GET `/invoices`, `/invoices/1`, `/invoices/1/pdf`, `/clients` → 401); `cargo check`. (Bin-only: jangan `cargo test --lib`; jangan `cargo fmt`.)
- **Frontend:** test `InvoicesPage` — render list, klik baris menampilkan detail, klik "Download PDF" memanggil `api.getBlob` (mock). `tsc --noEmit`, `vitest run`, `npm run build`.
- **Manual:** `/invoices` list muncul, detail benar, PDF terunduh & valid.

## Di luar scope
Buat/edit/hapus invoice dari UI, kelola klien, filter/search, pagination, PPN/pajak.
