use crate::db::Db;
use crate::ingestion::extract::{parse_extraction, ExtractError, Extraction, ExtractedEntry};
use crate::ingestion::matching::{resolve_account, suggest_instrument_for_entry};
use crate::llm::claude::Part;
use crate::llm::native::NativeLlmClient;
use crate::repo::review_items::{self, NewReviewItem};
use base64::Engine;

pub const SYSTEM_PROMPT: &str = r#"You extract financial transactions from an uploaded image or PDF for a personal investment tracker.
Classify the document as one of: holdings_snapshot, txn_history, bank_statement, trade_confirmation.
Return ONLY a JSON object, no prose, no markdown fences, no explanations of your arithmetic — do all calculations silently. Your entire response must start with "{" and end with "}". Shaped exactly:
{"doc_type": "<one of the four>", "entries": [ { "entry_type": "buy|sell|dividend|interest|fee|deposit|withdrawal|opening_balance", "symbol": "...", "instrument_name": "...", "quantity": "...", "price_native": "...", "fee_native": "...", "amount_native": "...", "currency": "...", "executed_at": "YYYY-MM-DDTHH:MM:SSZ", "account_hint": "...", "note": "...", "confidence": 0.0 } ] }
Rules: holdings_snapshot rows -> entry_type "opening_balance" with quantity and average cost as price_native. txn_history/trade_confirmation -> buy/sell/dividend/fee. bank_statement -> deposit/withdrawal/dividend/interest. Numbers as strings, no thousands separators, and ALWAYS '.' as the decimal point — write 5661.94, never "5,661.94" or "5661,94". Omit unknown fields. Set confidence in [0,1]. If a value is uncertain, still include the entry with a lower confidence.
IMPORTANT for Indonesian (IDX) brokers such as Stockbit, Ajaib, IPOT, BIONS: a column labeled "AMOUNT" (or "Total"/"Nilai") is the TOTAL transaction value in IDR INCLUDING fees, i.e. amount = quantity*price + fee. It is NOT the share quantity. Put that total in "amount_native" verbatim and do NOT use it as "quantity". IDX shares trade in lots of 100, so quantity is always a positive multiple of 100 (100, 200, 700, ...). Derive quantity by taking amount/price and rounding DOWN to the nearest multiple of 100, then set fee_native = amount - quantity*price. Example: BUY TLKM with AMOUNT 2.012.014 and PRICE 2.870 -> amount/price = 701.05, round down to lot -> quantity "700", fee_native = 2012014 - 700*2870 = "3014", amount_native "2012014", price_native "2870". Never emit quantity 701 here. These IDX lot/fee rules apply only to IDR-denominated stock rows; do NOT apply lot rounding to crypto, US stocks, or fractional shares.
IMPORTANT for Indonesian mutual fund apps such as Bibit: the goal/portfolio name (e.g. "Dana Darurat", "Pendidikan Noah", "Mobil") is NOT the instrument — put it in "account_hint" only. Put the clean fund name (e.g. "Sucorinvest Bond Fund", "Majoris Pasar Uang Indonesia") in "instrument_name". A pending purchase shows only an IDR amount with no units and no NAV — put that amount in "amount_native" and omit "quantity" and "price_native". BUT a settled order or a transaction-detail view DOES show "NAV" and "Jumlah Unit": when both are visible, set "price_native" to the NAV and "quantity" to the unit count (and still put the IDR total in "amount_native"). Never invent units or NAV — only copy them when the document shows them. Skip failed or cancelled orders. Put the order status (e.g. "Pembelian Berhasil") in "note"."#;

