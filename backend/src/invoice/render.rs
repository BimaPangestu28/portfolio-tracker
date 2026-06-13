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
        r##"#set page(paper: "a4", margin: (x: 2.2cm, top: 2.2cm, bottom: 2cm))
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
"##,
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
