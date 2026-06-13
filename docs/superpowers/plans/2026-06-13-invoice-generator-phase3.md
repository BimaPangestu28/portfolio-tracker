# Invoice Generator — Phase 3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make invoicing usable end-to-end from Telegram: "buatin invoice PT AIS: landing page 10jt, hosting 2jt" → the bot resolves/creates the client, numbers + formats + renders the invoice, sends the PDF back in chat, and persists it.

**Architecture:** `invoice::config` reads issuer/payment/due-days from env. A pure `invoice::assemble` turns parsed inputs (client + numeric line items + dates + config) into a display-ready `InvoiceData` (does the IDR formatting, totals, `terbilang`, Indonesian date). Two assistant tools — `list_clients` and `create_invoice` — wire it together; `create_invoice` resolves/creates the client, assembles, renders (Phase 2), persists, and sends the PDF via a Telegram client built from env + the linked owner chat.

**Tech Stack:** Rust, sqlx, chrono, rust_decimal, serde_json, anyhow. Binary crate `portfolio-tracker` — `cargo test --bin portfolio-tracker <filter>` from `backend/`. NEVER run `cargo fmt`. Typst builds are slow.

**Scope note:** Phase 3 (final) of `docs/superpowers/specs/2026-06-13-invoice-generator-design.md`. Builds on Phase 1 (terbilang, number, repos) + Phase 2 (model, render_pdf, send_document, document_filename).

---

## File Structure

- `backend/src/invoice/config.rs` — `InvoiceConfig`, `from_env()` (issuer/payment/due_days).
- `backend/src/invoice/assemble.rs` — `ParsedItem`, `format_idr`, `format_date_id`, `assemble_invoice_data(...)` (pure).
- `backend/src/invoice/mod.rs` — add `pub mod config;` + `pub mod assemble;`.
- `backend/src/assistant/tools.rs` — `list_clients` + `create_invoice` schemas + schema tests.
- `backend/src/assistant/dispatcher.rs` — `clickup`-style handlers `list_clients` + `create_invoice`.
- `backend/src/assistant/agent.rs` — `SYSTEM` prompt invoice guidance + test.
- `backend/.env.example` — document `INVOICE_*` vars.

All commands run from `backend/`.

---

### Task 1: `invoice::config` + pure formatting/assembly

**Files:**
- Create: `backend/src/invoice/config.rs`
- Create: `backend/src/invoice/assemble.rs`
- Modify: `backend/src/invoice/mod.rs`

- [ ] **Step 1: Declare modules**

In `backend/src/invoice/mod.rs` add `pub mod config;` and `pub mod assemble;` (keep the existing lines):

```rust
//! Invoice generation: data model, amount-in-words, numbering, rendering, config.
pub mod terbilang;
pub mod number;
pub mod model;
pub mod render;
pub mod config;
pub mod assemble;
```

- [ ] **Step 2: Write `config.rs` with a failing test**

Create `backend/src/invoice/config.rs`:

```rust
//! Issuer + payment details for invoices, from env (kept out of the repo —
//! public repo). `INVOICE_ISSUER_NAME` and `INVOICE_ACCOUNT_NO` are required;
//! the feature reports "belum dikonfigurasi" without them.

use crate::invoice::model::{Issuer, Payment};

pub struct InvoiceConfig {
    pub issuer: Issuer,
    pub payment: Payment,
    pub due_days: i64,
}

/// Read invoice config from env. Returns a human error (Indonesian) when the
/// required fields are missing, so the assistant can tell the owner.
pub fn from_env() -> Result<InvoiceConfig, String> {
    fn var(key: &str) -> String {
        std::env::var(key).unwrap_or_default()
    }
    let issuer_name = var("INVOICE_ISSUER_NAME");
    let account_no = var("INVOICE_ACCOUNT_NO");
    if issuer_name.trim().is_empty() || account_no.trim().is_empty() {
        return Err("invoice belum dikonfigurasi (set INVOICE_ISSUER_NAME, INVOICE_ACCOUNT_NO, dll di env)".into());
    }
    let due_days = var("INVOICE_DUE_DAYS").parse::<i64>().unwrap_or(14);
    Ok(InvoiceConfig {
        issuer: Issuer {
            name: issuer_name,
            company: var("INVOICE_ISSUER_COMPANY"),
            website: var("INVOICE_ISSUER_WEBSITE"),
            city: {
                let c = var("INVOICE_ISSUER_CITY");
                if c.is_empty() { "Jakarta".into() } else { c }
            },
        },
        payment: Payment {
            bank: var("INVOICE_BANK"),
            account_no,
            account_name: var("INVOICE_ACCOUNT_NAME"),
        },
        due_days,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_env_errors_without_issuer_name() {
        let prev_name = std::env::var("INVOICE_ISSUER_NAME").ok();
        let prev_acc = std::env::var("INVOICE_ACCOUNT_NO").ok();
        std::env::remove_var("INVOICE_ISSUER_NAME");
        std::env::remove_var("INVOICE_ACCOUNT_NO");
        let result = from_env();
        if let Some(v) = prev_name { std::env::set_var("INVOICE_ISSUER_NAME", v); }
        if let Some(v) = prev_acc { std::env::set_var("INVOICE_ACCOUNT_NO", v); }
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Run to verify it fails, then it passes**

Run: `cargo test --bin portfolio-tracker invoice::config 2>&1 | tail -10`
Expected: PASS (the impl is included above; this is the failing-then-passing combined — first ensure it compiles and the test passes).

- [ ] **Step 4: Write `assemble.rs` with failing tests**

Create `backend/src/invoice/assemble.rs`:

```rust
//! Pure assembly of display-ready `InvoiceData` from parsed inputs.

use crate::invoice::config::InvoiceConfig;
use crate::invoice::model::{ClientInfo, InvoiceData, LineItem};
use crate::repo::clients::ClientRow;
use rust_decimal::Decimal;

/// A line item as parsed from the tool input (numeric IDR).
pub struct ParsedItem {
    pub title: String,
    pub body: Option<String>,
    pub qty: i64,
    pub amount_idr: i64,
}

/// "Rp 12.000.000" from an integer rupiah amount.
pub fn format_idr(amount: i64) -> String {
    format!("Rp {}", crate::service::chat::group_id(&Decimal::from(amount)))
}

/// "11 Juni 2026" from a WIB calendar date.
pub fn format_date_id(date: chrono::NaiveDate) -> String {
    use chrono::Datelike;
    const MONTHS: [&str; 12] = [
        "Januari", "Februari", "Maret", "April", "Mei", "Juni",
        "Juli", "Agustus", "September", "Oktober", "November", "Desember",
    ];
    format!("{} {} {}", date.day(), MONTHS[(date.month() - 1) as usize], date.year())
}

