# Invoice Generator — Phase 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the pure, fully-testable foundation for invoice generation: Indonesian amount-in-words (`terbilang`), per-month invoice numbering, and the `client` + `invoice` persistence — with no external dependencies.

**Architecture:** A new `invoice` module holds `terbilang` (pure number→words) and `number` (roman month + per-month sequence). Two repos (`repo::clients`, `repo::invoices`) back a new migration. The numbering's DB-touching part is a thin wrapper over a pure `compute_number`, so almost everything is unit-tested without I/O.

**Tech Stack:** Rust, sqlx (SQLite), chrono, anyhow. Binary crate `portfolio-tracker` — run tests with `cargo test --bin portfolio-tracker <filter>` from `backend/`. NEVER run `cargo fmt` (rewrites ~600 files).

**Scope note:** Phase 1 of `docs/superpowers/specs/2026-06-13-invoice-generator-design.md`. Phase 2 (Typst render + `send_document`) and Phase 3 (tools + config + wiring) follow. This phase ships a tested data/logic foundation, no user-facing behavior yet.

**⚠️ Migration number:** the plan uses `0016`. Before creating it, run `ls backend/migrations | sort | tail -3` AND check `git fetch && git log origin/main --oneline -5 -- backend/migrations` — concurrent PRs add migrations, and sqlx rejects duplicate numbers. If `0016` is taken, use the next free number consistently.

---

## File Structure

- `backend/migrations/0016_invoices.sql` — `client` + `invoice` tables.
- `backend/src/invoice/mod.rs` — module root (`pub mod terbilang; pub mod number;`).
- `backend/src/invoice/terbilang.rs` — `terbilang(i64) -> String` + private `spell`.
- `backend/src/invoice/number.rs` — `roman_month`, `compute_number` (pure), `next_number(db, now)`.
- `backend/src/repo/clients.rs` — `ClientRow`, `NewClient`, `create`/`get_by_name`/`list`.
- `backend/src/repo/invoices.rs` — `InvoiceRow`, `NewInvoice`, `insert`, `max_seq_for_prefix`.
- `backend/src/main.rs` — add `mod invoice;`.
- `backend/src/repo/mod.rs` — add `pub mod clients;` + `pub mod invoices;`.

All commands run from `backend/`.

---

### Task 1: `terbilang` — Indonesian amount in words

**Files:**
- Create: `backend/src/invoice/mod.rs`
- Create: `backend/src/invoice/terbilang.rs`
- Modify: `backend/src/main.rs`

- [ ] **Step 1: Declare the module**

In `backend/src/main.rs`, add `mod invoice;` alongside the other top-level `mod` declarations (run `grep -n "^mod " src/main.rs` to place it consistently).

Create `backend/src/invoice/mod.rs`:

```rust
//! Invoice generation: amount-in-words and per-month numbering (Phase 1);
//! model + Typst rendering and the assistant tools come in later phases.
pub mod terbilang;
pub mod number;
```
(Note: `number` is created in Task 3. To keep this task compiling on its own, TEMPORARILY include only `pub mod terbilang;` now and add `pub mod number;` in Task 3. Do that — declare just `terbilang` here.)

So `mod.rs` for Task 1 is:

```rust
//! Invoice generation: amount-in-words and per-month numbering (Phase 1).
pub mod terbilang;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/invoice/terbilang.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spells_the_template_and_boundaries() {
        assert_eq!(terbilang(0), "Nol rupiah");
        assert_eq!(terbilang(11), "Sebelas rupiah");
        assert_eq!(terbilang(12), "Dua belas rupiah");
        assert_eq!(terbilang(21), "Dua puluh satu rupiah");
        assert_eq!(terbilang(100), "Seratus rupiah");
        assert_eq!(terbilang(1000), "Seribu rupiah");
        assert_eq!(terbilang(2500), "Dua ribu lima ratus rupiah");
        assert_eq!(terbilang(12_000_000), "Dua belas juta rupiah");
        assert_eq!(
            terbilang(1_234_567),
            "Satu juta dua ratus tiga puluh empat ribu lima ratus enam puluh tujuh rupiah"
        );
        assert_eq!(terbilang(2_000_000_000), "Dua miliar rupiah");
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker invoice::terbilang 2>&1 | tail -15`
Expected: FAIL to compile (`terbilang` not defined).

