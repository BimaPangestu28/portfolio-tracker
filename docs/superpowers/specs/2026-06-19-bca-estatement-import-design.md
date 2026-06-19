# BCA e-Statement Import → Cashflow + Portfolio Txn

**Date:** 2026-06-19
**Status:** Design approved, pending implementation plan

## Problem

Users cannot import a BCA "Rekening Tahapan" PDF e-statement to record
expenses (pengeluaran) and income (pemasukan). Two gaps exist today:

1. **PDF rejected at ingest.** In `backend/src/ingestion/ingest.rs` (~line 123),
   any `application/pdf` upload is staged with a hardcoded note
   `"PDF belum didukung — unggah ulang sebagai gambar (PNG/JPG)."` and never
   reaches the vision model.
2. **Bank entries land in the wrong table.** Even via the image path, confirmed
   `deposit`/`withdrawal` review items create a **portfolio transaction**
   (`transactions::create`, `review.rs` ~line 151), never a `cashflow` row. The
   Budget feature (cashflow) is therefore never populated from bank statements.

## Goal

Upload a BCA Tahapan PDF → each mutation row becomes a **review item** → user
confirms → the system creates **both** a `cashflow` row (for Budget /
expense-income) **and** a portfolio `txn` (for cash position), linked and
deduplicated so a single statement line is never double-counted in one metric.

## Key decisions (locked during brainstorming)

| Decision | Choice |
|---|---|
| Target table | **Both** cashflow (Budget) AND portfolio txn (cash position) |
| Parsing approach | **Deterministic** BCA parser, not vision LLM |
| Text extraction | `pdftotext -layout` (poppler-utils) — option A |
| Categorization | **Rule-based** keyword → category mapping, auto-create categories |
| Review flow | **Through the review queue** (consistent with existing ingest flow) |

Text-extraction option B (pure-Rust `pdf-extract`, no system dependency) was
considered and rejected for now: `pdftotext -layout` preserves column alignment,
which makes the parser markedly simpler and more robust. The extraction step is
isolated in `bca_text.rs` so swapping to B later does not touch the parser.

## Architecture

New module under the existing ingestion layer; small units with one purpose each:

```
backend/src/ingestion/
  bank/
    mod.rs          -- entry: parse_bca_statement(text) -> Vec<BcaMutation>
    bca_text.rs     -- PDF text extraction via `pdftotext -layout`
    bca_parser.rs   -- column state-machine (TANGGAL/KETERANGAN/MUTASI/SALDO)
    bca_category.rs -- keyword -> category rules + is_transfer flag
  ingest.rs         -- PDF branch: detect BCA -> route to parser
  review.rs         -- confirm: deposit/withdrawal -> txn + cashflow::insert_sourced
```

- `bca_parser.rs` is pure: `String` text in, `Vec<BcaMutation>` out, zero I/O —
  fully unit-testable from a text fixture.
- `bca_text.rs` is the only unit that touches the filesystem / external binary.

### `BcaMutation` (parser output)

```
tanggal:     NaiveDate     // resolved from DD/MM + statement period year
jenis:       String        // raw KETERANGAN header, e.g. "TRSF E-BANKING DB"
deskripsi:   String        // joined multi-line detail
amount:      String        // no thousands separator, '.' decimal — pipeline convention
direction:   Direction     // In (no DB suffix) | Out (" DB" suffix)
is_transfer: bool          // set by category rules
raw:         String        // original block, for audit / review display
```

## Data flow

```
POST /ingest (PDF)
  └─ ingest_batch: save file, kind="pdf"
      └─ detect BCA (text contains "REKENING TAHAPAN" + "NO. REKENING")
          ├─ yes → bca_text → bca_parser → bca_category
          │        └─ per mutation: review_items::create(
          │             doc_type        = "bank_statement_bca",
          │             payload_json    = ExtractedEntry{
          │                 entry_type  = deposit|withdrawal,
          │                 amount_native, currency = "IDR", executed_at,
          │                 note = deskripsi, account_hint = "BCA <no_rek>" },
          │             needs_attention = is_transfer || low_confidence,
          │             external_ref    = hash(no_rek, tgl, amount, intra_day_index) )
          └─ no  → existing "PDF belum didukung" payload (unchanged)

PATCH /review/:id        → user edits category / direction (existing review UI)
POST  /review/:id/confirm
  └─ confirm(): create portfolio txn (Deposit/Withdrawal) as today
       + NEW: cashflow::insert_sourced(NewCashflow{
              account_id, occurred_on, direction (in/out), amount,
              currency = "IDR", category_id (from rule, get-or-create), note },
              source = "bank_statement_bca", external_ref = <item external_ref>)
```

