use crate::ingestion::extract::ExtractedEntry;
use std::collections::HashMap;

/// Maps ExtractedEntry field names -> CSV column header names.
/// Recognized field keys: entry_type, symbol, quantity, price_native, fee_native, currency, executed_at, account_hint
pub type ColumnMapping = HashMap<String, String>;

#[derive(Debug, thiserror::Error)]
pub enum CsvError {
    #[error("empty csv")] Empty,
    #[error("no header column '{0}' for field '{1}'")] BadColumn(String, String),
    #[error("entry_type missing: provide an entry_type column mapping or a constant")] NoEntryType,
}

/// Parse CSV text (comma-separated, first line = header, no quoted-comma support) into entries.
/// `entry_type_const` supplies entry_type when there is no mapped column for it.
pub fn parse_csv_rows(csv_text: &str, mapping: &ColumnMapping, entry_type_const: Option<&str>) -> Result<Vec<ExtractedEntry>, CsvError> {
    let mut lines = csv_text.lines().filter(|l| !l.trim().is_empty());
    let header: Vec<String> = lines.next().ok_or(CsvError::Empty)?.split(',').map(|s| s.trim().to_string()).collect();
    let col = |field: &str| -> Option<usize> {
        mapping.get(field).and_then(|h| header.iter().position(|x| x == h))
    };
    // validate mapped columns exist
    for (field, h) in mapping {
        if !header.iter().any(|x| x == h) { return Err(CsvError::BadColumn(h.clone(), field.clone())); }
    }
    let type_idx = col("entry_type");
    if type_idx.is_none() && entry_type_const.is_none() { return Err(CsvError::NoEntryType); }
    let get = |cells: &Vec<String>, field: &str| -> Option<String> {
        col(field).and_then(|i| cells.get(i)).map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    };
    let mut out = Vec::new();
    for line in lines {
        let cells: Vec<String> = line.split(',').map(|s| s.trim().to_string()).collect();
        let entry_type = get(&cells, "entry_type").or_else(|| entry_type_const.map(String::from)).unwrap_or_default();
        out.push(ExtractedEntry {
            entry_type,
            symbol: get(&cells, "symbol"),
            instrument_name: None,
            quantity: get(&cells, "quantity"),
            price_native: get(&cells, "price_native"),
            fee_native: get(&cells, "fee_native"),
            currency: get(&cells, "currency"),
            executed_at: get(&cells, "executed_at"),
            account_hint: get(&cells, "account_hint"),
            note: None,
            confidence: 1.0,
            amount_native: get(&cells, "amount_native"),
            force_attention: false,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn maps_rows_to_entries() {
        let csv = "Date,Side,Ticker,Qty,Price\n2026-01-02,buy,BTC,0.5,60000\n2026-01-03,sell,ETH,2,3000\n";
        let mut m = ColumnMapping::new();
        m.insert("executed_at".into(),"Date".into());
        m.insert("entry_type".into(),"Side".into());
        m.insert("symbol".into(),"Ticker".into());
        m.insert("quantity".into(),"Qty".into());
        m.insert("price_native".into(),"Price".into());
        let rows = parse_csv_rows(csv, &m, None).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].entry_type, "buy");
        assert_eq!(rows[0].symbol.as_deref(), Some("BTC"));
        assert_eq!(rows[1].entry_type, "sell");
    }
    #[test]
    fn entry_type_const_used_when_no_column() {
        let csv = "Ticker,Qty,Price\nVOO,1,400\n";
        let mut m = ColumnMapping::new();
        m.insert("symbol".into(),"Ticker".into());
        m.insert("quantity".into(),"Qty".into());
        m.insert("price_native".into(),"Price".into());
        let rows = parse_csv_rows(csv, &m, Some("buy")).unwrap();
        assert_eq!(rows[0].entry_type, "buy");
    }
    #[test]
    fn missing_required_mapping_errors() {
        // neither entry_type column nor const provided
        let csv = "Ticker\nBTC\n";
        let mut m = ColumnMapping::new();
        m.insert("symbol".into(),"Ticker".into());
        assert!(parse_csv_rows(csv, &m, None).is_err());
    }
}