- [ ] **Step 4: Implement**

Prepend to `backend/src/invoice/terbilang.rs` (above the tests):

```rust
//! Indonesian rupiah amount in words, e.g. 12_000_000 -> "Dua belas juta rupiah".

const UNITS: [&str; 12] = [
    "nol", "satu", "dua", "tiga", "empat", "lima", "enam", "tujuh", "delapan",
    "sembilan", "sepuluh", "sebelas",
];

/// Lowercase words for a non-negative integer (no "rupiah").
fn spell(n: u64) -> String {
    match n {
        0..=11 => UNITS[n as usize].to_string(),
        12..=19 => format!("{} belas", UNITS[(n - 10) as usize]),
        20..=99 => {
            let tens = format!("{} puluh", UNITS[(n / 10) as usize]);
            if n % 10 == 0 { tens } else { format!("{tens} {}", UNITS[(n % 10) as usize]) }
        }
        100..=199 => {
            let rest = n % 100;
            if rest == 0 { "seratus".into() } else { format!("seratus {}", spell(rest)) }
        }
        200..=999 => {
            let hundreds = format!("{} ratus", UNITS[(n / 100) as usize]);
            let rest = n % 100;
            if rest == 0 { hundreds } else { format!("{hundreds} {}", spell(rest)) }
        }
        1000..=1999 => {
            let rest = n % 1000;
            if rest == 0 { "seribu".into() } else { format!("seribu {}", spell(rest)) }
        }
        2000..=999_999 => group(n, 1000, "ribu"),
        1_000_000..=999_999_999 => group(n, 1_000_000, "juta"),
        1_000_000_000..=999_999_999_999 => group(n, 1_000_000_000, "miliar"),
        _ => group(n, 1_000_000_000_000, "triliun"),
    }
}

/// "<spell(n/scale)> <unit> [spell(rest)]" for the thousands/millions/... groups.
fn group(n: u64, scale: u64, unit: &str) -> String {
    let head = format!("{} {unit}", spell(n / scale));
    let rest = n % scale;
    if rest == 0 { head } else { format!("{head} {}", spell(rest)) }
}

/// Capitalized amount in words with the "rupiah" suffix. Negatives clamp to 0.
pub fn terbilang(amount: i64) -> String {
    let words = spell(amount.max(0) as u64);
    let mut chars = words.chars();
    let capitalized = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => words,
    };
    format!("{capitalized} rupiah")
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker invoice::terbilang 2>&1 | tail -15`
Expected: PASS.
Run: `cargo build --bin portfolio-tracker 2>&1 | tail -5` (builds; `terbilang` unused warning is fine until later phases use it).

- [ ] **Step 6: Commit**

```bash
git add backend/src/invoice/ backend/src/main.rs
git commit -m "feat(invoice): add terbilang (rupiah amount in words)"
```

---

### Task 2: Migration + `client` and `invoice` repos

**Files:**
- Create: `backend/migrations/0016_invoices.sql` (verify the number first — see warning above)
- Create: `backend/src/repo/clients.rs`
- Create: `backend/src/repo/invoices.rs`
- Modify: `backend/src/repo/mod.rs`

- [ ] **Step 1: Create the migration**

First confirm the number is free: `ls migrations | sort | tail -3`. If `0016` is taken, use the next free number for the filename (keep the SQL identical).

Create `backend/migrations/0016_invoices.sql`:

```sql
-- Freelance invoicing: reusable client details + write-once invoices.
CREATE TABLE client (
  id         INTEGER PRIMARY KEY,
  name       TEXT NOT NULL UNIQUE COLLATE NOCASE,
  sub_name   TEXT,
  website    TEXT,
  created_at TEXT NOT NULL
);

CREATE TABLE invoice (
  id              INTEGER PRIMARY KEY,
  number          TEXT NOT NULL UNIQUE,
  client_id       INTEGER NOT NULL REFERENCES client(id),
  issue_date      TEXT NOT NULL,
  due_date        TEXT NOT NULL,
  subtotal        TEXT NOT NULL,
  total           TEXT NOT NULL,
  line_items_json TEXT NOT NULL,
  created_at      TEXT NOT NULL
);
```

- [ ] **Step 2: Register the repos**

In `backend/src/repo/mod.rs`, add (alphabetically near the others):

```rust
pub mod clients;
pub mod invoices;
```

- [ ] **Step 3: Write the failing client-repo test**

Create `backend/src/repo/clients.rs`:

```rust
//! Persistence for invoice clients (see migration 0016).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ClientRow {
    pub id: i64,
    pub name: String,
    pub sub_name: Option<String>,
    pub website: Option<String>,
    pub created_at: String,
}

pub struct NewClient<'a> {
    pub name: &'a str,
    pub sub_name: Option<&'a str>,
    pub website: Option<&'a str>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn create_then_get_by_name_is_case_insensitive() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let made = create(&db, &NewClient { name: "PT AIS", sub_name: Some("AIS Helicopter"), website: Some("www.aishelicopter.com") }).await.unwrap();
        assert_eq!(made.name, "PT AIS");
        let found = get_by_name(&db, "pt ais").await.unwrap().expect("case-insensitive match");
        assert_eq!(found.id, made.id);
        assert_eq!(found.sub_name.as_deref(), Some("AIS Helicopter"));
        assert!(get_by_name(&db, "Unknown Co").await.unwrap().is_none());
        let all = list(&db).await.unwrap();
        assert_eq!(all.len(), 1);
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker repo::clients 2>&1 | tail -15`
Expected: FAIL (`create`/`get_by_name`/`list` not defined).

- [ ] **Step 5: Implement the client repo**

Add to `backend/src/repo/clients.rs` (above the test module):

```rust
pub async fn create(db: &Db, c: &NewClient<'_>) -> anyhow::Result<ClientRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO client (name, sub_name, website, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(c.name).bind(c.sub_name).bind(c.website).bind(&now)
    .execute(db).await?.last_insert_rowid();
    get(db, id).await
}

pub async fn get(db: &Db, id: i64) -> anyhow::Result<ClientRow> {
    Ok(sqlx::query_as::<_, ClientRow>("SELECT * FROM client WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

pub async fn get_by_name(db: &Db, name: &str) -> anyhow::Result<Option<ClientRow>> {
    Ok(sqlx::query_as::<_, ClientRow>("SELECT * FROM client WHERE name = ? COLLATE NOCASE LIMIT 1")
        .bind(name).fetch_optional(db).await?)
}

pub async fn list(db: &Db) -> anyhow::Result<Vec<ClientRow>> {
    Ok(sqlx::query_as::<_, ClientRow>("SELECT * FROM client ORDER BY name")
        .fetch_all(db).await?)
}
```

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker repo::clients 2>&1 | tail -15`
Expected: PASS.

- [ ] **Step 7: Write the failing invoice-repo test**

Create `backend/src/repo/invoices.rs`:

```rust
//! Persistence for generated invoices (see migration 0016). Write-once: line
//! items are stored as JSON, not a separate table.

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct InvoiceRow {
    pub id: i64,
    pub number: String,
    pub client_id: i64,
    pub issue_date: String,
    pub due_date: String,
    pub subtotal: String,
    pub total: String,
    pub line_items_json: String,
    pub created_at: String,
}