/// Build display-ready `InvoiceData`. `total = subtotal = sum(amount)` (no PPN).
pub fn assemble_invoice_data(
    number: String,
    issue_date: chrono::NaiveDate,
    config: &InvoiceConfig,
    client: &ClientRow,
    items: &[ParsedItem],
) -> InvoiceData {
    let due = issue_date + chrono::Duration::days(config.due_days);
    let line_items: Vec<LineItem> = items
        .iter()
        .map(|it| {
            let qty = it.qty.max(1);
            LineItem {
                title: it.title.clone(),
                body: it.body.clone(),
                qty: qty.to_string(),
                unit_price: format_idr(it.amount_idr / qty),
                amount: format_idr(it.amount_idr),
            }
        })
        .collect();
    let total: i64 = items.iter().map(|it| it.amount_idr).sum();
    InvoiceData {
        number,
        issue_date: format_date_id(issue_date),
        due_date: format_date_id(due),
        issuer: config.issuer.clone(),
        client: ClientInfo {
            name: client.name.clone(),
            sub_name: client.sub_name.clone(),
            website: client.website.clone(),
        },
        payment: config.payment.clone(),
        line_items,
        subtotal: format_idr(total),
        total: format_idr(total),
        terbilang: crate::invoice::terbilang::terbilang(total),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::model::{Issuer, Payment};

    fn config() -> InvoiceConfig {
        InvoiceConfig {
            issuer: Issuer { name: "Bima".into(), company: "Catalyst".into(), website: "catalystlabs.id".into(), city: "Jakarta".into() },
            payment: Payment { bank: "BCA".into(), account_no: "123".into(), account_name: "Bima".into() },
            due_days: 14,
        }
    }
    fn client() -> ClientRow {
        ClientRow { id: 1, name: "PT AIS".into(), sub_name: Some("AIS Helicopter".into()), website: None, created_at: String::new() }
    }

    #[test]
    fn format_idr_groups_thousands() {
        assert_eq!(format_idr(12_000_000), "Rp 12.000.000");
        assert_eq!(format_idr(0), "Rp 0");
    }

    #[test]
    fn format_date_id_is_indonesian() {
        let d = chrono::NaiveDate::from_ymd_opt(2026, 6, 11).unwrap();
        assert_eq!(format_date_id(d), "11 Juni 2026");
    }

    #[test]
    fn assemble_totals_and_due_date() {
        let issue = chrono::NaiveDate::from_ymd_opt(2026, 6, 11).unwrap();
        let items = vec![
            ParsedItem { title: "Landing".into(), body: None, qty: 1, amount_idr: 10_000_000 },
            ParsedItem { title: "Hosting".into(), body: None, qty: 1, amount_idr: 2_000_000 },
        ];
        let data = assemble_invoice_data("INV/2026/VI/001".into(), issue, &config(), &client(), &items);
        assert_eq!(data.total, "Rp 12.000.000");
        assert_eq!(data.subtotal, "Rp 12.000.000");
        assert_eq!(data.terbilang, "Dua belas juta rupiah");
        assert_eq!(data.issue_date, "11 Juni 2026");
        assert_eq!(data.due_date, "25 Juni 2026");
        assert_eq!(data.client.name, "PT AIS");
        assert_eq!(data.line_items.len(), 2);
        assert_eq!(data.line_items[0].amount, "Rp 10.000.000");
    }

    #[test]
    fn assemble_unit_price_divides_by_qty() {
        let issue = chrono::NaiveDate::from_ymd_opt(2026, 6, 11).unwrap();
        let items = vec![ParsedItem { title: "Jam".into(), body: None, qty: 4, amount_idr: 4_000_000 }];
        let data = assemble_invoice_data("INV/2026/VI/002".into(), issue, &config(), &client(), &items);
        assert_eq!(data.line_items[0].unit_price, "Rp 1.000.000");
        assert_eq!(data.line_items[0].qty, "4");
    }
}
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test --bin portfolio-tracker invoice::assemble 2>&1 | tail -12`
Expected: PASS (all 4 tests).
Run: `cargo test --bin portfolio-tracker invoice:: 2>&1 | tail -4` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add src/invoice/
git commit -m "feat(invoice): add env config and pure InvoiceData assembly"
```

---

### Task 2: `list_clients` + `create_invoice` tools

**Files:**
- Modify: `backend/src/assistant/tools.rs`
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Add the schemas**

In `backend/src/assistant/tools.rs`, after the last tool object in `definitions()` (find it; append a comma to it):

```rust
{
    "name": "list_clients",
    "description": "List saved invoice clients (name). Use to reuse an existing client before create_invoice.",
    "input_schema": { "type": "object", "properties": {} }
},
{
    "name": "create_invoice",
    "description": "Create and send a freelance invoice PDF over Telegram. Dictate the client and line items. If the client is new, first ask for their details (sub_name/website) and pass client_details. Echo the parsed items + total to the user before calling so they can catch typos.",
    "input_schema": {
        "type": "object",
        "properties": {
            "client_name": { "type": "string", "description": "Client name, e.g. PT AIS" },
            "line_items": {
                "type": "array",
                "description": "Invoice lines",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string", "description": "Bold line, e.g. Pengembangan Landing Page" },
                        "body": { "type": "string", "description": "Optional description paragraph" },
                        "qty": { "type": "integer", "description": "Quantity, default 1" },
                        "amount": { "type": "integer", "description": "Line total in IDR (e.g. 10 juta -> 10000000)" }
                    },
                    "required": ["title", "amount"]
                }
            },
            "client_details": {
                "type": "object",
                "description": "Only when the client is new",
                "properties": {
                    "sub_name": { "type": "string" },
                    "website": { "type": "string" }
                }
            },
            "due_days": { "type": "integer", "description": "Override default due-in days" }
        },
        "required": ["client_name", "line_items"]
    }
}
```
Append `"list_clients"` and `"create_invoice"` to the `defines_all_tools_with_schemas` names vec (in that order, after the current last entry). Add to `required_fields_are_marked`:
`assert_eq!(find("create_invoice")["input_schema"]["required"], serde_json::json!(["client_name", "line_items"]));`

- [ ] **Step 2: Write failing dispatcher tests**

In `backend/src/assistant/dispatcher.rs` test module:

```rust
#[tokio::test]
async fn list_clients_lists_saved() {
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    crate::repo::clients::create(&db, &crate::repo::clients::NewClient { name: "PT AIS", sub_name: None, website: None }).await.unwrap();
    let out = invoice_list_clients(&db).await.unwrap();
    assert!(out.contains("PT AIS"), "{out}");
}

#[tokio::test]
async fn create_invoice_persists_and_reports_number() {
    // Issuer config required; set it for the test, restore after.
    std::env::set_var("INVOICE_ISSUER_NAME", "Bima");
    std::env::set_var("INVOICE_ACCOUNT_NO", "123");
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    crate::repo::clients::create(&db, &crate::repo::clients::NewClient { name: "PT AIS", sub_name: None, website: None }).await.unwrap();
    let out = invoice_create(&db, &serde_json::json!({
        "client_name": "PT AIS",
        "line_items": [{ "title": "Landing page", "amount": 10_000_000 }]
    })).await.unwrap();
    std::env::remove_var("INVOICE_ISSUER_NAME");
    std::env::remove_var("INVOICE_ACCOUNT_NO");
    assert!(out.contains("INV/"), "should report the invoice number: {out}");
    // Persisted.
    let seq = crate::repo::invoices::max_seq_for_prefix(&db, "INV/").await.unwrap();
    assert!(seq.is_some(), "invoice not persisted");
}

#[tokio::test]
async fn create_invoice_unknown_client_asks_for_details() {
    std::env::set_var("INVOICE_ISSUER_NAME", "Bima");
    std::env::set_var("INVOICE_ACCOUNT_NO", "123");
    let db = crate::db::connect("sqlite::memory:").await.unwrap();
    let err = invoice_create(&db, &serde_json::json!({
        "client_name": "Klien Baru",
        "line_items": [{ "title": "x", "amount": 1000 }]
    })).await.unwrap_err();
    std::env::remove_var("INVOICE_ISSUER_NAME");
    std::env::remove_var("INVOICE_ACCOUNT_NO");
    assert!(err.contains("Klien Baru"), "{err}");
    assert!(err.contains("detail") || err.contains("belum ada"), "{err}");
}
```
Handler names: `invoice_list_clients(db)` and `invoice_create(db, input)`.

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_invoice 2>&1 | tail -15`
Expected: FAIL (`invoice_create` undefined).

- [ ] **Step 4: Implement handlers + dispatch arms**

In `backend/src/assistant/dispatcher.rs` add:

```rust
async fn invoice_list_clients(db: &Db) -> Result<String, String> {
    let clients = crate::repo::clients::list(db).await.map_err(|e| format!("db error: {e}"))?;
    if clients.is_empty() {
        return Ok("belum ada klien tersimpan".into());
    }
    let mut out = String::new();
    for c in clients {
        out.push_str(&format!("#{} {}\n", c.id, c.name));
    }
    Ok(out)
}

fn parse_line_items(input: &serde_json::Value) -> Result<Vec<crate::invoice::assemble::ParsedItem>, String> {
    let arr = input.get("line_items").and_then(|v| v.as_array())
        .ok_or("line_items harus berupa array")?;
    if arr.is_empty() {
        return Err("line_items kosong".into());
    }
    let mut items = Vec::new();
    for it in arr {
        let title = it.get("title").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty())
            .ok_or("setiap item butuh 'title'")?;
        let amount = it.get("amount").and_then(|v| v.as_i64())
            .ok_or("setiap item butuh 'amount' (angka IDR)")?;
        let qty = it.get("qty").and_then(|v| v.as_i64()).unwrap_or(1);
        let body = it.get("body").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()).map(|s| s.to_string());
        items.push(crate::invoice::assemble::ParsedItem { title: title.to_string(), body, qty, amount_idr: amount });
    }
    Ok(items)
}

