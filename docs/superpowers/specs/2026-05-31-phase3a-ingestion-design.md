# Investment Tracker — Phase 3A (LLM Ingestion Core) Design Spec

**Tanggal:** 2026-05-31
**Status:** Disetujui untuk Fase 3A
**Depends on:** Fase 1 (ledger, repos, REST API, dashboard) — sudah merged ke `main`.

---

## 1. Problem

Input transaksi manual satu-satu nggak realistis. User pegang portfolio tersebar dan ingin
meng-input dari **screenshot** (holdings/riwayat/nota) dan **statement bank (PDF)**. Fase 3A
membangun jalur ingestion: upload → ekstraksi LLM → **review queue** (konfirmasi/edit/reject)
→ commit ke ledger Fase 1. Budgeting/CSV = Fase 3B (terpisah).

## 2. Prinsip

- **LLM nggak pernah auto-commit.** Semua hasil ekstraksi mendarat di staging `review_item`
  berstatus `pending`; hanya yang dikonfirmasi user yang masuk ledger.
- **Ledger Fase 1 tetap utuh.** Tidak ada perubahan pada logika cost-basis/valuation/summary;
  confirm hanya memakai repo `transactions/instruments/accounts` yang sudah ada & teruji.
- **Tidak ada entri yang hilang diam-diam.** Entri low-confidence tetap di-stage, ditandai
  `needs_attention` — bukan di-drop.
- **Audit:** file sumber asli + respons LLM mentah disimpan untuk re-review/debug.

## 3. Scope Fase 3A

### In scope
- Endpoint upload (`POST /ingest`) menerima 1+ file (image/PDF) sebagai base64.
- Klien Claude Messages API dari backend Rust (vision + PDF document blocks).
- Ekstraksi terstruktur: model meng-klasifikasi `doc_type` lalu mengeluarkan entri kandidat.
- Staging table `review_item` + repo + service confirm/reject.
- Saran match instrument (by symbol) & account (by name); **inline create** instrument/account
  saat review.
- REST: list/edit/confirm/reject review items.
- Frontend **Review page**: upload, daftar pending editable, inline create, confirm/reject.
- Empat `doc_type`: `holdings_snapshot`, `txn_history`, `bank_statement`, `trade_confirmation`.
- Pemrosesan **sinkron**.

### Out of scope (fase lain)
Budgeting / kategori pengeluaran & CSV importer (3B) · chatbot/WhatsApp ingestion (Fase 4) ·
auto-sync exchange/on-chain (Fase 2) · enkripsi secret (API key via env) · pemrosesan async/job
queue · multi-user.

## 4. Arsitektur & alur

```
Dashboard Review page (React)
   │  POST /ingest  (files base64 + optional account hint)
   ▼
axum  ──►  ingestion service (Rust)
            ├─ llm/claude.rs ──► Anthropic Messages API (vision + PDF)
            ├─ ingestion/extract.rs  (build prompt, parse response → ExtractedEntry[])
            └─ ingestion/review.rs    (stage review_item, confirm/reject → ledger via Fase 1 repos)
                     │
                     ▼  SQLite: review_item   +   disk: data/uploads/<batch_id>/
   ▲  GET/PATCH/POST /ingest/review...
   │
Dashboard Review page  ◄── pending items (edit, map/create instrument+account, confirm/reject)
```

Alur:
1. User upload file di Review page → `POST /ingest`.
2. Backend simpan file ke `data/uploads/<batch_id>/`, base64-encode, panggil Claude dengan
   prompt ekstraksi + skema JSON yang diharapkan.
3. Model mengembalikan `{ doc_type, entries[] }`. Backend memvalidasi/parse jadi
   `ExtractedEntry[]`, menghitung saran match instrument/account, menyimpan tiap entri sebagai
   `review_item` (`pending`).
4. Review page menampilkan pending (grouped per `batch_id`). User edit field, pilih/buat
   instrument & account, lalu confirm/reject.
5. Confirm → transform payload → insert ledger via repo Fase 1; set `status=confirmed`,
   `created_txn_id`. Reject → `status=rejected`.

## 5. Mapping doc_type → ledger

| doc_type | Entri ledger yang dihasilkan |
|----------|------------------------------|
| `holdings_snapshot` | satu `opening_balance` per instrumen (quantity + avg cost sebagai price_native) |
| `txn_history` | transaksi individual: `buy`/`sell`/`dividend`/`fee` sesuai baris |
| `trade_confirmation` | satu transaksi (`buy`/`sell`) |
| `bank_statement` | `deposit`/`withdrawal`/`dividend`/`interest` di account terpilih; income/dividen di-tie ke instrumen bila terbaca, selain itu ke instrumen cash |

## 6. Komponen (boundary)

### `src/llm/claude.rs`
Wrapper tipis Anthropic Messages API. Fungsi: kirim daftar content blocks (text + image/document
base64) + system prompt, kembalikan teks/JSON respons. Konfigurasi: `ANTHROPIC_API_KEY` (env),
model default `claude-sonnet-4-6` (override via env `INGEST_MODEL`). Error → `LlmError`
(no panic). Timeout wajar (mis. 60s).

### `src/ingestion/extract.rs`
- `build_extraction_request(files, hint) -> ClaudeRequest` — susun prompt + content blocks.
- `parse_extraction(json) -> Result<Extraction, ExtractError>` — **pure**, validasi bentuk
  `{ doc_type, entries[] }` → `Extraction { doc_type, entries: Vec<ExtractedEntry> }`.
  Unit-tested dengan sample JSON per `doc_type`, tanpa network.

`ExtractedEntry` (kandidat, sebelum mapping):
```
entry_type (buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance),
symbol?, instrument_name?, quantity?, price_native?, fee_native?, currency?,
executed_at?, account_hint?, note?, confidence(0..1)
```

