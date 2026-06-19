# BCA e-Statement Import Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Import a BCA "Rekening Tahapan" PDF e-statement so each mutation row becomes a review item that, on confirm, creates both a portfolio transaction and a cashflow entry for the Budget feature.

**Architecture:** A new `backend/src/ingestion/bank/` module extracts text from the PDF via `pdftotext -layout`, parses BCA's column layout deterministically into mutations, maps each to an `ExtractedEntry` with a keyword-derived cashflow category and a stable `external_ref`, and stages them as review items. The existing review/confirm flow is extended so confirming a `bank_statement_bca` deposit/withdrawal also writes a deduplicated `cashflow` row alongside the portfolio txn.

**Tech Stack:** Rust (tokio, sqlx, anyhow, thiserror, chrono, rust_decimal), SQLite, `pdftotext` (poppler-utils) invoked as a subprocess. No new Cargo dependency.

## Global Constraints

- No `unwrap()` / `panic!()` in production code paths — use `anyhow`/`thiserror`. (`unwrap()` is fine inside `#[cfg(test)]`.)
- Do NOT run `cargo fmt` / rustfmt. Verify with `cargo test` and `cargo clippy` only.
- Do NOT add a new crate to `Cargo.toml` — avoid Cargo.lock churn. Text extraction shells out to the `pdftotext` binary.
- Numbers stored as strings with no thousands separator and `.` as decimal point (existing pipeline convention).
- `cashflow.direction` is `"in"` | `"out"`. `cashflow_category.kind` is `"income"` | `"expense"`.
- Conventional commits (`feat:`, `fix:`, `refactor:`, `chore:`, `docs:`).

---

## File Structure

- Create: `backend/src/ingestion/bank/mod.rs` — module entry; `parse_statement(text) -> Vec<ExtractedEntry>` + BCA detection re-export.
- Create: `backend/src/ingestion/bank/bca_text.rs` — `extract_text(path)` (subprocess) + `is_bca_statement(text)` + `statement_meta(text)`.
- Create: `backend/src/ingestion/bank/bca_parser.rs` — `parse_mutations(text) -> Vec<BcaMutation>`, `BcaMutation`, `Direction`.
- Create: `backend/src/ingestion/bank/bca_category.rs` — `categorize(text) -> BcaCategory`.
- Modify: `backend/src/ingestion/mod.rs` — add `pub mod bank;`.
- Modify: `backend/src/ingestion/extract.rs` — add two `#[serde(default)]` fields to `ExtractedEntry`.
- Modify: `backend/src/ingestion/ingest.rs` — route detected BCA PDFs to the parser instead of the "unsupported" payload.
- Modify: `backend/src/ingestion/review.rs` — `confirm()` also writes a cashflow row for `bank_statement_bca` deposit/withdrawal items.
- Modify: `backend/Dockerfile` (backend image) — install `poppler-utils`.

---

## Task 1: BCA detection + statement metadata (pure text helpers)

**Files:**
- Create: `backend/src/ingestion/bank/bca_text.rs`
- Modify: `backend/src/ingestion/mod.rs`

**Interfaces:**
- Produces:
  - `pub fn is_bca_statement(text: &str) -> bool`
  - `pub struct StatementMeta { pub account_no: String, pub year: i32 }`
  - `pub fn statement_meta(text: &str) -> anyhow::Result<StatementMeta>`
  - `pub async fn extract_text(path: &str) -> anyhow::Result<String>` (added in Task 1 Step 6; not unit-tested)

- [ ] **Step 1: Declare the module**

In `backend/src/ingestion/mod.rs`, add the line (keep alphabetical-ish grouping with the others):

```rust
pub mod bank;
```

- [ ] **Step 2: Write failing tests for detection + metadata**

Create `backend/src/ingestion/bank/bca_text.rs` with only the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
                                                     REKENING TAHAPAN
    KCP PONDOK TIMUR

    B I M A P A N GE STU                          NO. RE KE NING   :    8415 5 25 237
                                                  HALAMAN          :    1/3
                                                  PE RIOD E        :    ME I 2026
                                                  MATA U ANG       :    ID R
";

    #[test]
    fn detects_bca_statement() {
        assert!(is_bca_statement(SAMPLE));
        assert!(!is_bca_statement("just some random invoice text"));
    }

    #[test]
    fn parses_account_no_and_year() {
        let m = statement_meta(SAMPLE).unwrap();
        assert_eq!(m.account_no, "8415525237");
        assert_eq!(m.year, 2026);
    }
}
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd backend && cargo test ingestion::bank::bca_text -- --nocapture`
Expected: FAIL — `is_bca_statement` / `statement_meta` / `StatementMeta` not found.

- [ ] **Step 4: Implement detection + metadata**

Prepend to `backend/src/ingestion/bank/bca_text.rs` (above the test module). Note: `pdftotext -layout` spaces letters out (e.g. `NO. RE KE NING`), so we strip whitespace before matching keywords and digits.

```rust
//! Text extraction and structural detection for BCA "Rekening Tahapan" PDFs.

/// Statement-level fields needed to build stable external refs and resolve dates.
#[derive(Debug, Clone, PartialEq)]
pub struct StatementMeta {
    pub account_no: String,
    pub year: i32,
}

/// Whitespace-stripped, uppercased copy for keyword matching. `pdftotext -layout`
/// frequently inserts spaces inside words, so we cannot match on raw substrings.
fn squashed(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect::<String>().to_uppercase()
}

/// True when the document looks like a BCA Tahapan statement.
pub fn is_bca_statement(text: &str) -> bool {
    let s = squashed(text);
    s.contains("REKENINGTAHAPAN") && s.contains("NO.REKENING")
}