/// Decide if an entry needs human attention (low confidence or missing core fields).
pub fn needs_attention(e: &ExtractedEntry) -> bool {
    if e.force_attention { return true; }
    if e.confidence < 0.6 { return true; }
    match e.entry_type.as_str() {
        "deposit" | "withdrawal" | "dividend" | "interest" => e.quantity.is_none() && e.price_native.is_none(),
        // Trades are complete with either a symbol or a name (mutual funds have no
        // ticker) and either units or a total amount (amount-only fund buys).
        _ => {
            let has_name = e.symbol.is_some() || e.instrument_name.is_some();
            let has_size = e.quantity.is_some() || e.amount_native.is_some();
            !has_name || !has_size
        }
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

/// Strip any directory components from a client-supplied filename to prevent path traversal.
fn safe_filename(name: &str) -> anyhow::Result<String> {
    let base = std::path::Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| anyhow::anyhow!("invalid filename: {name}"))?;
    if base.is_empty() || base == "." || base == ".." {
        return Err(anyhow::anyhow!("invalid filename: {name}"));
    }
    Ok(base.to_string())
}

/// Decode + save a file to data/uploads/<batch_id>/, returning (kind, path).
fn save_file(batch_id: &str, f: &UploadFile) -> anyhow::Result<(String, String)> {
    let dir = format!("data/uploads/{batch_id}");
    std::fs::create_dir_all(&dir)?;
    let safe = safe_filename(&f.filename)?;
    let path = format!("{dir}/{safe}");
    let bytes = base64::engine::general_purpose::STANDARD.decode(f.data_base64.as_bytes())
        .map_err(|e| anyhow::anyhow!("bad base64 for {}: {e}", f.filename))?;
    std::fs::write(&path, &bytes)?;
    let kind = if f.media_type == "application/pdf" { "pdf" } else { "image" };
    Ok((kind.to_string(), path))
}

/// Review payload staged when a PDF is uploaded: the vision client extracts
/// from images only, so the file is kept for the user to re-upload as an image.
const PDF_UNSUPPORTED_PAYLOAD: &str =
    "{\"note\":\"PDF belum didukung — unggah ulang sebagai gambar (PNG/JPG).\"}";

/// The vision model declined to read the document — it returned a natural-language
/// refusal instead of JSON, and did so again on retry. Surfaced as a typed error
/// (downcastable from the `anyhow::Error`) so the Telegram/API layer can tell the
/// owner the real cause rather than a generic "failed to process the file".
#[derive(Debug, thiserror::Error)]
#[error("vision model refused to extract from the document")]
pub struct ModelRefused;

/// How many times to ask the vision model before giving up on a refusal.
const MAX_EXTRACT_ATTEMPTS: usize = 2;

/// Call the vision model and parse its JSON, retrying once when the model returns
/// a prose-only refusal. Such refusals are largely stochastic, so a second
/// attempt frequently succeeds; one that survives the retry becomes a typed
/// [`ModelRefused`] error. Malformed-but-JSON-ish replies are a different problem
/// and fail fast with the original parse error.
///
/// Returns the parsed extraction alongside the raw model reply (kept for the
/// `raw_llm_json` audit column).
async fn extract_with_refusal_retry(
    client: &NativeLlmClient,
    parts: &[Part],
) -> anyhow::Result<(Extraction, String)> {
    let mut last_snippet = String::new();
    for attempt in 1..=MAX_EXTRACT_ATTEMPTS {
        let raw = client.complete(SYSTEM_PROMPT, parts).await
            .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
        match parse_extraction(&raw) {
            Ok(extraction) => return Ok((extraction, raw)),
            Err(ExtractError::Refused(snippet)) => {
                tracing::warn!(
                    "ingest: vision model refused (attempt {attempt}/{MAX_EXTRACT_ATTEMPTS}): {snippet}"
                );
                last_snippet = snippet;
            }
            Err(e) => return Err(anyhow::anyhow!("parse error: {e}; raw={raw}")),
        }
    }
    Err(anyhow::Error::new(ModelRefused)
        .context(format!("model refused after {MAX_EXTRACT_ATTEMPTS} attempts: {last_snippet}")))
}

/// Full pipeline for one upload batch: save files, call the vision model once per
/// image, parse, stage items. `batch_id` is supplied by the caller (the API
/// layer) so it is deterministic/testable.
pub async fn ingest_batch(db: &Db, client: &NativeLlmClient, batch_id: &str, files: &[UploadFile]) -> anyhow::Result<IngestResult> {
    let mut items = Vec::new();
    for f in files {
        let (kind, path) = save_file(batch_id, f)?;
        if f.media_type == "application/pdf" {
            let row = review_items::create(db, &NewReviewItem {
                batch_id,
                source_kind: &kind,
                source_filename: &f.filename,
                source_path: &path,
                doc_type: "unknown",
                needs_attention: true,
                payload_json: PDF_UNSUPPORTED_PAYLOAD,
                raw_llm_json: "{}",
                suggested_instrument_id: None,
                suggested_account_id: None,
            }).await?;
            items.push(row);
            continue;
        }
        let parts = vec![
            Part::Text("Extract per the system instructions.".into()),
            Part::Image(f.media_type.clone(), f.data_base64.clone()),
        ];
        let (extraction, raw) = extract_with_refusal_retry(client, &parts).await?;
        if extraction.entries.is_empty() {
            let row = review_items::create(db, &NewReviewItem {
                batch_id,
                source_kind: &kind,
                source_filename: &f.filename,
                source_path: &path,
                doc_type: &extraction.doc_type,
                needs_attention: true,
                payload_json: "{\"note\":\"no entries extracted from this document\"}",
                raw_llm_json: &raw,
                suggested_instrument_id: None,
                suggested_account_id: None,
            }).await?;
            items.push(row);
            continue;
        }
        for entry in &extraction.entries {
            let payload = serde_json::to_string(entry)?;
            let sug_ins = suggest_instrument_for_entry(db, entry.symbol.as_deref(), entry.instrument_name.as_deref()).await?;
            // Resolve the account against the DB (exact name, then this
            // instrument's history, then a single fuzzy match) so previously-seen
            // instruments don't show up as "belum dikenali".
            let sug_acc = resolve_account(db, entry.account_hint.as_deref(), sug_ins).await?;
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
            executed_at:None, account_hint:None, note:None, confidence:conf, amount_native:None, force_attention:false }
    }

    #[test]
    fn model_refused_survives_context_for_downcast() {
        // The Telegram layer distinguishes a refusal via downcast_ref through the
        // anyhow context chain; lock that contract so the tailored reply keeps firing.
        let err = anyhow::Error::new(ModelRefused)
            .context("model refused after 2 attempts: I'm sorry, I cannot fulfill this request.");
        assert!(err.downcast_ref::<ModelRefused>().is_some());
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

    /// Bibit-style mutual fund buy: name + IDR amount, no symbol/units/NAV.
    fn fund_entry() -> ExtractedEntry {
        ExtractedEntry { entry_type:"buy".into(), symbol:None,
            instrument_name:Some("Sucorinvest Bond Fund".into()),
            quantity:None, price_native:None, fee_native:None, currency:Some("IDR".into()),
            executed_at:None, account_hint:Some("Pendidikan Noah".into()), note:None,
            confidence:0.72, amount_native:Some("13000000".into()), force_attention:false }
    }

    #[test]
    fn amount_only_fund_buy_with_name_is_complete() {
        assert!(!needs_attention(&fund_entry()));
    }
    #[test]
    fn amount_only_without_any_name_needs_attention() {
        let mut e = fund_entry();
        e.instrument_name = None;
        assert!(needs_attention(&e));
    }
    #[test]
    fn name_without_quantity_or_amount_needs_attention() {
        let mut e = fund_entry();
        e.amount_native = None;
        assert!(needs_attention(&e));
    }

    #[test]
    fn system_prompt_tells_model_to_capture_nav_and_units_when_shown() {
        assert!(super::SYSTEM_PROMPT.contains("Jumlah Unit") || super::SYSTEM_PROMPT.contains("unit count"));
        assert!(super::SYSTEM_PROMPT.contains("NAV") && super::SYSTEM_PROMPT.contains("price_native"));
    }

    #[test]
    fn safe_filename_strips_traversal() {
        assert_eq!(super::safe_filename("../../etc/passwd").unwrap(), "passwd");
        assert_eq!(super::safe_filename("a.png").unwrap(), "a.png");
        assert_eq!(super::safe_filename("sub/dir/x.pdf").unwrap(), "x.pdf");
        assert!(super::safe_filename("..").is_err());
        assert!(super::safe_filename("").is_err());
    }

    /// Live end-to-end check against the configured vision provider. Skips
    /// unless both the client env (OPENAI_API_KEY etc.) and INGEST_SMOKE_IMAGE
    /// (a path to a real image — OpenAI's parser rejects 1x1 placeholders) are
    /// set. Run with:
    ///   INGEST_SMOKE_IMAGE=/path/to/broker.png cargo test live_extract_smoke -- --ignored
    #[tokio::test]
    #[ignore]
    async fn live_extract_smoke() {
        let client = match crate::llm::native::NativeLlmClient::from_env() { Ok(c) => c, Err(_) => return };
        let img_path = match std::env::var("INGEST_SMOKE_IMAGE") { Ok(p) if !p.is_empty() => p, _ => return };
        let bytes = std::fs::read(&img_path).expect("read INGEST_SMOKE_IMAGE");
        let media_type = if img_path.ends_with(".png") { "image/png" } else { "image/jpeg" };
        let data_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let parts = vec![
            Part::Text("Extract per the system instructions.".into()),
            Part::Image(media_type.into(), data_base64),
        ];
        let out = client.complete(SYSTEM_PROMPT, &parts).await.unwrap();
        let parsed = crate::ingestion::extract::parse_extraction(&out).unwrap();
        assert!(["holdings_snapshot","txn_history","bank_statement","trade_confirmation"].contains(&parsed.doc_type.as_str()));
    }
}
