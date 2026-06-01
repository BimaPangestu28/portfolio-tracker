# Investment Tracker — Phase 3B (Budgeting + CSV Importer) Design Spec

**Tanggal:** 2026-06-01
**Status:** Disetujui untuk dieksekusi (mandate "lanjut sampai selesai" — keputusan default didokumentasikan & ditandai ASUMSI)
**Depends on:** Fase 1 (ledger, repos), Fase 3A (review_item staging + Import page).

---

## 1. Problem & scope

Dua kebutuhan tersisa dari Fase 3:
- **CSV importer** — import bulk history dari export broker/bank tanpa LLM (deterministik, murah).
- **Budgeting / cashflow** — lacak uang masuk/keluar yang BUKAN transaksi instrumen (gaji, pengeluaran harian), kategorisasi, view bulanan, target budget.

### ASUMSI (default yang saya ambil — koreksi kalau salah)
- **A1.** CSV di-stage ke `review_item` yang SUDAH ADA (Fase 3A) dengan `doc_type="csv_import"`, jadi UI review/confirm/reject yang sama dipakai ulang. Tidak ada parser ajaib per-broker; user kasih **mapping kolom→field** sekali.
- **A2.** Budgeting dibuat **lean MVP**: tabel `cashflow` terpisah (income/expense, opsional ke instrumen tidak), `cashflow_category` dengan target bulanan opsional, view bulanan (income vs expense vs net + per-kategori + budget-vs-actual). Entry via form manual.
- **A3.** Jalur "confirm bank_statement → cashflow" (alih-alih ledger) **ditunda** sebagai follow-up — biar tidak mengusik confirm 3A yang sudah teruji. Untuk sekarang cashflow di-input manual / nanti via importer khusus.
- **A4.** Mata uang cashflow disimpan apa adanya (string + currency); konversi/agregasi bulanan diasumsikan satu mata uang dominan (IDR) untuk view — multi-currency cashflow = follow-up.

### Out of scope
Auto-sync (Fase 2), chatbot (Fase 4), bank-statement→cashflow confirm path, multi-currency cashflow aggregation, recurring-budget automation.

---

## 2. Komponen

### 2A. CSV importer
- **`POST /ingest/csv`** — body `{ filename, csv_text, mapping, doc_type_hint? }`. `mapping` memetakan nama kolom CSV ke field `ExtractedEntry` (`entry_type`, `symbol`, `quantity`, `price_native`, `fee_native`, `currency`, `executed_at`, `account_hint`). Opsi `entry_type_const` bila CSV tidak punya kolom tipe (mis. semua "buy").
- Backend `ingestion/csv.rs`: parse CSV (header + rows), untuk tiap row → `ExtractedEntry` via mapping → stage `review_item` (`doc_type="csv_import"`, `needs_attention` bila field inti kosong). **Pure parser** (`parse_csv_rows(csv_text, mapping) -> Vec<ExtractedEntry>`) unit-tested.
- Reuse confirm/reject 3A apa adanya (entri masuk ledger sama seperti screenshot).
- Frontend: di Import page tambah pilihan "CSV": textarea/file `.csv` + UI mapping kolom sederhana (deteksi header, dropdown per field). Hasil masuk review list yang sama.

### 2B. Budgeting / cashflow
- **`cashflow_category`** tabel: `{id, name, kind(income|expense), monthly_budget(nullable, TEXT decimal), color}`.
- **`cashflow`** tabel: `{id, account_id(nullable), occurred_on(date), direction(in|out), amount(TEXT decimal), currency, category_id(nullable), note, created_at}`.
- Repo + service: CRUD cashflow & kategori; **monthly summary** `month_summary(db, year_month) -> { total_in, total_out, net, per_category: [{category, kind, actual, budget, over_budget}] }` (pure aggregation fn unit-tested atas list cashflow+kategori).
- API: `GET/POST/DELETE /cashflow`, `GET/POST/DELETE /cashflow/categories`, `GET /cashflow/summary?month=YYYY-MM`.
- Frontend: **Budget page** — form entry cashflow (tanggal, in/out, amount, currency, kategori, note), daftar kategori dgn target, **view bulanan** (kartu income/expense/net + bar per-kategori actual-vs-budget dgn flag over-budget), pemilih bulan.

---

## 3. Data model (migrations baru)

`0003_cashflow.sql`:
```sql
CREATE TABLE cashflow_category (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  name TEXT NOT NULL,
  kind TEXT NOT NULL,                 -- 'income' | 'expense'
  monthly_budget TEXT,                -- nullable decimal string
  color TEXT
);
CREATE TABLE cashflow (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_id INTEGER REFERENCES account(id),
  occurred_on TEXT NOT NULL,          -- 'YYYY-MM-DD'
  direction TEXT NOT NULL,            -- 'in' | 'out'
  amount TEXT NOT NULL,
  currency TEXT NOT NULL,
  category_id INTEGER REFERENCES cashflow_category(id),
  note TEXT,
  created_at TEXT NOT NULL
);
CREATE INDEX idx_cashflow_month ON cashflow(occurred_on);
```
Tidak ada perubahan tabel Fase 1/3A. CSV importer tidak butuh tabel baru (pakai `review_item`).

---

## 4. Logika inti (pure, TDD)

- `parse_csv_rows(csv_text, mapping) -> Vec<ExtractedEntry>` — split header, map tiap row; baris kosong di-skip; angka di-trim separator ribuan opsional.
- `month_summary(cashflows, categories, year_month) -> MonthSummary` — filter by `occurred_on` prefix `YYYY-MM`, jumlahkan in/out, net = in-out, per-category actual vs `monthly_budget`, `over_budget = actual > budget`.

## 5. Error handling & testing
- CSV: baris yang gagal-map (field inti kosong) tetap di-stage `needs_attention` (tidak di-drop). Mapping tanpa kolom wajib → `400`.
- Cashflow: amount/decimal divalidasi sebelum insert (pola Fase 1). `direction`/`kind` divalidasi enum.
- No `unwrap`/`panic` produksi. Tests: parser CSV, month_summary, repo CRUD, API; frontend Budget/CSV via MSW.

## 6. Pemecahan plan
- **3B-backend:** migration cashflow, cashflow repo + categories repo, `month_summary` aggregation, `ingestion/csv.rs` parser + `POST /ingest/csv`, cashflow/summary API.
- **3B-frontend:** Budget page (cashflow entry + categories + monthly view), CSV import UI on Import page, schemas/hooks, nav.

## 7. Risiko
- Format CSV broker ID beragam → mapping manual mengatasi mayoritas; preset bisa ditambah belakangan.
- Budgeting multi-currency disederhanakan (ASUMSI A4) — bisa diperluas bila perlu.