pub struct NewInvoice<'a> {
    pub number: &'a str,
    pub client_id: i64,
    pub issue_date: &'a str,
    pub due_date: &'a str,
    pub subtotal: &'a str,
    pub total: &'a str,
    pub line_items_json: &'a str,
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn seed_client(db: &Db) -> i64 {
        crate::repo::clients::create(db, &crate::repo::clients::NewClient {
            name: "PT AIS", sub_name: None, website: None,
        }).await.unwrap().id
    }

    #[tokio::test]
    async fn insert_then_max_seq_tracks_the_month_prefix() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let client_id = seed_client(&db).await;
        assert_eq!(max_seq_for_prefix(&db, "INV/2026/VI/").await.unwrap(), None);

        insert(&db, &NewInvoice {
            number: "INV/2026/VI/001", client_id, issue_date: "2026-06-11", due_date: "2026-06-25",
            subtotal: "12000000", total: "12000000", line_items_json: "[]",
        }).await.unwrap();
        insert(&db, &NewInvoice {
            number: "INV/2026/VI/002", client_id, issue_date: "2026-06-12", due_date: "2026-06-26",
            subtotal: "2000000", total: "2000000", line_items_json: "[]",
        }).await.unwrap();
        // A different month must not affect June's max.
        insert(&db, &NewInvoice {
            number: "INV/2026/VII/001", client_id, issue_date: "2026-07-01", due_date: "2026-07-15",
            subtotal: "500000", total: "500000", line_items_json: "[]",
        }).await.unwrap();

        assert_eq!(max_seq_for_prefix(&db, "INV/2026/VI/").await.unwrap(), Some(2));
        assert_eq!(max_seq_for_prefix(&db, "INV/2026/VII/").await.unwrap(), Some(1));
    }
}
```

- [ ] **Step 8: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker repo::invoices 2>&1 | tail -15`
Expected: FAIL (`insert`/`max_seq_for_prefix` not defined).

- [ ] **Step 9: Implement the invoice repo**

Add to `backend/src/repo/invoices.rs` (above the test module):

```rust
pub async fn insert(db: &Db, inv: &NewInvoice<'_>) -> anyhow::Result<InvoiceRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO invoice (number, client_id, issue_date, due_date, subtotal, total, line_items_json, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(inv.number).bind(inv.client_id).bind(inv.issue_date).bind(inv.due_date)
    .bind(inv.subtotal).bind(inv.total).bind(inv.line_items_json).bind(&now)
    .execute(db).await?.last_insert_rowid();
    Ok(sqlx::query_as::<_, InvoiceRow>("SELECT * FROM invoice WHERE id = ?")
        .bind(id).fetch_one(db).await?)
}

/// Highest NNN among invoices whose number starts with `prefix`
/// (e.g. "INV/2026/VI/"); None when the month has no invoices yet.
pub async fn max_seq_for_prefix(db: &Db, prefix: &str) -> anyhow::Result<Option<u32>> {
    let pattern = format!("{}%", prefix.replace('%', "\\%"));
    let numbers: Vec<(String,)> =
        sqlx::query_as("SELECT number FROM invoice WHERE number LIKE ?")
            .bind(&pattern).fetch_all(db).await?;
    let max = numbers
        .iter()
        .filter_map(|(n,)| n.rsplit('/').next())   // the "NNN" tail
        .filter_map(|tail| tail.parse::<u32>().ok())
        .max();
    Ok(max)
}
```