### `src/ingestion/review.rs` + repo
Staging CRUD + confirm/reject. `confirm(item, edited_payload)`:
- map/lookup instrument (by id atau create inline dari payload), account (idem),
- isi `fx_to_idr`/`fx_to_usd` dari `latest_fx` bila kosong,
- panggil `transactions::create(...)` (validasi decimal Fase 1 berlaku),
- set `status=confirmed`, `created_txn_id`.
`reject(item)` → `status=rejected`.

### Matching
`suggest_instrument(db, symbol) -> Option<i64>` (case-insensitive exact match dulu),
`suggest_account(db, name_or_hint) -> Option<i64>`.

### API (axum)
- `POST /ingest` — body `{ files: [{ filename, media_type, data_base64 }], account_hint? }`
  → `{ batch_id, items: [ReviewItem] }`.
- `GET /ingest/review?status=pending` → `[ReviewItem]`.
- `PATCH /ingest/review/:id` — body payload edit → `ReviewItem`.
- `POST /ingest/review/:id/confirm` → `{ created_txn_id }`.
- `POST /ingest/review/:id/reject` → 200.

### Frontend Review page
Upload widget (drag/drop file → base64) + daftar pending per batch dengan baris editable
(entry_type, instrument selector + "create new", account selector + "create new", qty, price,
fee, currency, date), badge `doc_type` & `needs_attention`, tombol Confirm/Reject per item dan
Confirm-all per batch.

## 7. Data model — tabel baru `review_item`

```
id INTEGER PK
batch_id TEXT            -- group satu upload
source_kind TEXT         -- 'image' | 'pdf'
source_filename TEXT
source_path TEXT         -- data/uploads/<batch_id>/<filename>
doc_type TEXT
status TEXT              -- 'pending' | 'confirmed' | 'rejected'
needs_attention INTEGER  -- 0/1 (low confidence / field kosong)
payload_json TEXT        -- ExtractedEntry editable (string angka, konsisten dgn ledger TEXT)
raw_llm_json TEXT        -- audit
suggested_instrument_id INTEGER NULL
suggested_account_id INTEGER NULL
created_txn_id INTEGER NULL
created_at TEXT
confirmed_at TEXT NULL
```

Migration baru `0002_review_item.sql`. Tidak mengubah tabel Fase 1.

## 8. Error handling

- **LLM call gagal** (network/HTTP non-2xx) → `502`, tidak ada `review_item` dibuat, file
  sumber tetap tersimpan + dicatat.
- **JSON respons rusak/incomplete** → retry sekali dengan instruksi lebih ketat; bila tetap
  gagal → simpan `raw_llm_json` ke log + kembalikan error; tidak ada pending nyangkut separuh.
- **Confirm** → validasi via repo Fase 1 (decimal, txn_type). Field kosong wajib → `400`.
- **Low confidence / field tak lengkap** → entri tetap di-stage, `needs_attention=1`.
- No `unwrap()`/`panic!()` di jalur produksi; secret tak pernah di-log.

## 9. Testing

- **Unit:** `parse_extraction` per doc_type (sample JSON → entries), transform confirm
  (`holdings_snapshot`→`opening_balance`; `bank_statement` line→deposit/withdrawal),
  `suggest_instrument`/`suggest_account`.
- **Integrasi:** `review_item` repo CRUD; confirm flow → baris ledger ter-insert + status
  confirmed + `created_txn_id`; reject → status rejected; inline-create instrument/account.
- **LLM client:** mock HTTP (uji bentuk request + parsing respons). Satu live test di-`#[ignore]`
  tanpa `ANTHROPIC_API_KEY`.
- **Frontend:** Review page render pending (MSW), interaksi confirm/reject, inline create.

## 10. Keputusan & default

| Topik | Keputusan |
|-------|-----------|
| Engine | Claude Messages API dipanggil dari Rust (reqwest), vision + PDF document blocks |
| Model | `claude-sonnet-4-6` default, override `INGEST_MODEL` |
| doc_type didukung | holdings_snapshot, txn_history, bank_statement, trade_confirmation |
| Unknown instrument/account | inline create + auto-suggest by symbol/name |
| Pemrosesan | sinkron |
| Staging | tabel `review_item` terpisah (Approach A) |
| Penyimpanan file sumber | disk `data/uploads/<batch_id>/`, path di row |
| Secret | `ANTHROPIC_API_KEY` via env |
| FX entri | default dari `latest_fx` saat confirm, editable |
| Review channel | dashboard (WhatsApp = Fase 4) |

## 11. Pemecahan plan

- **Plan 3A-backend:** migration `review_item`, `llm/claude.rs`, `ingestion/extract.rs`
  (+parser tests), review repo + service (confirm/reject + matching), API endpoints.
- **Plan 3A-frontend:** Review page (upload, list editable, inline create, confirm/reject),
  zod schema `ReviewItem`, hooks, nav entry.

## 12. Risiko & catatan

- **Akurasi ekstraksi LLM** bervariasi (kualitas screenshot, layout statement). Mitigasi:
  review queue wajib + `needs_attention` + audit raw JSON; user selalu bisa edit sebelum commit.
- **Biaya API** per ekstraksi (vision token). Mitigasi: model Sonnet default, sinkron 1 panggilan
  per upload, tidak ada retry kecuali JSON rusak.
- **FX historis** tetap perkiraan (default kurs terkini saat confirm) — konsisten dgn catatan Fase 1.
- **PDF besar** bisa lambat/timeout (sinkron). Bila jadi masalah nyata → pertimbangkan async di
  iterasi lanjut (di luar 3A).
