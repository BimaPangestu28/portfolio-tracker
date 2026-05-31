use crate::db::Db;
use crate::ingestion::extract::{parse_extraction, ExtractedEntry};
use crate::ingestion::matching::{suggest_account, suggest_instrument};
use crate::llm::claude::{ClaudeClient, Part};
use crate::repo::review_items::{self, NewReviewItem};
use base64::Engine;

pub const SYSTEM_PROMPT: &str = r#"You extract financial transactions from an uploaded image or PDF for a personal investment tracker.
Classify the document as one of: holdings_snapshot, txn_history, bank_statement, trade_confirmation.
Return ONLY a JSON object, no prose, shaped exactly:
{"doc_type": "<one of the four>", "entries": [ { "entry_type": "buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance", "symbol": "...", "instrument_name": "...", "quantity": "...", "price_native": "...", "fee_native": "...", "currency": "...", "executed_at": "YYYY-MM-DDTHH:MM:SSZ", "account_hint": "...", "note": "...", "confidence": 0.0 } ] }
Rules: holdings_snapshot rows -> entry_type "opening_balance" with quantity and average cost as price_native. txn_history/trade_confirmation -> buy/sell/dividend/fee. bank_statement -> deposit/withdrawal/dividend/interest. Numbers as strings, no thousands separators. Omit unknown fields. Set confidence in [0,1]. If a value is uncertain, still include the entry with a lower confidence."#;

/// Decide if an entry needs human attention (low confidence or missing core fields).
pub fn needs_attention(e: &ExtractedEntry) -> bool {
    if e.confidence < 0.6 { return true; }
    match e.entry_type.as_str() {
        "deposit" | "withdrawal" | "dividend" | "interest" => e.quantity.is_none() && e.price_native.is_none(),
        _ => e.symbol.is_none() || e.quantity.is_none(),
    }
}

pub struct UploadFile {
    pub filename: String,
    pub media_type: String, // "image/png", "image/jpeg", "application/pdf"
    pub data_base64: String,
}

pub struct IngestResult {
    pub batch_id: String,
    pub items: Vec<review_items::ReviewItemRow>,
}

/// Decode + save a file to data/uploads/<batch_id>/, returning (kind, path).
fn save_file(batch_id: &str, f: &UploadFile) -> anyhow::Result<(String, String)> {
    let dir = format!("data/uploads/{batch_id}");
    std::fs::create_dir_all(&dir)?;
    let path = format!("{dir}/{}", f.filename);
    let bytes = base64::engine::general_purpose::STANDARD.decode(f.data_base64.as_bytes())
        .map_err(|e| anyhow::anyhow!("bad base64 for {}: {e}", f.filename))?;
    std::fs::write(&path, &bytes)?;
    let kind = if f.media_type == "application/pdf" { "pdf" } else { "image" };
    Ok((kind.to_string(), path))
}

fn to_part(f: &UploadFile) -> Part {
    if f.media_type == "application/pdf" {
        Part::Pdf(f.data_base64.clone())
    } else {
        Part::Image(f.media_type.clone(), f.data_base64.clone())
    }
}

/// Full pipeline for one upload batch: save files, call Claude once per file, parse, stage items.
/// `batch_id` is supplied by the caller (the API layer) so it is deterministic/testable.
pub async fn ingest_batch(db: &Db, client: &ClaudeClient, batch_id: &str, files: &[UploadFile]) -> anyhow::Result<IngestResult> {
    let mut items = Vec::new();
    for f in files {
        let (kind, path) = save_file(batch_id, f)?;
        let parts = vec![Part::Text("Extract per the system instructions.".into()), to_part(f)];
        let raw = client.complete(SYSTEM_PROMPT, &parts).await
            .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
        let extraction = parse_extraction(&raw)
            .map_err(|e| anyhow::anyhow!("parse error: {e}; raw={raw}"))?;
        for entry in &extraction.entries {
            let payload = serde_json::to_string(entry)?;
            let sug_ins = match &entry.symbol { Some(s) => suggest_instrument(db, s).await?, None => None };
            let sug_acc = match &entry.account_hint { Some(a) => suggest_account(db, a).await?, None => None };
            let row = review_items::create(db, &NewReviewItem {
                batch_id,
                source_kind: &kind,
                source_filename: &f.filename,
                source_path: &path,
                doc_type: &extraction.doc_type,
                needs_attention: needs_attention(entry),
                payload_json: &payload,
                raw_llm_json: &raw,
                suggested_instrument_id: sug_ins,
                suggested_account_id: sug_acc,
            }).await?;
            items.push(row);
        }
    }
    Ok(IngestResult { batch_id: batch_id.to_string(), items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingestion::extract::ExtractedEntry;

    fn entry(conf: f64, symbol: Option<&str>, qty: Option<&str>) -> ExtractedEntry {
        ExtractedEntry { entry_type:"buy".into(), symbol:symbol.map(String::from), instrument_name:None,
            quantity:qty.map(String::from), price_native:Some("1".into()), fee_native:None, currency:Some("USD".into()),
            executed_at:None, account_hint:None, note:None, confidence:conf }
    }

    #[test]
    fn low_confidence_needs_attention() {
        assert!(needs_attention(&entry(0.4, Some("BTC"), Some("1"))));
    }
    #[test]
    fn missing_symbol_needs_attention() {
        assert!(needs_attention(&entry(0.9, None, Some("1"))));
    }
    #[test]
    fn complete_high_confidence_ok() {
        assert!(!needs_attention(&entry(0.9, Some("BTC"), Some("1"))));
    }
}