async fn invoice_create(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let config = crate::invoice::config::from_env()?;
    let client_name = str_arg(input, "client_name").ok_or("missing required argument 'client_name'")?;
    let items = parse_line_items(input)?;

    // Resolve or create the client.
    let client = match crate::repo::clients::get_by_name(db, client_name).await.map_err(|e| format!("db error: {e}"))? {
        Some(c) => c,
        None => {
            let details = input.get("client_details");
            let sub = details.and_then(|d| d.get("sub_name")).and_then(|v| v.as_str());
            let web = details.and_then(|d| d.get("website")).and_then(|v| v.as_str());
            if details.is_none() {
                return Err(format!("klien '{client_name}' belum ada — minta detail klien (sub_name/website) ke user dulu, lalu kirim lewat client_details"));
            }
            crate::repo::clients::create(db, &crate::repo::clients::NewClient { name: client_name, sub_name: sub, website: web })
                .await.map_err(|e| format!("db error: {e}"))?
        }
    };

    // Number + assemble + render.
    let now = chrono::Utc::now();
    let number = crate::invoice::number::next_number(db, now).await.map_err(|e| format!("db error: {e}"))?;
    let issue_date = now.with_timezone(&crate::assistant::time::wib()).date_naive();
    let data = crate::invoice::assemble::assemble_invoice_data(number.clone(), issue_date, &config, &client, &items);
    let pdf = crate::invoice::render::render_pdf(&data).map_err(|e| format!("gagal render invoice: {e}"))?;

    // Persist (line items as JSON of {title, body, qty, amount}).
    let line_items_json = serde_json::to_string(
        &items.iter().map(|it| serde_json::json!({ "title": it.title, "body": it.body, "qty": it.qty, "amount": it.amount_idr })).collect::<Vec<_>>()
    ).unwrap_or_else(|_| "[]".into());
    crate::repo::invoices::insert(db, &crate::repo::invoices::NewInvoice {
        number: &number, client_id: client.id,
        issue_date: &data.issue_date, due_date: &data.due_date,
        subtotal: &data.subtotal, total: &data.total, line_items_json: &line_items_json,
    }).await.map_err(|e| format!("db error: {e}"))?;

    // Send the PDF to the linked owner chat (best-effort).
    let sent = send_invoice_pdf(db, &number, pdf).await;
    let suffix = match sent {
        Ok(true) => " dan dikirim ke Telegram".to_string(),
        Ok(false) => " (tersimpan; Telegram belum tertaut, jadi PDF tidak dikirim)".to_string(),
        Err(e) => format!(" (tersimpan, tapi gagal kirim PDF: {e})"),
    };
    Ok(format!("Invoice {number} dibuat — total {}{suffix}", data.total))
}

