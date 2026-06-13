# Invoice Generator — Phase 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn an `InvoiceData` value into a PDF that looks like the Catalyst Labs invoice template, and add the ability to send a file (the PDF) over Telegram.

**Architecture:** `invoice::model` holds display-ready data (the handler in Phase 3 does all formatting/terbilang). `invoice::render` builds a Typst source string from the data — a pure, unit-tested `build_typ` plus an `escape_typst` helper — then compiles it to PDF bytes via the verified `typst-as-lib` + `typst-assets` (bundled Libertinus Serif) toolchain. `telegram::client::send_document` uploads the PDF.

**Tech Stack:** Rust, `typst-as-lib` 0.15, `typst-pdf`/`typst-assets` 0.14 (already added in `Cargo.toml`), reqwest multipart, anyhow. Binary crate `portfolio-tracker` — `cargo test --bin portfolio-tracker <filter>` from `backend/`. NEVER run `cargo fmt`. NOTE: builds are slow now (Typst) — allow up to 10 min per compile.

**Scope note:** Phase 2 of `docs/superpowers/specs/2026-06-13-invoice-generator-design.md`. Builds on Phase 1 (terbilang, numbering, repos). Phase 3 (the `create_invoice`/`list_clients` tools, env config, prompt, wiring) follows and is what makes it user-facing. The Typst toolchain was de-risked with a spike (renders `%PDF` with bundled fonts).

---

## File Structure

- `backend/src/invoice/model.rs` — `InvoiceData`, `LineItem`, `ClientInfo`, `Issuer`, `Payment` (display-ready strings).
- `backend/src/invoice/render.rs` — `escape_typst`, `build_typ(&InvoiceData) -> String` (pure), `render_pdf(&InvoiceData) -> anyhow::Result<Vec<u8>>`.
- `backend/src/invoice/mod.rs` — add `pub mod model;` + `pub mod render;`.
- `backend/src/telegram/client.rs` — `send_document`.

All commands run from `backend/`.

---

### Task 1: Invoice data model + Typst source builder

**Files:**
- Create: `backend/src/invoice/model.rs`
- Create: `backend/src/invoice/render.rs`
- Modify: `backend/src/invoice/mod.rs`

- [ ] **Step 1: Declare the modules**

In `backend/src/invoice/mod.rs`:

```rust
//! Invoice generation: data model, amount-in-words, numbering, and rendering.
pub mod terbilang;
pub mod number;
pub mod model;
pub mod render;
```

- [ ] **Step 2: Write the model**

Create `backend/src/invoice/model.rs` (all fields are display-ready; the Phase 3 handler formats IDR and computes terbilang):

```rust
//! Display-ready invoice data. The handler formats money (e.g. "Rp 12.000.000")
//! and computes `terbilang` before building this; rendering just lays it out.

#[derive(Debug, Clone)]
pub struct Issuer {
    pub name: String,
    pub company: String,
    pub website: String,
    pub city: String,
}

#[derive(Debug, Clone)]
pub struct Payment {
    pub bank: String,
    pub account_no: String,
    pub account_name: String,
}

#[derive(Debug, Clone)]
pub struct ClientInfo {
    pub name: String,
    pub sub_name: Option<String>,
    pub website: Option<String>,
}

#[derive(Debug, Clone)]
pub struct LineItem {
    pub title: String,
    pub body: Option<String>,
    pub qty: String,
    pub unit_price: String, // "Rp 10.000.000"
    pub amount: String,     // "Rp 10.000.000"
}

#[derive(Debug, Clone)]
pub struct InvoiceData {
    pub number: String,
    pub issue_date: String, // "11 Juni 2026"
    pub due_date: String,
    pub issuer: Issuer,
    pub client: ClientInfo,
    pub payment: Payment,
    pub line_items: Vec<LineItem>,
    pub subtotal: String,
    pub total: String,
    pub terbilang: String,
}
```