/// Indonesian month name (as it appears squashed in the PERIODE line) -> month number.
fn month_from_periode(squashed_text: &str) -> Option<u32> {
    const MONTHS: [(&str, u32); 12] = [
        ("JANUARI", 1), ("FEBRUARI", 2), ("MARET", 3), ("APRIL", 4),
        ("MEI", 5), ("JUNI", 6), ("JULI", 7), ("AGUSTUS", 8),
        ("SEPTEMBER", 9), ("OKTOBER", 10), ("NOVEMBER", 11), ("DESEMBER", 12),
    ];
    let after = squashed_text.split("PERIODE").nth(1)?;
    MONTHS.iter().find(|(name, _)| after.contains(name)).map(|(_, n)| *n)
}

/// Extract the account number and statement year from the header.
pub fn statement_meta(text: &str) -> anyhow::Result<StatementMeta> {
    let s = squashed(text);
    // Account number: the run of digits immediately after "NO.REKENING".
    let after_acct = s.split("NO.REKENING").nth(1)
        .ok_or_else(|| anyhow::anyhow!("no NO.REKENING marker"))?;
    let account_no: String = after_acct.chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    if account_no.is_empty() {
        anyhow::bail!("could not read account number");
    }
    // Year: the 4-digit run after the month name in the PERIODE line.
    let _month = month_from_periode(&s); // validated for presence; row dates carry MM
    let after_periode = s.split("PERIODE").nth(1)
        .ok_or_else(|| anyhow::anyhow!("no PERIODE marker"))?;
    let year_str: String = after_periode.chars()
        .skip_while(|c| !c.is_ascii_digit())
        // skip the leading "1/3"-style page noise is avoided because PERIODE is its own field;
        // take the first standalone 4-digit run that is a plausible year.
        .collect();
    let year = year_str.as_bytes().windows(4)
        .filter_map(|w| std::str::from_utf8(w).ok())
        .filter_map(|w| w.parse::<i32>().ok())
        .find(|y| (2000..=2100).contains(y))
        .ok_or_else(|| anyhow::anyhow!("could not read statement year"))?;
    Ok(StatementMeta { account_no, year })
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd backend && cargo test ingestion::bank::bca_text -- --nocapture`
Expected: PASS (both tests).

- [ ] **Step 6: Add the subprocess text extractor (no unit test)**

Append above the test module in `backend/src/ingestion/bank/bca_text.rs`:

```rust
/// Render a PDF's text layer using `pdftotext -layout`, which preserves the
/// column alignment our parser relies on. Returns a clear error if the binary
/// is missing or the file has no extractable text.
pub async fn extract_text(path: &str) -> anyhow::Result<String> {
    let path = path.to_string();
    let output = tokio::process::Command::new("pdftotext")
        .arg("-layout")
        .arg(&path)
        .arg("-") // write to stdout
        .output()
        .await
        .map_err(|e| anyhow::anyhow!("failed to run pdftotext (is poppler-utils installed?): {e}"))?;
    if !output.status.success() {
        anyhow::bail!(
            "pdftotext failed for {path}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    if text.trim().is_empty() {
        anyhow::bail!("pdftotext produced no text for {path} (scanned/image-only PDF?)");
    }
    Ok(text)
}
```

- [ ] **Step 7: Verify it still compiles + clippy clean**

Run: `cd backend && cargo test ingestion::bank::bca_text && cargo clippy --all-targets -- -D warnings`
Expected: PASS, no clippy errors in the new file.

- [ ] **Step 8: Commit**

```bash
git add backend/src/ingestion/mod.rs backend/src/ingestion/bank/bca_text.rs
git commit -m "feat(ingestion): BCA statement detection + text extraction"
```

---

## Task 2: BCA mutation parser

**Files:**
- Create: `backend/src/ingestion/bank/bca_parser.rs`
- Modify: `backend/src/ingestion/bank/mod.rs` (create with `mod` declarations — see Step 1)

**Interfaces:**
- Consumes: `StatementMeta` from Task 1 (`super::bca_text::StatementMeta`).
- Produces:
  - `pub enum Direction { In, Out }`
  - `pub struct BcaMutation { pub date: chrono::NaiveDate, pub jenis: String, pub deskripsi: String, pub amount: String, pub direction: Direction }`
  - `pub fn parse_mutations(text: &str, meta: &StatementMeta) -> Vec<BcaMutation>`

- [ ] **Step 1: Create the module file with submodule declarations**

Create `backend/src/ingestion/bank/mod.rs`:

```rust
//! BCA "Rekening Tahapan" e-statement import.
pub mod bca_category;
pub mod bca_parser;
pub mod bca_text;
```

- [ ] **Step 2: Write failing parser tests**

Create `backend/src/ingestion/bank/bca_parser.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::bank::bca_text::StatementMeta;

    // Real `pdftotext -layout` shape: date in the left column opens a row;
    // continuation lines have no leading date; MUTASI carries a trailing " DB"
    // for debits and nothing for credits.
    const ROWS: &str = "\
       01/05         SALDO AWAL                                                          4,153,064.29
       01/05         TRSF E-BANKING DB    0105/FTFVA/WS95271                242,000.00 DB     3,911,064.29
                                          38165/PT Moratelin
       01/05         TRANSAKSI DEBIT      TGL: 01/05                        137,000.00 DB
                                          QRC014
                                          00000.00IDM INDOMA
       12/05         TRSF E-BANKING CR    1205/FTSCY/WS95051             49,995,500.00        40,831,664.29
                                          SINAR DIGITAL TERD
";

    fn meta() -> StatementMeta { StatementMeta { account_no: "8415525237".into(), year: 2026 } }

    #[test]
    fn skips_saldo_awal() {
        let m = parse_mutations(ROWS, &meta());
        assert!(m.iter().all(|x| !x.jenis.contains("SALDO AWAL")));
    }

    #[test]
    fn parses_three_mutations_with_direction_and_amount() {
        let m = parse_mutations(ROWS, &meta());
        assert_eq!(m.len(), 3);

        assert_eq!(m[0].jenis, "TRSF E-BANKING DB");
        assert_eq!(m[0].amount, "242000.00");
        assert!(matches!(m[0].direction, Direction::Out));
        assert_eq!(m[0].date, chrono::NaiveDate::from_ymd_opt(2026, 5, 1).unwrap());
        assert!(m[0].deskripsi.contains("PT Moratelin"));

        assert_eq!(m[1].jenis, "TRANSAKSI DEBIT");
        assert_eq!(m[1].amount, "137000.00");
        assert!(matches!(m[1].direction, Direction::Out));
        assert!(m[1].deskripsi.contains("QRC014"));

        assert_eq!(m[2].jenis, "TRSF E-BANKING CR");
        assert_eq!(m[2].amount, "49995500.00");
        assert!(matches!(m[2].direction, Direction::In));
    }
}
```

- [ ] **Step 3: Run tests to verify they fail**

Run: `cd backend && cargo test ingestion::bank::bca_parser -- --nocapture`
Expected: FAIL — `parse_mutations` / `BcaMutation` / `Direction` not found.

- [ ] **Step 4: Implement the parser**

Prepend to `backend/src/ingestion/bank/bca_parser.rs`:

```rust
//! Deterministic parser for BCA Tahapan statements rendered with `pdftotext -layout`.

use crate::ingestion::bank::bca_text::StatementMeta;

#[derive(Debug, Clone, PartialEq)]
pub enum Direction {
    In,
    Out,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BcaMutation {
    pub date: chrono::NaiveDate,
    pub jenis: String,
    pub deskripsi: String,
    pub amount: String,
    pub direction: Direction,
}

/// A line that opens a new mutation starts (after indentation) with `DD/MM`.
fn leading_date(line: &str) -> Option<(u32, u32)> {
    let t = line.trim_start();
    let bytes = t.as_bytes();
    if bytes.len() >= 5
        && bytes[0].is_ascii_digit() && bytes[1].is_ascii_digit()
        && bytes[2] == b'/'
        && bytes[3].is_ascii_digit() && bytes[4].is_ascii_digit()
    {
        let dd = t[0..2].parse().ok()?;
        let mm = t[3..5].parse().ok()?;
        return Some((dd, mm));
    }
    None
}

/// Pull the first money-like token (`1,234.56`) and whether it is a debit
/// (trailing " DB"). Returns the normalized amount (no separators) + direction.
fn money_and_direction(rest: &str) -> Option<(String, Direction)> {
    // Find a token matching <digits with commas>.<2 digits>.
    let tokens: Vec<&str> = rest.split_whitespace().collect();
    for (i, tok) in tokens.iter().enumerate() {
        let cleaned: String = tok.chars().filter(|c| *c != ',').collect();
        if cleaned.contains('.')
            && cleaned.chars().all(|c| c.is_ascii_digit() || c == '.')
            && cleaned.split('.').nth(1).map(|f| f.len() == 2).unwrap_or(false)
        {
            // Debit if this token (or the next) is "DB".
            let is_db = tok.ends_with("DB")
                || tokens.get(i + 1).map(|n| *n == "DB").unwrap_or(false);
            let direction = if is_db { Direction::Out } else { Direction::In };
            // The first money token is MUTASI; a later one would be SALDO — we
            // only take the first, which is always the transaction amount.
            return Some((cleaned, direction));
        }
    }
    None
}

/// The KETERANGAN "type" sits between the date and the first detail/amount.
/// In `-layout` output it is the run of words after the date column and before
/// the long whitespace gap that precedes the detail/MUTASI columns.
fn split_jenis_and_first_detail(after_date: &str) -> (String, String) {
    // Columns are separated by 2+ spaces. First chunk after the date is jenis;
    // the remainder (detail + amount) we keep raw for description harvesting.
    let trimmed = after_date.trim_start();
    // jenis = leading words up to a run of 2+ spaces.
    if let Some(gap) = trimmed.find("  ") {
        let jenis = trimmed[..gap].trim().to_string();
        let detail = trimmed[gap..].trim().to_string();
        (jenis, detail)
    } else {
        (trimmed.trim().to_string(), String::new())
    }
}

/// Strip the money/SALDO tail off a detail line so descriptions stay clean.
fn detail_without_money(detail: &str) -> String {
    detail
        .split_whitespace()
        .take_while(|t| {
            let cleaned: String = t.chars().filter(|c| *c != ',').collect();
            !(cleaned.contains('.')
                && cleaned.chars().all(|c| c.is_ascii_digit() || c == '.'))
                && *t != "DB"
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn parse_mutations(text: &str, meta: &StatementMeta) -> Vec<BcaMutation> {
    let mut out: Vec<BcaMutation> = Vec::new();
    for line in text.lines() {
        if let Some((dd, mm)) = leading_date(line) {
            let t = line.trim_start();
            let after_date = &t[5..]; // skip "DD/MM"
            let (jenis, first_detail) = split_jenis_and_first_detail(after_date);
            if jenis.eq_ignore_ascii_case("SALDO AWAL") {
                continue;
            }
            let Some(date) = chrono::NaiveDate::from_ymd_opt(meta.year, mm, dd) else {
                continue;
            };
            match money_and_direction(after_date) {
                Some((amount, direction)) => {
                    out.push(BcaMutation {
                        date,
                        jenis,
                        deskripsi: detail_without_money(&first_detail),
                        amount,
                        direction,
                    });
                }
                None => {
                    // A dated row with no money token is malformed; record it
                    // with a zero amount so it surfaces in review rather than
                    // vanishing. amount "0.00" + downstream force_attention.
                    out.push(BcaMutation {
                        date,
                        jenis,
                        deskripsi: detail_without_money(&first_detail),
                        amount: "0.00".to_string(),
                        direction: Direction::Out,
                    });
                }
            }
        } else if let Some(last) = out.last_mut() {
            // Continuation line: append non-money detail to the current mutation.
            let extra = detail_without_money(line.trim());
            if !extra.is_empty() {
                if !last.deskripsi.is_empty() {
                    last.deskripsi.push(' ');
                }
                last.deskripsi.push_str(&extra);
            }
        }
    }
    out
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cd backend && cargo test ingestion::bank::bca_parser -- --nocapture`
Expected: PASS (all three tests).

- [ ] **Step 6: Clippy**

Run: `cd backend && cargo clippy --all-targets -- -D warnings`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add backend/src/ingestion/bank/mod.rs backend/src/ingestion/bank/bca_parser.rs
git commit -m "feat(ingestion): deterministic BCA mutation parser"
```

---

## Task 3: Keyword categorization

**Files:**
- Create: `backend/src/ingestion/bank/bca_category.rs`

**Interfaces:**
- Produces:
  - `pub struct BcaCategory { pub name: &'static str, pub kind: &'static str, pub is_transfer: bool }`
  - `pub fn categorize(haystack: &str) -> BcaCategory`

- [ ] **Step 1: Write failing tests**

Create `backend/src/ingestion/bank/bca_category.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_is_flagged() {
        let c = categorize("TRSF E-BANKING DB 0105/FTFVA/WS95271 PT Moratelin");
        assert_eq!(c.name, "Transfer");
        assert!(c.is_transfer);
    }

    #[test]
    fn qris_merchant_is_belanja() {
        let c = categorize("TRANSAKSI DEBIT TGL: 01/05 QRC014 IDM INDOMA");
        assert_eq!(c.name, "Belanja/QRIS");
        assert!(!c.is_transfer);
    }

    #[test]
    fn credit_card_payment() {
        let c = categorize("KARTU KREDIT/PL 0100 BCA CARD BIMA PANGESTU");
        assert_eq!(c.name, "Kartu Kredit");
    }

    #[test]
    fn interest_and_fee_and_default() {
        assert_eq!(categorize("BUNGA").name, "Bunga");
        assert_eq!(categorize("BIAYA ADM").name, "Biaya Bank");
        assert_eq!(categorize("SOMETHING UNRECOGNIZED").name, "Lainnya");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd backend && cargo test ingestion::bank::bca_category -- --nocapture`
Expected: FAIL — `categorize` / `BcaCategory` not found.

- [ ] **Step 3: Implement the rules**

Prepend to `backend/src/ingestion/bank/bca_category.rs`:

```rust
//! Maps a BCA mutation's KETERANGAN text to a cashflow category.

#[derive(Debug, Clone, PartialEq)]
pub struct BcaCategory {
    pub name: &'static str,
    pub kind: &'static str, // "income" | "expense"
    pub is_transfer: bool,
}

/// First matching rule wins. `kind` is the default used when the category is
/// first created; the cashflow row's own direction is what reporting keys on.
pub fn categorize(haystack: &str) -> BcaCategory {
    let h = haystack.to_uppercase();
    let has = |needle: &str| h.contains(needle);

    if has("TRSF E-BANKING") || has("FTFVA") || has("FTSCY") {
        BcaCategory { name: "Transfer", kind: "expense", is_transfer: true }
    } else if has("KARTU KREDIT") || has("BCA CARD") {
        BcaCategory { name: "Kartu Kredit", kind: "expense", is_transfer: false }
    } else if has("QRC") || has("QR ") || has("TRANSAKSI DEBIT") {
        BcaCategory { name: "Belanja/QRIS", kind: "expense", is_transfer: false }
    } else if has("BIAYA ADM") || has("ADMIN") {
        BcaCategory { name: "Biaya Bank", kind: "expense", is_transfer: false }
    } else if has("BUNGA") {
        BcaCategory { name: "Bunga", kind: "income", is_transfer: false }
    } else if has("PAJAK") {
        BcaCategory { name: "Pajak", kind: "expense", is_transfer: false }
    } else {
        BcaCategory { name: "Lainnya", kind: "expense", is_transfer: false }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd backend && cargo test ingestion::bank::bca_category -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/ingestion/bank/bca_category.rs
git commit -m "feat(ingestion): keyword categorization for BCA mutations"
```

---

## Task 4: Map mutations → ExtractedEntry (with provenance fields)

**Files:**
- Modify: `backend/src/ingestion/extract.rs` (add two fields to `ExtractedEntry`)
- Modify: `backend/src/ingestion/bank/mod.rs` (add `parse_statement`)

**Interfaces:**
- Consumes: `parse_mutations`, `BcaMutation`, `Direction` (Task 2); `categorize` (Task 3); `is_bca_statement`, `statement_meta`, `StatementMeta` (Task 1); `ExtractedEntry` (extract.rs).
- Produces:
  - New fields on `ExtractedEntry`: `pub cashflow_category: Option<String>`, `pub external_ref: Option<String>`.
  - `pub fn parse_statement(text: &str) -> anyhow::Result<Vec<ExtractedEntry>>` in `bank/mod.rs`.

- [ ] **Step 1: Add provenance fields to `ExtractedEntry`**

In `backend/src/ingestion/extract.rs`, inside `struct ExtractedEntry`, add after the `force_attention` field (these mirror the existing non-LLM `force_attention` precedent — deterministic post-processing only):

```rust
    /// Cashflow category name chosen by deterministic bank-statement parsing
    /// (e.g. BCA). Read at confirm time to attach the cashflow row's category.
    #[serde(default)]
    pub cashflow_category: Option<String>,
    /// Stable provenance key for deduplicating bank-statement rows across
    /// re-uploads. Set by deterministic parsing, never by the LLM.
    #[serde(default)]
    pub external_ref: Option<String>,
```

- [ ] **Step 2: Write a failing test for `parse_statement`**

Append a test module to `backend/src/ingestion/bank/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const DOC: &str = "\
                                                     REKENING TAHAPAN
    NO. RE KE NING   :    8415 5 25 237
    PE RIOD E        :    ME I 2026

       01/05         TRSF E-BANKING DB    0105/FTFVA/WS95271                242,000.00 DB     3,911,064.29
                                          38165/PT Moratelin
       01/05         TRANSAKSI DEBIT      TGL: 01/05                        137,000.00 DB
                                          QRC014
       12/05         TRSF E-BANKING CR    1205/FTSCY/WS95051             49,995,500.00        40,831,664.29
";

    #[test]
    fn builds_entries_with_provenance() {
        let entries = parse_statement(DOC).unwrap();
        assert_eq!(entries.len(), 3);

        let e0 = &entries[0];
        assert_eq!(e0.entry_type, "withdrawal");
        assert_eq!(e0.amount_native.as_deref(), Some("242000.00"));
        assert_eq!(e0.currency.as_deref(), Some("IDR"));
        assert_eq!(e0.executed_at.as_deref(), Some("2026-05-01T00:00:00Z"));
        assert_eq!(e0.cashflow_category.as_deref(), Some("Transfer"));
        assert_eq!(e0.external_ref.as_deref(), Some("bca:8415525237:2026-05-01:242000.00:0"));
        assert!(e0.account_hint.as_deref().unwrap().contains("8415525237"));

        let e2 = &entries[2];
        assert_eq!(e2.entry_type, "deposit");
        assert_eq!(e2.cashflow_category.as_deref(), Some("Transfer"));
    }

    #[test]
    fn rejects_non_bca() {
        assert!(parse_statement("random text").is_err());
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd backend && cargo test ingestion::bank::tests -- --nocapture`
Expected: FAIL — `parse_statement` not found.

- [ ] **Step 4: Implement `parse_statement`**

In `backend/src/ingestion/bank/mod.rs`, replace the module header so it has the use-imports and function above the test module:

```rust
//! BCA "Rekening Tahapan" e-statement import.
pub mod bca_category;
pub mod bca_parser;
pub mod bca_text;

use crate::ingestion::extract::ExtractedEntry;
use bca_parser::Direction;

/// Parse a BCA statement's `pdftotext -layout` text into candidate ledger
/// entries with cashflow category + dedup provenance attached. Errors if the
/// text is not a recognizable BCA statement.
pub fn parse_statement(text: &str) -> anyhow::Result<Vec<ExtractedEntry>> {
    if !bca_text::is_bca_statement(text) {
        anyhow::bail!("not a BCA statement");
    }
    let meta = bca_text::statement_meta(text)?;
    let mutations = bca_parser::parse_mutations(text, &meta);

    // Disambiguate identical (date, amount) rows by their order within a day.
    let mut per_day: std::collections::HashMap<chrono::NaiveDate, usize> =
        std::collections::HashMap::new();

    let mut entries = Vec::with_capacity(mutations.len());
    for m in mutations {
        let idx = per_day.entry(m.date).or_insert(0);
        let intra_day = *idx;
        *idx += 1;

        let entry_type = match m.direction {
            Direction::In => "deposit",
            Direction::Out => "withdrawal",
        };
        let cat = bca_category::categorize(&format!("{} {}", m.jenis, m.deskripsi));
        let external_ref = format!(
            "bca:{}:{}:{}:{}",
            meta.account_no, m.date, m.amount, intra_day
        );
        let malformed = m.amount == "0.00";

        entries.push(ExtractedEntry {
            entry_type: entry_type.to_string(),
            symbol: None,
            instrument_name: None,
            quantity: None,
            price_native: None,
            fee_native: None,
            currency: Some("IDR".to_string()),
            executed_at: Some(format!("{}T00:00:00Z", m.date)),
            account_hint: Some(format!("BCA {}", meta.account_no)),
            note: Some(format!("{} {}", m.jenis, m.deskripsi).trim().to_string()),
            confidence: if malformed { 0.3 } else { 1.0 },
            amount_native: Some(m.amount.clone()),
            force_attention: malformed || cat.is_transfer,
            cashflow_category: Some(cat.name.to_string()),
            external_ref: Some(external_ref),
        });
    }
    Ok(entries)
}
```

- [ ] **Step 5: Fix every other `ExtractedEntry { .. }` literal**

The two new fields break existing struct literals. Find them:

Run: `cd backend && cargo build 2>&1 | grep -n "missing field" | head`

For each reported site (notably in `backend/src/ingestion/extract.rs` `normalize_entry`/tests and `backend/src/ingestion/matching.rs` if any construct the struct), add `cashflow_category: None,` and `external_ref: None,`. Prefer `..Default::default()` only if the struct already derives `Default` — it does NOT, so add the two fields explicitly.

- [ ] **Step 6: Run tests + build**

Run: `cd backend && cargo test ingestion::bank && cargo build`
Expected: PASS, build clean.

- [ ] **Step 7: Clippy**

Run: `cd backend && cargo clippy --all-targets -- -D warnings`
Expected: no errors.

- [ ] **Step 8: Commit**

```bash
git add backend/src/ingestion/extract.rs backend/src/ingestion/bank/mod.rs
git commit -m "feat(ingestion): map BCA mutations to ExtractedEntry with provenance"
```

---

## Task 5: Route detected BCA PDFs through the parser

**Files:**
- Modify: `backend/src/ingestion/ingest.rs` (the PDF branch in `ingest_batch`, ~lines 123-138)

**Interfaces:**
- Consumes: `bank::parse_statement`, `bank::bca_text::extract_text` (Tasks 1 & 4); `review_items::create`, `NewReviewItem` (existing).
- Produces: side effect — staged review items with `doc_type = "bank_statement_bca"`.

- [ ] **Step 1: Add a helper that builds review items from a BCA PDF**

In `backend/src/ingestion/ingest.rs`, add this function near `save_file` (it isolates the new I/O+parse so the branch in `ingest_batch` stays small):

```rust
/// Try to handle an uploaded PDF as a BCA statement. Returns the staged review
/// items on success, or `None` if the PDF is not a recognizable BCA statement
/// (caller then falls back to the "unsupported" payload). Extraction/parse
/// errors propagate so the API surfaces a real cause.
async fn try_ingest_bca_pdf(
    db: &Db,
    batch_id: &str,
    f: &UploadFile,
    kind: &str,
    path: &str,
) -> anyhow::Result<Option<Vec<review_items::ReviewItemRow>>> {
    let text = crate::ingestion::bank::bca_text::extract_text(path).await?;
    if !crate::ingestion::bank::bca_text::is_bca_statement(&text) {
        return Ok(None);
    }
    let entries = crate::ingestion::bank::parse_statement(&text)?;
    let mut rows = Vec::with_capacity(entries.len());
    for e in &entries {
        let payload = serde_json::to_string(e)?;
        let row = review_items::create(db, &NewReviewItem {
            batch_id,
            source_kind: kind,
            source_filename: &f.filename,
            source_path: path,
            doc_type: "bank_statement_bca",
            needs_attention: e.force_attention,
            payload_json: &payload,
            raw_llm_json: "{}",
            suggested_instrument_id: None,
            suggested_account_id: None,
        }).await?;
        rows.push(row);
    }
    Ok(Some(rows))
}
```

Ensure `review_items::ReviewItemRow` is reachable (the file already `use`s `review_items`; if not, add `use crate::repo::review_items;` and `use crate::db::Db;` as needed — check existing imports first).

- [ ] **Step 2: Rewrite the PDF branch to try BCA first**

In `ingest_batch`, replace the existing PDF block (the `if f.media_type == "application/pdf" { ... continue; }` that stages `PDF_UNSUPPORTED_PAYLOAD`) with:

```rust
        if f.media_type == "application/pdf" {
            match try_ingest_bca_pdf(db, batch_id, f, &kind, &path).await {
                Ok(Some(rows)) => {
                    items.extend(rows);
                    continue;
                }
                Ok(None) => { /* not BCA — fall through to unsupported */ }
                Err(e) => {
                    tracing::warn!("ingest: BCA PDF handling failed for {}: {e:#}", f.filename);
                    // fall through to the unsupported payload rather than 500ing
                }
            }
            let row = review_items::create(db, &NewReviewItem {
                batch_id,
                source_kind: &kind,
                source_filename: &f.filename,
                source_path: &path,
                doc_type: "unknown",
                needs_attention: true,
                payload_json: PDF_UNSUPPORTED_PAYLOAD,
                raw_llm_json: "{}",
                suggested_instrument_id: None,
                suggested_account_id: None,
            }).await?;
            items.push(row);
            continue;
        }
```

- [ ] **Step 3: Build + clippy**

Run: `cd backend && cargo build && cargo clippy --all-targets -- -D warnings`
Expected: clean. (No new unit test here: this glue needs the `pdftotext` binary + a real file; it is covered by the manual verification in Task 8 and the parser's own unit tests.)

- [ ] **Step 4: Commit**

```bash
git add backend/src/ingestion/ingest.rs
git commit -m "feat(ingestion): route BCA statement PDFs to the deterministic parser"
```

---

## Task 6: Confirm writes a cashflow row for BCA deposit/withdrawal

**Files:**
- Modify: `backend/src/ingestion/review.rs` (`confirm()`, after `transactions::create`)

**Interfaces:**
- Consumes: `cashflow::insert_sourced`, `NewCashflow` (repo/cashflow.rs); `cashflow_categories::ensure_by_name` (repo/cashflow_categories.rs); `ExtractedEntry` (extract.rs); stored `item.payload_json`, `item.doc_type`.
- Produces: side effect — a deduplicated `cashflow` row linked by `external_ref`.

- [ ] **Step 1: Write a failing test for the confirm→cashflow path**

Add to the `#[cfg(test)] mod tests` in `backend/src/ingestion/review.rs` (follow the existing test setup in that module for seeding account/instrument; if the module lacks a memory-db helper, mirror the FK-seeding pattern from `repo/review_items.rs` tests):

```rust
#[tokio::test]
async fn confirm_bca_withdrawal_creates_txn_and_cashflow() {
    use crate::repo::{cashflow, review_items};
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let now = chrono::Utc::now().to_rfc3339();

    // Seed a Cash instrument and a BCA account (FKs for the txn).
    let account_id = sqlx::query(
        "INSERT INTO account (name, account_type, native_currency, created_at) VALUES (?,?,?,?)")
        .bind("BCA").bind("bank").bind("IDR").bind(&now)
        .execute(&db).await.unwrap().last_insert_rowid();
    let instrument_id = sqlx::query(
        "INSERT INTO instrument (symbol, name, instrument_type, native_currency, price_source) VALUES (?,?,?,?,?)")
        .bind("CASHIDR").bind("Cash IDR").bind("cash").bind("IDR").bind("manual")
        .execute(&db).await.unwrap().last_insert_rowid();

    // Stage a BCA withdrawal review item carrying provenance in its payload.
    let payload = serde_json::json!({
        "entry_type": "withdrawal",
        "currency": "IDR",
        "executed_at": "2026-05-01T00:00:00Z",
        "amount_native": "242000.00",
        "cashflow_category": "Transfer",
        "external_ref": "bca:8415525237:2026-05-01:242000.00:0",
        "note": "TRSF E-BANKING DB PT Moratelin"
    }).to_string();
    let item = review_items::create(&db, &crate::repo::review_items::NewReviewItem {
        batch_id: "b1", source_kind: "pdf", source_filename: "s.pdf", source_path: "",
        doc_type: "bank_statement_bca", needs_attention: false,
        payload_json: &payload, raw_llm_json: "{}",
        suggested_instrument_id: None, suggested_account_id: None,
    }).await.unwrap();

    let p = ConfirmPayload {
        account_id, instrument_id, entry_type: "withdrawal".into(),
        executed_at: "2026-05-01T00:00:00Z".into(),
        quantity: String::new(), price_native: String::new(),
        fee_native: None, currency: "IDR".into(),
        fx_to_idr: Some("1".into()), fx_to_usd: Some("1".into()),
        note: None, amount_native: Some("242000.00".into()),
    };
    let txn_id = confirm(&db, item.id, &p).await.unwrap();
    assert!(txn_id > 0);

    let rows = cashflow::list_all(&db).await.unwrap();
    assert_eq!(rows.len(), 1, "one cashflow row created");
    assert_eq!(rows[0].direction, "out");
    assert_eq!(rows[0].amount, "242000.00");
    assert_eq!(rows[0].source.as_deref(), Some("bank_statement_bca"));
    assert_eq!(rows[0].external_ref.as_deref(), Some("bca:8415525237:2026-05-01:242000.00:0"));
    assert!(rows[0].category_id.is_some(), "Transfer category attached");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd backend && cargo test ingestion::review::tests::confirm_bca_withdrawal -- --nocapture`
Expected: FAIL — no cashflow row created (current `confirm` only makes a txn).

- [ ] **Step 3: Implement the cashflow side-effect in `confirm()`**

In `backend/src/ingestion/review.rs`, immediately after `let txn = transactions::create(db, &nt).await?;` and before `review_items::mark_confirmed(...)`, insert:

```rust
    // Bank-statement entries also feed the cashflow/Budget view. We read the
    // category + dedup ref from the stored extraction (the user-facing
    // ConfirmPayload does not carry them), and key direction off entry_type.
    if item.doc_type == "bank_statement_bca"
        && matches!(p.entry_type.as_str(), "deposit" | "withdrawal")
    {
        if let Err(e) = write_bank_cashflow(db, &item, p).await {
            // Don't fail the whole confirm if the cashflow mirror fails; the
            // txn is the source of truth. Surface it loudly for follow-up.
            tracing::warn!("confirm: cashflow mirror failed for item {}: {e:#}", item.id);
        }
    }
```

Then add this helper function below `confirm` in the same file:

```rust
/// Mirror a confirmed bank-statement deposit/withdrawal into the cashflow table.
/// Idempotent on `(source, external_ref)` so re-imports never double-count.
async fn write_bank_cashflow(
    db: &Db,
    item: &crate::repo::review_items::ReviewItemRow,
    p: &ConfirmPayload,
) -> anyhow::Result<()> {
    use crate::repo::{cashflow, cashflow_categories};
    let stored: crate::ingestion::extract::ExtractedEntry =
        serde_json::from_str(&item.payload_json)?;
    let external_ref = stored.external_ref
        .ok_or_else(|| anyhow::anyhow!("bank_statement item missing external_ref"))?;

    let direction = if p.entry_type == "deposit" { "in" } else { "out" };
    let amount = p.amount_native.clone()
        .ok_or_else(|| anyhow::anyhow!("bank cashflow needs amount_native"))?;
    let occurred_on = crate::ingestion::review::to_rfc3339(&p.executed_at)
        .unwrap_or_else(|| p.executed_at.clone());
    let occurred_on = occurred_on.get(0..10).unwrap_or(&occurred_on).to_string();

    let category_id = match stored.cashflow_category.as_deref() {
        Some(name) if !name.is_empty() => {
            let kind = if direction == "in" { "income" } else { "expense" };
            Some(cashflow_categories::ensure_by_name(db, name, kind).await?.id)
        }
        _ => None,
    };

    cashflow::insert_sourced(
        db,
        &cashflow::NewCashflow {
            account_id: Some(p.account_id),
            occurred_on,
            direction: direction.to_string(),
            amount,
            currency: p.currency.clone(),
            category_id,
            note: p.note.clone().or(stored.note),
        },
        "bank_statement_bca",
        &external_ref,
    ).await?;
    Ok(())
}
```

Confirm the imports at the top of `review.rs` allow `Db` (already used) and that `to_rfc3339` is `pub` (it is, per the existing `pub fn to_rfc3339`). If `write_bank_cashflow` cannot see `to_rfc3339` via the module path, call it unqualified since it is in the same module: `to_rfc3339(&p.executed_at)`.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test ingestion::review::tests::confirm_bca_withdrawal -- --nocapture`
Expected: PASS.

- [ ] **Step 5: Run the full review + cashflow test modules + clippy**

Run: `cd backend && cargo test ingestion::review && cargo test repo::cashflow && cargo clippy --all-targets -- -D warnings`
Expected: PASS, no clippy errors.

- [ ] **Step 6: Commit**

```bash
git add backend/src/ingestion/review.rs
git commit -m "feat(ingestion): confirm BCA deposit/withdrawal mirrors to cashflow"
```

---

## Task 7: Install poppler-utils in the backend image

**Files:**
- Modify: `backend/Dockerfile`

**Interfaces:** none (deployment dependency only).

- [ ] **Step 1: Inspect the runtime stage**

Run: `cd backend && sed -n '1,80p' Dockerfile` (or open it). Identify the final/runtime stage's base image (Debian/Ubuntu slim vs Alpine) — the install command differs.

- [ ] **Step 2: Add the package to the runtime stage**

For a Debian/Ubuntu-based runtime stage, add after the `FROM ... ` of the final stage (adjust to match existing `apt-get` usage in the file):

```dockerfile
RUN apt-get update \
    && apt-get install -y --no-install-recommends poppler-utils \
    && rm -rf /var/lib/apt/lists/*
```

For an Alpine-based runtime stage instead:

```dockerfile
RUN apk add --no-cache poppler-utils
```

- [ ] **Step 3: Verify the binary is present in the built image**

Run (Debian example): `cd backend && docker build -t pt-backend-test . && docker run --rm pt-backend-test pdftotext -v`
Expected: prints a `pdftotext version ...` banner (poppler) to stderr, exit 0.

If Docker is unavailable locally, note that CI builds the image on push; verify there and confirm `pdftotext -v` works in a running container before relying on PDF import in prod.

- [ ] **Step 4: Commit**

```bash
git add backend/Dockerfile
git commit -m "chore(backend): install poppler-utils for PDF text extraction"
```

---

## Task 8: End-to-end manual verification

**Files:** none (verification only).

**Interfaces:** none.

- [ ] **Step 1: Confirm the frontend already accepts PDF**

Run: `grep -n "application/pdf\|accept" frontend/src/lib/upload.ts frontend/src/pages/ImportPage.tsx`
Expected: PDF MIME already accepted (per design — no frontend change needed). If PDF is NOT accepted, add `application/pdf` to the accepted types and commit separately as `feat(web): accept PDF uploads on import`.

- [ ] **Step 2: Run the backend locally and ingest the real statement**

With `pdftotext` installed locally (`which pdftotext`), start the backend and POST the sample PDF through the existing `/ingest` endpoint (base64 the file, `media_type: "application/pdf"`). Use the project's normal dev run command.

- [ ] **Step 3: Verify review items appear**

`GET /review?status=pending` — expect one item per BCA mutation, `doc_type="bank_statement_bca"`, transfers flagged `needs_attention`, each payload carrying `cashflow_category` + `external_ref`.

- [ ] **Step 4: Confirm one deposit + one withdrawal**

Confirm a withdrawal (e.g. the QRIS Indomaret row) and a deposit (the `12/05 ... CR` transfer-in) against a Cash instrument + the BCA account. Then:
- `GET /cashflow` (or open BudgetPage) — expect matching rows: withdrawal → `direction=out`, deposit → `direction=in`, both `source=bank_statement_bca`, categories attached.
- Verify the portfolio txn was also created (existing ledger view).

- [ ] **Step 5: Verify idempotent re-upload**

Re-POST the same PDF and confirm the same row again (new review item, same `external_ref`). Expect NO duplicate cashflow row (the `ON CONFLICT(source, external_ref) DO NOTHING` guard holds). Note: the portfolio txn has no such guard — duplicate txns are still possible on double-confirm; this is acceptable for v1 and called out in the spec's out-of-scope notes.

- [ ] **Step 6: Final full-suite check**

Run: `cd backend && cargo test && cargo clippy --all-targets -- -D warnings`
Expected: all green, no clippy errors. Do NOT run `cargo fmt`.

---

## Self-Review Notes

- **Spec coverage:** PDF extraction (Task 1) · deterministic parser (Task 2) · categorization (Task 3) · ExtractedEntry mapping + dedup ref (Task 4) · ingest routing (Task 5) · confirm→cashflow+txn (Task 6) · poppler-utils deploy dep (Task 7) · anti-double-count via shared external_ref + transfer flag (Tasks 4 & 6) · review-queue flow reused (Tasks 5 & 8). All spec sections mapped.
- **Double-count handling:** one statement line → one txn + one cashflow row sharing `external_ref`; transfers flagged so reporting can exclude them. Cashflow dedup is enforced; txn dedup is explicitly out of scope for v1 (noted in Task 8 Step 5).
- **Type consistency:** `parse_statement` (mod.rs) ↔ `parse_mutations` (bca_parser) ↔ `categorize` (bca_category) ↔ `ExtractedEntry` fields `cashflow_category`/`external_ref` (extract.rs) ↔ `write_bank_cashflow` reading those fields (review.rs) all use consistent names. `NewCashflow`, `insert_sourced`, `ensure_by_name` signatures match repo definitions verified during planning.
- **No new crates:** extraction shells out to `pdftotext`; Cargo.lock untouched.