/// Send the rendered PDF to the linked owner chat. Ok(false) = no link/token.
async fn send_invoice_pdf(db: &Db, number: &str, pdf: Vec<u8>) -> Result<bool, String> {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else { return Ok(false); };
    if token.trim().is_empty() { return Ok(false); }
    let Some(link) = crate::repo::telegram_link::get(db).await.map_err(|e| format!("db error: {e}"))? else { return Ok(false); };
    let client = crate::telegram::client::TelegramClient::new(token);
    let filename = crate::telegram::client::document_filename(number);
    client.send_document(link.chat_id, &filename, pdf, &format!("Invoice {number}"))
        .await.map_err(|e| format!("{e}"))?;
    Ok(true)
}
```

Add dispatch arms in the `match name` block, after the last existing arm:

```rust
"list_clients" => invoice_list_clients(db).await,
"create_invoice" => invoice_create(db, input).await,
```

- [ ] **Step 5: Run to verify they pass**

Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::create_invoice 2>&1 | tail -15`
Expected: PASS (persist + unknown-client tests).
Run: `cargo test --bin portfolio-tracker assistant::dispatcher::tests::list_clients 2>&1 | tail -8` → PASS.
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add src/assistant/tools.rs src/assistant/dispatcher.rs
git commit -m "feat(invoice): add list_clients and create_invoice assistant tools"
```

---

### Task 3: System prompt + env docs + schema tests

**Files:**
- Modify: `backend/src/assistant/agent.rs`
- Modify: `backend/.env.example`

- [ ] **Step 1: Failing prompt test**

In `backend/src/assistant/agent.rs` test module:

```rust
#[test]
fn system_prompt_mentions_invoicing() {
    let prompt = system_prompt("2026-06-13T10:00:00+07:00");
    assert!(prompt.contains("create_invoice"), "{prompt}");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_invoicing 2>&1 | tail -10`
Expected: FAIL.

- [ ] **Step 3: Extend the `SYSTEM` const**

Append to the END of the `SYSTEM` literal (keep existing text; ` \` continuation; last line flows into `";`):

```
 You can also make invoices: when the owner says e.g. 'buatin invoice PT AIS: landing page 10 juta, hosting 2 juta', parse each line into {title, amount in IDR} and call create_invoice with client_name + line_items. Convert '10 juta' to 10000000. First echo the parsed items and the total back to the owner so they can catch typos. If create_invoice reports the client 'belum ada', ask the owner for the client's sub-name/website and retry with client_details. The PDF is sent to Telegram automatically.
```

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker assistant::agent::tests::system_prompt_mentions_invoicing 2>&1 | tail -10`
Expected: PASS.

- [ ] **Step 5: Document env vars**

In `backend/.env.example`, append a section (at the end of the file):

```
# --- Invoicing (optional; INVOICE_ISSUER_NAME + INVOICE_ACCOUNT_NO required to enable) ---
INVOICE_ISSUER_NAME=
INVOICE_ISSUER_COMPANY=
INVOICE_ISSUER_WEBSITE=
INVOICE_ISSUER_CITY=Jakarta
INVOICE_BANK=
INVOICE_ACCOUNT_NO=
INVOICE_ACCOUNT_NAME=
# Days until due (default 14).
INVOICE_DUE_DAYS=14
```

- [ ] **Step 6: Full suite + build**

Run: `cargo test --bin portfolio-tracker 2>&1 | tail -8` → report counts, 0 failed.
Run: `cargo build --bin portfolio-tracker 2>&1 | grep -c "^warning"` → report (the invoice modules are now all reachable; aim for few/zero new warnings — the only acceptable leftovers are any pre-existing ones).

- [ ] **Step 7: Commit**

```bash
git add src/assistant/agent.rs .env.example
git commit -m "feat(invoice): teach assistant to create invoices; document env"
```

---

## Self-Review Notes

- **Spec coverage (Phase 3):** env config → Task 1; IDR/date formatting + assembly → Task 1; `list_clients`/`create_invoice` (resolve/create client, number, render, persist, send) → Task 2; offer-to-collect-details for new clients → Task 2 handler; prompt + env docs → Task 3.
- **Type consistency:** `InvoiceConfig{issuer,payment,due_days}`; `assemble_invoice_data(number, NaiveDate, &InvoiceConfig, &ClientRow, &[ParsedItem]) -> InvoiceData`; `ParsedItem{title,body,qty,amount_idr}`; handlers `invoice_list_clients(db)`, `invoice_create(db,input)`, helper `send_invoice_pdf`. Reuses Phase 1/2: `terbilang`, `next_number`, `render_pdf`, `document_filename`, `send_document`, repos `clients`/`invoices`, `telegram_link::get`, `service::chat::group_id`.
- **Public-repo hygiene:** issuer/bank come from env only; `.env.example` has placeholders, no real values.
- **Send is best-effort:** the invoice always persists; a missing token/link or send failure is reported in the result string, never a panic. The handler's persist+number is tested with an in-memory db (render runs real Typst — slow).
- **No placeholders:** all handler/test names are final (`invoice_list_clients`, `invoice_create`, `send_invoice_pdf`).
