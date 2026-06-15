//! Reconstruct display-ready InvoiceData from a stored InvoiceRow for re-rendering.

use crate::invoice::assemble::{assemble_invoice_data, ParsedItem};
use crate::invoice::config::InvoiceConfig;
use crate::invoice::model::InvoiceData;
use crate::repo::clients::ClientRow;
use crate::repo::invoices::InvoiceRow;
use serde::Deserialize;

#[derive(Deserialize)]
struct StoredItem {
    title: String,
    #[serde(default)]
    body: Option<String>,
    qty: i64,
    amount: i64,
}

/// Rebuild display-ready `InvoiceData` from a saved row. Parses `line_items_json`
/// and preserves the stored `due_date` by deriving `due_days` from the stored
/// issue/due dates (so a re-rendered PDF matches what was originally issued).
pub fn data_from_row(
    row: &InvoiceRow,
    client: &ClientRow,
    mut config: InvoiceConfig,
) -> anyhow::Result<InvoiceData> {
    let stored: Vec<StoredItem> = serde_json::from_str(&row.line_items_json)?;
    let items: Vec<ParsedItem> = stored
        .into_iter()
        .map(|s| ParsedItem { title: s.title, body: s.body, qty: s.qty, amount_idr: s.amount })
        .collect();
    let issue = chrono::NaiveDate::parse_from_str(&row.issue_date, "%Y-%m-%d")?;
    let due = chrono::NaiveDate::parse_from_str(&row.due_date, "%Y-%m-%d")?;
    config.due_days = (due - issue).num_days();
    Ok(assemble_invoice_data(row.number.clone(), issue, &config, client, &items))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::invoice::config::InvoiceConfig;
    use crate::invoice::model::{Issuer, Payment};
    use crate::repo::clients::ClientRow;
    use crate::repo::invoices::InvoiceRow;

    fn config() -> InvoiceConfig {
        InvoiceConfig {
            issuer: Issuer { name: "Bima".into(), company: "Catalyst".into(), website: "catalystlabs.id".into(), city: "Jakarta".into() },
            payment: Payment { bank: "BCA".into(), account_no: "123".into(), account_name: "Bima".into() },
            due_days: 14,
        }
    }

    #[test]
    fn rebuilds_data_and_preserves_stored_due_date() {
        let row = InvoiceRow {
            id: 1,
            number: "INV/2026/VI/001".into(),
            client_id: 1,
            issue_date: "2026-06-11".into(),
            due_date: "2026-06-30".into(),
            subtotal: "Rp 12.000.000".into(),
            total: "Rp 12.000.000".into(),
            line_items_json: r#"[{"title":"Landing","body":null,"qty":1,"amount":12000000}]"#.into(),
            created_at: "2026-06-11T08:00:00Z".into(),
        };
        let client = ClientRow { id: 1, name: "PT AIS".into(), sub_name: None, website: None, created_at: String::new() };
        let data = data_from_row(&row, &client, config()).unwrap();
        assert_eq!(data.number, "INV/2026/VI/001");
        assert_eq!(data.total, "Rp 12.000.000");
        assert_eq!(data.issue_date, "11 Juni 2026");
        assert_eq!(data.due_date, "30 Juni 2026");
        assert_eq!(data.line_items.len(), 1);
    }
}