- [ ] **Step 3: Write the failing tests for `escape_typst` + `build_typ`**

Create `backend/src/invoice/render.rs` with the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::model::*;

    fn sample() -> InvoiceData {
        InvoiceData {
            number: "INV/2026/VI/001".into(),
            issue_date: "11 Juni 2026".into(),
            due_date: "25 Juni 2026".into(),
            issuer: Issuer { name: "Bima Pangestu".into(), company: "Catalyst Labs".into(), website: "catalystlabs.id".into(), city: "Jakarta".into() },
            client: ClientInfo { name: "PT AIS".into(), sub_name: Some("AIS Helicopter".into()), website: Some("www.aishelicopter.com".into()) },
            payment: Payment { bank: "BCA".into(), account_no: "8415525237".into(), account_name: "Bima Pangestu".into() },
            line_items: vec![
                LineItem { title: "Pengembangan Landing Page Website".into(), body: Some("Desain & frontend + backend pendukung.".into()), qty: "1".into(), unit_price: "Rp 10.000.000".into(), amount: "Rp 10.000.000".into() },
                LineItem { title: "Hosting, Domain & Maintenance".into(), body: None, qty: "1".into(), unit_price: "Rp 2.000.000".into(), amount: "Rp 2.000.000".into() },
            ],
            subtotal: "Rp 12.000.000".into(),
            total: "Rp 12.000.000".into(),
            terbilang: "Dua belas juta rupiah".into(),
        }
    }

    #[test]
    fn escape_typst_neutralizes_markup() {
        assert_eq!(escape_typst("a#b*c_d[e]"), "a\\#b\\*c\\_d\\[e\\]");
        assert_eq!(escape_typst("plain text"), "plain text");
    }

    #[test]
    fn build_typ_includes_the_key_fields() {
        let src = build_typ(&sample());
        for needle in [
            "INV/2026/VI/001", "INVOICE", "PT AIS", "AIS Helicopter",
            "Pengembangan Landing Page Website", "Hosting, Domain & Maintenance",
            "Rp 12.000.000", "Dua belas juta rupiah", "Catalyst Labs",
            "BCA", "8415525237", "Bima Pangestu", "11 Juni 2026", "25 Juni 2026",
        ] {
            assert!(src.contains(needle), "build_typ output missing {needle:?}:\n{src}");
        }
    }

    #[test]
    fn build_typ_escapes_a_hash_in_a_value() {
        let mut data = sample();
        data.client.name = "C#Corp".into();
        let src = build_typ(&data);
        assert!(src.contains("C\\#Corp"), "client name not escaped:\n{src}");
    }
}
```

- [ ] **Step 4: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker invoice::render 2>&1 | tail -15`
Expected: FAIL (`escape_typst`/`build_typ` not defined).

- [ ] **Step 5: Implement `escape_typst` + `build_typ`**

Prepend to `backend/src/invoice/render.rs` (above the tests):

```rust
//! Render `InvoiceData` to a PDF via Typst. The Typst source is built from the
//! data in Rust (pure `build_typ`), then compiled with bundled fonts.

use crate::invoice::model::{InvoiceData, LineItem};

/// Backslash-escape the Typst content-mode special characters so interpolated
/// values can't break the markup.
pub fn escape_typst(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if matches!(ch, '\\' | '#' | '$' | '*' | '_' | '`' | '<' | '>' | '@' | '[' | ']' | '"') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

fn line_item_block(item: &LineItem) -> String {
    let title = escape_typst(&item.title);
    let body = match &item.body {
        Some(b) => format!(" \\\n  #text(size: 9pt, fill: gray.darken(20%))[{}]", escape_typst(b)),
        None => String::new(),
    };
    format!(
        "  [*{title}*{body}], [{}], [{}], [{}],\n",
        escape_typst(&item.qty),
        escape_typst(&item.unit_price),
        escape_typst(&item.amount),
    )
}

