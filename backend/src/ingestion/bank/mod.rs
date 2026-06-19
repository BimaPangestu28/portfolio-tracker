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