### Deduplication & double-count avoidance

- `external_ref` derived from `(no_rek, tanggal, amount, intra_day_index)`.
  Re-uploading the same statement is idempotent: `cashflow::insert_sourced`
  already dedups on `(source, external_ref)`. A matching idempotency guard is
  added on the review-item side so re-upload does not pile duplicates into the
  queue.
- The same statement line produces one txn AND one cashflow row, linked by the
  shared `external_ref`. They live in different lenses (cash position vs Budget)
  and are never summed into a single figure.
- Transfer rows (TRSF) get `is_transfer=true` and the "Transfer" category, so the
  existing spending report can exclude them from "real" expenses.

## Parser rules (BCA Tahapan)

State machine over `pdftotext -layout` output:

- A line starting with a `\d{2}/\d{2}` date in the left column opens a new mutation.
- A line without a leading date is a continuation → appended to the current
  mutation's description (joins multi-line detail).
- MUTASI column: number with trailing ` DB` → **Out (withdrawal)**; number with
  no suffix → **In (deposit)**.
- `SALDO AWAL` line is skipped. The SALDO (running balance) column is used only as
  an **optional checksum**, not as a transaction.
- Numbers: strip thousands `,`, keep `.` decimal → precise string (existing
  pipeline convention).
- Year for `DD/MM` dates is resolved from the statement `PERIODE` (e.g. "MEI 2026").

## Categorization rules

Keyword match against KETERANGAN, first match wins (top to bottom):

| Keyword | Category | Note |
|---|---|---|
| `TRSF E-BANKING` / `FTFVA` / `FTSCY` | Transfer | `is_transfer=true` |
| `KARTU KREDIT` / `BCA CARD` | Kartu Kredit | CC payment (out) |
| `QRC` / `QR ` / `TRANSAKSI DEBIT` | Belanja/QRIS | merchant (out) |
| `BIAYA ADM` / `ADMIN` | Biaya Bank | fee |
| `BUNGA` | Bunga | income (in) |
| `PAJAK` | Pajak | fee |
| *(no match)* | *(uncategorized)* | flagged for review |

Categories are `get-or-create` by name (idempotent). User can override in review.

## Error handling

No `unwrap()` / `panic!()` in production (per project standards).

- `pdftotext` missing or failing → clear `anyhow` error; the batch falls back to
  the existing "PDF belum didukung" payload (no silent failure).
- Detected-not-BCA → existing PDF-unsupported behaviour, not an error.
- Ambiguous mutation (empty amount, invalid date) → that mutation is
  `needs_attention=true` with a `note` explaining why; never silently dropped.
- Saldo checksum mismatch → flag the batch `needs_attention`, `tracing::warn`,
  still route to review (do not reject the whole upload).

## Testing

- Fixture: `pdftotext -layout` output from a real statement, **redacted**
  (account number and name anonymized). Unit-test `bca_parser`: multi-line
  description join, DB vs CR direction, large transfer-in
  (`12/05 ... CR 49,995,500`), `SALDO AWAL` skipped, year resolution from PERIODE.
- Unit-test `bca_category` per rule.
- `confirm()` new path: a deposit/withdrawal yields both a txn AND a cashflow row
  sharing one `external_ref`; re-confirm / re-upload is idempotent (no duplicates).
- Follow backend convention: `cargo test` + `clippy`. **No `cargo fmt`.**

## Out of scope (YAGNI)

- Non-BCA bank statements (other layouts).
- Scanned/image-only PDFs (no text layer → would need OCR).
- Bank API auto-sync (no connector).
- Spending analytics/insights beyond the existing monthly summary.
