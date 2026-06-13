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
