use serde::{Deserialize, Serialize};

/// One candidate ledger entry extracted by the LLM (pre-mapping, pre-confirm).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExtractedEntry {
    pub entry_type: String, // buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance
    #[serde(default)]
    pub symbol: Option<String>,
    #[serde(default)]
    pub instrument_name: Option<String>,
    #[serde(default)]
    pub quantity: Option<String>,
    #[serde(default)]
    pub price_native: Option<String>,
    #[serde(default)]
    pub fee_native: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub executed_at: Option<String>,
    #[serde(default)]
    pub account_hint: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default = "default_conf")]
    pub confidence: f64,
}
fn default_conf() -> f64 { 1.0 }

#[derive(Debug, Clone)]
pub struct Extraction {
    pub doc_type: String,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    #[error("response not valid JSON: {0}")]
    NotJson(String),
    #[error("missing field: {0}")]
    Missing(String),
}

/// Strip an optional ```json ... ``` markdown fence the model may wrap around the JSON.
fn strip_fence(s: &str) -> &str {
    let t = s.trim();
    if let Some(rest) = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")) {
        return rest.trim().strip_suffix("```").unwrap_or(rest).trim();
    }
    t
}

pub fn parse_extraction(raw: &str) -> Result<Extraction, ExtractError> {
    let cleaned = strip_fence(raw);
    let v: serde_json::Value = serde_json::from_str(cleaned).map_err(|e| ExtractError::NotJson(e.to_string()))?;
    let doc_type = v.get("doc_type").and_then(|d| d.as_str())
        .ok_or_else(|| ExtractError::Missing("doc_type".into()))?.to_string();
    let entries_val = v.get("entries").cloned().unwrap_or_else(|| serde_json::json!([]));
    let entries: Vec<ExtractedEntry> = serde_json::from_value(entries_val)
        .map_err(|e| ExtractError::NotJson(e.to_string()))?;
    Ok(Extraction { doc_type, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_holdings_snapshot() {
        let raw = r#"{"doc_type":"holdings_snapshot","entries":[
            {"entry_type":"opening_balance","symbol":"BTC","quantity":"0.5","price_native":"60000","currency":"USD","confidence":0.9}
        ]}"#;
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "holdings_snapshot");
        assert_eq!(e.entries.len(), 1);
        assert_eq!(e.entries[0].symbol.as_deref(), Some("BTC"));
        assert_eq!(e.entries[0].entry_type, "opening_balance");
    }

    #[test]
    fn tolerates_json_wrapped_in_markdown_fence() {
        let raw = "```json\n{\"doc_type\":\"txn_history\",\"entries\":[]}\n```";
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "txn_history");
        assert_eq!(e.entries.len(), 0);
    }

    #[test]
    fn missing_doc_type_errors() {
        let raw = r#"{"entries":[]}"#;
        assert!(matches!(parse_extraction(raw), Err(ExtractError::Missing(_))));
    }

    #[test]
    fn defaults_confidence_when_absent() {
        let raw = r#"{"doc_type":"trade_confirmation","entries":[{"entry_type":"buy","symbol":"VOO"}]}"#;
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.entries[0].confidence, 1.0);
    }
}