/// Build the full Typst source document for an invoice.
pub fn build_typ(data: &InvoiceData) -> String {
    let e = escape_typst;
    let client_extra = {
        let mut lines = String::new();
        if let Some(sub) = &data.client.sub_name {
            lines.push_str(&format!(" \\\n{}", e(sub)));
        }
        if let Some(web) = &data.client.website {
            lines.push_str(&format!(" \\\n{}", e(web)));
        }
        lines
    };
    let rows: String = data.line_items.iter().map(line_item_block).collect();

    format!(
        r#"#set page(paper: "a4", margin: (x: 2.2cm, top: 2.2cm, bottom: 2cm))
#set text(font: "Libertinus Serif", size: 10pt, fill: rgb("#1a1a2e"))
#let label(t) = text(size: 8pt, tracking: 1pt, fill: gray.darken(10%))[#upper(t)]

#grid(columns: (1fr, 1fr), align: (left, right),
  [#text(size: 15pt, weight: "bold")[{issuer_name}] \
   #text(size: 9pt, fill: gray.darken(20%))[{issuer_company} · {issuer_website}]],
  [#text(size: 26pt, tracking: 3pt)[INVOICE] \
   #text(size: 9pt, fill: gray.darken(20%))[No. {number}]],
)
#v(6pt)
#line(length: 100%, stroke: 1.5pt + rgb("#1a1a2e"))
#v(10pt)

#grid(columns: (2fr, 1fr, 1fr), gutter: 8pt,
  [#label("Ditagihkan kepada") \ #text(weight: "bold")[{client_name}]{client_extra}],
  [#label("Tanggal invoice") \ #text(weight: "bold")[{issue_date}]],
  [#label("Jatuh tempo") \ #text(weight: "bold")[{due_date}]],
)
#v(14pt)

#table(
  columns: (1fr, auto, auto, auto),
  align: (left, center, right, right),
  stroke: (x, y) => if y == 0 {{ (bottom: 0.8pt) }} else {{ (bottom: 0.4pt + gray) }},
  inset: 8pt,
  table.header([#label("Deskripsi")], [#label("Qty")], [#label("Harga")], [#label("Jumlah")]),
{rows})
#v(10pt)

#align(right, grid(columns: (auto, auto), gutter: 10pt, align: (left, right),
  [#label("Subtotal")], [{subtotal}],
  [#label("PPN")], [—],
  [#text(weight: "bold")[#label("Total Tagihan")]], [#text(size: 14pt, weight: "bold")[{total}]],
))
#v(4pt)
#emph[Terbilang: {terbilang}]
#v(18pt)

#grid(columns: (1.4fr, 1fr), gutter: 16pt, align: (left, right),
  [#label("Pembayaran") \ #v(2pt)
   #box(stroke: 0.5pt + gray, inset: 10pt, radius: 3pt)[
     Bank #h(1fr) *{bank}* \
     No. Rekening #h(1fr) *{account_no}* \
     Atas Nama #h(1fr) *{account_name}*
   ]],
  [#v(20pt) {city}, {issue_date} \ #v(28pt) #text(weight: "bold")[{issuer_name}]],
)

#place(bottom + center, text(size: 8pt, fill: gray)[Terima kasih atas kepercayaan Anda · {issuer_website}])
"#,
        issuer_name = e(&data.issuer.name),
        issuer_company = e(&data.issuer.company),
        issuer_website = e(&data.issuer.website),
        number = e(&data.number),
        client_name = e(&data.client.name),
        client_extra = client_extra,
        issue_date = e(&data.issue_date),
        due_date = e(&data.due_date),
        rows = rows,
        subtotal = e(&data.subtotal),
        total = e(&data.total),
        terbilang = e(&data.terbilang),
        bank = e(&data.payment.bank),
        account_no = e(&data.payment.account_no),
        account_name = e(&data.payment.account_name),
        city = e(&data.issuer.city),
    )
}
```

Note the doubled braces `{{ }}` inside the `format!` raw string are literal Typst braces; single-brace `{name}` are format args. Keep them exactly as written.

- [ ] **Step 6: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker invoice::render 2>&1 | tail -15`
Expected: PASS (the 3 tests; `render_pdf` not yet present — that's Task 2).

- [ ] **Step 7: Commit**

```bash
git add backend/src/invoice/
git commit -m "feat(invoice): add data model and Typst source builder"
```

---

### Task 2: `render_pdf` — compile the Typst source to PDF

**Files:**
- Modify: `backend/src/invoice/render.rs`

- [ ] **Step 1: Write the failing smoke test**

Add to `backend/src/invoice/render.rs`'s test module:

```rust
#[test]
fn render_pdf_produces_a_real_pdf() {
    let pdf = render_pdf(&sample()).expect("render");
    assert!(pdf.len() > 800, "pdf too small: {} bytes", pdf.len());
    assert_eq!(&pdf[..5], b"%PDF-");
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker invoice::render::tests::render_pdf 2>&1 | tail -15`
Expected: FAIL (`render_pdf` not defined).

- [ ] **Step 3: Implement `render_pdf`**

Add to `backend/src/invoice/render.rs` (after `build_typ`, before tests):

```rust
/// Compile the invoice to PDF bytes using Typst with bundled fonts.
pub fn render_pdf(data: &InvoiceData) -> anyhow::Result<Vec<u8>> {
    use typst_as_lib::TypstEngine;
    let source = build_typ(data);
    let fonts: Vec<&[u8]> = typst_assets::fonts().collect();
    let engine = TypstEngine::builder().main_file(source).fonts(fonts).build();
    let doc = engine
        .compile()
        .output
        .map_err(|e| anyhow::anyhow!("typst compile failed: {e:?}"))?;
    let pdf = typst_pdf::pdf(&doc, &Default::default())
        .map_err(|e| anyhow::anyhow!("typst pdf export failed: {e:?}"))?;
    Ok(pdf)
}
```
NOTE: `main_file` accepts an owned `String` source (verified in the spike). If the compiler complains about the argument type, wrap as needed per the `typst-as-lib` 0.15 API, but the spike confirmed a `&'static str` / `String` source works.

- [ ] **Step 4: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker invoice::render::tests::render_pdf 2>&1 | tail -15`
Expected: PASS (compile is slow the first time).
Full invoice tests: `cargo test --bin portfolio-tracker invoice:: 2>&1 | tail -6` → 0 failed.

- [ ] **Step 5: (manual, optional) eyeball the PDF**

Write a throwaway binary or `dbg!` to dump `render_pdf(&sample())` to `/tmp/inv.pdf` and open it to sanity-check the layout against the template. Do NOT commit throwaway code. This is optional — the automated test only checks it's a valid PDF.

- [ ] **Step 6: Commit**

```bash
git add backend/src/invoice/render.rs
git commit -m "feat(invoice): render InvoiceData to PDF via Typst"
```

---

### Task 3: `send_document` on the Telegram client

**Files:**
- Modify: `backend/Cargo.toml` (add reqwest `multipart` feature)
- Modify: `backend/src/telegram/client.rs`

Verified facts about `client.rs`: `struct TelegramClient { token: String, client: reqwest::Client }`; `fn url(&self, method: &str) -> String` builds `https://api.telegram.org/bot<token>/<method>`; `async fn check(resp) -> Result<serde_json::Value, TgError>` handles 401→`Unauthorized`, non-2xx→`Api{status,body}`, JSON parse→`Http`. `TgError` variants: `Unauthorized`, `Http(String)`, `Api{status,body}`, `Shape(String)`. Reuse all of these.

- [ ] **Step 1: Enable reqwest multipart**

`backend/Cargo.toml` currently has:
`reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }`
Add `"multipart"` to the features:
`reqwest = { version = "0.12", features = ["json", "rustls-tls", "multipart"], default-features = false }`

- [ ] **Step 2: Write the failing filename test**

`client.rs` has a `#[cfg(test)] mod tests` (it already contains client tests — confirm with `grep -n "mod tests" src/telegram/client.rs`; if absent, add `#[cfg(test)] mod tests { use super::*; }`). Add this test:

```rust
#[test]
fn document_filename_is_safe() {
    assert_eq!(document_filename("INV/2026/VI/001"), "INV-2026-VI-001.pdf");
}
```

- [ ] **Step 3: Run to verify it fails**

Run: `cargo test --bin portfolio-tracker telegram::client::tests::document_filename 2>&1 | tail -10`
Expected: FAIL (`document_filename` not defined).

- [ ] **Step 4: Implement `document_filename` (free fn) + `send_document` (method)**

Add the free function near the other module-level helpers in `client.rs`:

```rust
/// Sanitize an invoice number into a safe PDF filename:
/// "INV/2026/VI/001" -> "INV-2026-VI-001.pdf".
pub fn document_filename(invoice_number: &str) -> String {
    let safe: String = invoice_number
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    format!("{safe}.pdf")
}
```

Add the method inside `impl TelegramClient` (next to `send_message`), reusing `self.url`, `self.client`, and `Self::check`:

```rust
/// Upload bytes (the invoice PDF) as a Telegram document with a caption.
pub async fn send_document(
    &self,
    chat_id: i64,
    filename: &str,
    bytes: Vec<u8>,
    caption: &str,
) -> Result<(), TgError> {
    let part = reqwest::multipart::Part::bytes(bytes)
        .file_name(filename.to_string())
        .mime_str("application/pdf")
        .map_err(|e| TgError::Http(e.to_string()))?;
    let form = reqwest::multipart::Form::new()
        .text("chat_id", chat_id.to_string())
        .text("caption", caption.to_string())
        .part("document", part);
    let resp = self
        .client
        .post(self.url("sendDocument"))
        .multipart(form)
        .send()
        .await
        .map_err(|e| TgError::Http(e.to_string()))?;
    Self::check(resp).await?;
    Ok(())
}
```

- [ ] **Step 5: Run to verify it passes**

Run: `cargo test --bin portfolio-tracker telegram::client::tests::document_filename 2>&1 | tail -10`
Expected: PASS.
Build: `cargo build --bin portfolio-tracker 2>&1 | tail -5` (compiles; `send_document`/`render_pdf`/model unused warnings are expected until Phase 3 wires them — do NOT add `#[allow]`).
Full suite: `cargo test --bin portfolio-tracker 2>&1 | tail -6` → 0 failed.

- [ ] **Step 6: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/src/telegram/client.rs
git commit -m "feat(telegram): add send_document for uploading the invoice PDF"
```

---

## Self-Review Notes

- **Spec coverage (Phase 2):** model → Task 1; Typst render (escape + build_typ + render_pdf) → Tasks 1-2; `send_document` → Task 3. The `create_invoice`/`list_clients` tools, env issuer/payment config, prompt, and wiring are Phase 3.
- **Type consistency:** `InvoiceData`/`LineItem`/`ClientInfo`/`Issuer`/`Payment` field names are used identically in `build_typ`; `build_typ(&InvoiceData) -> String`, `render_pdf(&InvoiceData) -> Result<Vec<u8>>`, `escape_typst(&str) -> String`, `document_filename(&str) -> String`.
- **Typst escaping** is applied to every interpolated value in `build_typ` (verified by the hash-escape test). The literal Typst braces in the template use `{{ }}` inside `format!`.
- **Telegram skeleton must be adapted** to the real `send_message` patterns in `client.rs` (Step 1 read is mandatory) — the plan's skeleton flags every spot that must match existing code rather than inventing `TgError` variants.
- **Slow builds** (Typst) are expected; not a failure.
- **Visual fidelity** to the Canva template is approximate in v1; the `.typ` layout can be refined later without touching the data/flow.