- [ ] **Step 10: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker repo::invoices 2>&1 | tail -15`
Expected: PASS.
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 11: Commit**

```bash
git add backend/migrations/ backend/src/repo/
git commit -m "feat(invoice): add client + invoice tables and repos"
```

---

### Task 3: `invoice::number` — roman month + per-month sequence

**Files:**
- Modify: `backend/src/invoice/mod.rs`
- Create: `backend/src/invoice/number.rs`

- [ ] **Step 1: Register the submodule**

In `backend/src/invoice/mod.rs`, add `pub mod number;` (so it reads):

```rust
//! Invoice generation: amount-in-words and per-month numbering (Phase 1).
pub mod terbilang;
pub mod number;
```

- [ ] **Step 2: Write the failing tests**

Create `backend/src/invoice/number.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn roman_months_cover_all_twelve() {
        assert_eq!(roman_month(1), "I");
        assert_eq!(roman_month(6), "VI");
        assert_eq!(roman_month(7), "VII");
        assert_eq!(roman_month(12), "XII");
    }

    #[test]
    fn compute_number_resets_per_month() {
        assert_eq!(compute_number(2026, 6, None), "INV/2026/VI/001");
        assert_eq!(compute_number(2026, 6, Some(2)), "INV/2026/VI/003");
        assert_eq!(compute_number(2026, 7, None), "INV/2026/VII/001");
    }

    #[tokio::test]
    async fn next_number_uses_the_wib_month_and_db_state() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        // 2026-06-30 20:00 UTC == 2026-07-01 03:00 WIB → July, not June.
        let now = chrono::Utc.with_ymd_and_hms(2026, 6, 30, 20, 0, 0).unwrap();
        assert_eq!(next_number(&db, now).await.unwrap(), "INV/2026/VII/001");
    }
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker invoice::number 2>&1 | tail -15`
Expected: FAIL (`roman_month`/`compute_number`/`next_number` not defined).

- [ ] **Step 4: Implement**

Add to `backend/src/invoice/number.rs` (above the tests):

```rust
//! Invoice numbers: `INV/<year>/<roman-month>/<NNN>`, NNN reset per month (WIB).

use crate::db::Db;
use chrono::{DateTime, Datelike, Utc};

pub fn roman_month(month: u32) -> &'static str {
    const ROMAN: [&str; 12] = [
        "I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII",
    ];
    ROMAN[(month.clamp(1, 12) - 1) as usize]
}

/// Pure: format the number for a given year/month and the highest existing
/// sequence that month (None when it's the first).
pub fn compute_number(year: i32, month: u32, last_seq: Option<u32>) -> String {
    let seq = last_seq.unwrap_or(0) + 1;
    format!("INV/{year}/{}/{seq:03}", roman_month(month))
}

/// Next invoice number for `now` (interpreted in WIB), reading the month's
/// current max sequence from the DB.
pub async fn next_number(db: &Db, now: DateTime<Utc>) -> anyhow::Result<String> {
    let wib = now.with_timezone(&crate::assistant::time::wib());
    let (year, month) = (wib.year(), wib.month());
    let prefix = format!("INV/{year}/{}/", roman_month(month));
    let last_seq = crate::repo::invoices::max_seq_for_prefix(db, &prefix).await?;
    Ok(compute_number(year, month, last_seq))
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker invoice::number 2>&1 | tail -15`
Expected: PASS.
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.
Build: `cargo build --bin portfolio-tracker 2>&1 | grep -c "^warning"` (report count; `terbilang`/number/repo unused warnings are expected until later phases — do NOT add `#[allow]`).

- [ ] **Step 6: Commit**

```bash
git add backend/src/invoice/
git commit -m "feat(invoice): add per-month invoice numbering"
```

---

## Self-Review Notes

- **Spec coverage (Phase 1):** terbilang → Task 1; client + invoice schema/repos → Task 2; numbering (roman + per-month reset, WIB) → Task 3. Render/model/send_document/tools/config are Phases 2-3.
- **Type consistency:** `terbilang(i64) -> String`; `roman_month(u32) -> &'static str`; `compute_number(i32, u32, Option<u32>) -> String`; `next_number(db, DateTime<Utc>) -> Result<String>`; `max_seq_for_prefix(db, &str) -> Result<Option<u32>>` consumed by `next_number`; `NewClient`/`ClientRow`, `NewInvoice`/`InvoiceRow` field names match the migration columns. `crate::assistant::time::wib()` reused.
- **Numbering coupling:** `next_number` builds the prefix `INV/<year>/<roman>/` and `max_seq_for_prefix` parses the `/NNN` tail — the same format `compute_number` emits. Verified consistent.
- **Known expected warnings:** Phase 1 code is unused until Phases 2-3; warnings are acceptable here (don't suppress).
- **Migration-number risk** is called out at the top; recheck before creating the file.
