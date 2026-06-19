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
    /// Raw total transaction value as printed on the document (e.g. Stockbit's
    /// "AMOUNT" column for IDX brokers = qty*price + fee, in IDR). Used by
    /// deterministic post-processing to recover lot-rounded quantity and fee.
    #[serde(default)]
    pub amount_native: Option<String>,
    /// Set by deterministic post-processing when a value looked off but could
    /// not be safely corrected; forces the item into the human review queue.
    #[serde(default)]
    pub force_attention: bool,
    /// Cashflow category name chosen by deterministic bank-statement parsing
    /// (e.g. BCA). Read at confirm time to attach the cashflow row's category.
    #[serde(default)]
    pub cashflow_category: Option<String>,
    /// Stable provenance key for deduplicating bank-statement rows across
    /// re-uploads. Set by deterministic parsing, never by the LLM.
    #[serde(default)]
    pub external_ref: Option<String>,
}
fn default_conf() -> f64 { 1.0 }

#[derive(Debug, Clone)]
pub struct Extraction {
    pub doc_type: String,
    pub entries: Vec<ExtractedEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExtractError {
    /// The model returned a natural-language reply with no JSON object at all —
    /// almost always a content refusal (e.g. "I'm sorry, I cannot fulfill this
    /// request."). Kept distinct from `NotJson` so the caller can retry and,
    /// failing that, explain the real cause instead of a generic parse error.
    #[error("model refused (no JSON in response): {0}")]
    Refused(String),
    #[error("response not valid JSON: {0}")]
    NotJson(String),
    #[error("missing field: {0}")]
    Missing(String),
}

/// Longest refusal snippet preserved for logs/messages — enough to recognize the
/// refusal without dumping an unbounded model reply.
const REFUSAL_SNIPPET_LEN: usize = 200;

/// Locate the JSON object in a model response. Despite the ONLY-JSON
/// instruction the model sometimes prefixes reasoning prose and/or wraps the
/// JSON in a ```json fence anywhere in the response — find it instead of
/// assuming the response starts with it.
fn extract_json(s: &str) -> &str {
    // Prefer an explicit ```json fence wherever it appears.
    if let Some(start) = s.find("```json") {
        let body = &s[start + "```json".len()..];
        let body = body.strip_prefix('\n').unwrap_or(body);
        if let Some(end) = body.find("```") {
            return body[..end].trim();
        }
        return body.trim();
    }
    // Otherwise take the outermost braces (tolerates prose before/after).
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start < end {
            return s[start..=end].trim();
        }
    }
    s.trim()
}

pub fn parse_extraction(raw: &str) -> Result<Extraction, ExtractError> {
    // A valid extraction is always a JSON object, so an absence of braces means
    // the model emitted prose only — i.e. it declined the task. Surface that as a
    // refusal rather than letting serde report a vague "expected value" error.
    if !raw.contains('{') {
        let snippet: String = raw.trim().chars().take(REFUSAL_SNIPPET_LEN).collect();
        return Err(ExtractError::Refused(snippet));
    }
    let cleaned = extract_json(raw);
    let v: serde_json::Value = serde_json::from_str(cleaned).map_err(|e| ExtractError::NotJson(e.to_string()))?;
    let doc_type = v.get("doc_type").and_then(|d| d.as_str())
        .ok_or_else(|| ExtractError::Missing("doc_type".into()))?.to_string();
    let entries_val = v.get("entries").cloned().unwrap_or_else(|| serde_json::json!([]));
    let mut entries: Vec<ExtractedEntry> = serde_json::from_value(entries_val)
        .map_err(|e| ExtractError::NotJson(e.to_string()))?;
    for e in &mut entries {
        normalize_entry(e);
    }
    Ok(Extraction { doc_type, entries })
}

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

/// One IDX trading lot = 100 shares. Quantities on the regular market are always
/// integer multiples of this.
const IDX_LOT: i64 = 100;
/// Price floor (IDR) below which we do NOT assume a row is an IDX equity. Keeps
/// us away from crypto/foreign fractional instruments priced in small numbers.
const IDX_PRICE_FLOOR: Decimal = Decimal::from_parts(50, 0, 0, false, 0);
/// A derived fee larger than this fraction of the total amount is implausible for
/// an IDX broker (typical all-in fee is well under 1%); we flag rather than fabricate.
const MAX_FEE_FRACTION: f64 = 0.01;

fn parse_dec(s: &Option<String>) -> Option<Decimal> {
    s.as_deref().and_then(|t| {
        let cleaned: String = t.chars().filter(|c| !c.is_whitespace()).collect();
        cleaned.parse::<Decimal>().ok()
    })
}

/// Render a Decimal without trailing zeros / separators, matching the LLM string convention.
fn dec_to_string(d: Decimal) -> String {
    d.normalize().to_string()
}

/// Deterministic, defensive normalization applied to every extracted entry after parsing.
///
/// The headline case (the bug): IDX brokers such as Stockbit print an "AMOUNT"
/// column that is the *total* IDR value of the trade including fee
/// (`amount = quantity*price + fee`). The vision model frequently mistakes that
/// column for share quantity, emitting `quantity = round(amount/price)` and no fee.
///
/// When we have the total `amount` and a `price` that looks like an IDX equity
/// (IDR currency, price >= IDX_PRICE_FLOOR), we recover the truth deterministically
/// as `quantity = floor(amount / price / 100) * 100` (regular-market lots) and
/// `fee = amount - quantity*price`.
///
/// We only commit the change when the implied fee is non-negative and a plausible
/// fraction (< MAX_FEE_FRACTION) of the amount. Otherwise we leave the data exactly
/// as extracted and set `force_attention` so a human reviews it — we never fabricate
/// a large/implausible fee or silently mangle a valid odd-lot/nego trade.
pub fn normalize_entry(e: &mut ExtractedEntry) {
    if e.entry_type != "buy" && e.entry_type != "sell" {
        return;
    }
    if e.currency.as_deref() != Some("IDR") {
        return;
    }
    let price = match parse_dec(&e.price_native) {
        Some(p) if p >= IDX_PRICE_FLOOR => p,
        _ => return, // missing/low price -> not a regular-market IDX equity; leave alone
    };

    // We can only recover lot/fee deterministically when the total amount was captured.
    let amount = match parse_dec(&e.amount_native) {
        Some(a) if a > Decimal::ZERO => a,
        _ => return,
    };

    // Lots-floored quantity from the total amount.
    let raw_qty = amount / price; // shares-equivalent of the total (incl. fee)
    let raw_qty_i = match raw_qty.floor().to_i64() {
        Some(v) if v >= IDX_LOT => v,
        _ => {
            // Less than a full lot implied -> ambiguous (odd lot / nego market). Flag it.
            e.force_attention = true;
            return;
        }
    };
    let lot_qty = (raw_qty_i / IDX_LOT) * IDX_LOT;
    let qty_dec = Decimal::from(lot_qty);
    let fee = amount - qty_dec * price;

    // Plausibility guard: fee must be non-negative and a small fraction of the total.
    let fee_ratio = (fee / amount).to_f64().unwrap_or(1.0);
    if fee < Decimal::ZERO || fee_ratio >= MAX_FEE_FRACTION {
        // Can't safely correct: don't fabricate a fee, don't mangle qty. Let a human decide.
        e.force_attention = true;
        return;
    }

    e.quantity = Some(dec_to_string(qty_dec));
    e.fee_native = Some(dec_to_string(fee));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx_entry(qty: Option<&str>, price: &str, amount: Option<&str>) -> ExtractedEntry {
        ExtractedEntry {
            entry_type: "buy".into(),
            symbol: Some("TLKM".into()),
            instrument_name: None,
            quantity: qty.map(String::from),
            price_native: Some(price.into()),
            fee_native: None,
            currency: Some("IDR".into()),
            executed_at: None,
            account_hint: None,
            note: None,
            confidence: 1.0,
            amount_native: amount.map(String::from),
            force_attention: false,
            cashflow_category: None,
            external_ref: None,
        }
    }

    // --- IDX lot/fee normalization (the real bug) ---
    // Stockbit "AMOUNT" = total IDR incl. fee. LLM mis-reads it as quantity
    // (qty = round(amount/price)). We recover qty (floor to lot of 100) and
    // fee = amount - qty*price from the preserved amount.

    #[test]
    fn idx_tlkm_amount_misread_as_quantity_is_corrected() {
        // BUY TLKM, AMOUNT 2.012.014, PRICE 2.870 -> qty 700 (lot of 100),
        // fee = 2_012_014 - 700*2_870 = 3_014.
        let mut e = idx_entry(Some("701"), "2870", Some("2012014"));
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("700"));
        assert_eq!(e.fee_native.as_deref(), Some("3014"));
    }

    #[test]
    fn idx_bmri_corrected() {
        // BMRI amount 793.188 price 3.960 -> qty 200 fee 1.188.
        let mut e = idx_entry(Some("200"), "3960", Some("793188"));
        e.symbol = Some("BMRI".into());
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("200"));
        assert_eq!(e.fee_native.as_deref(), Some("1188"));
    }

    #[test]
    fn idx_ptba_corrected() {
        // PTBA 1.041.560/2.600 -> qty 400 fee 1.560.
        let mut e = idx_entry(Some("401"), "2600", Some("1041560"));
        e.symbol = Some("PTBA".into());
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("400"));
        assert_eq!(e.fee_native.as_deref(), Some("1560"));
    }

    #[test]
    fn idx_asii_300_corrected() {
        // ASII 1.406.106/4.680 -> qty 300 fee 2.106.
        let mut e = idx_entry(Some("300"), "4680", Some("1406106"));
        e.symbol = Some("ASII".into());
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("300"));
        assert_eq!(e.fee_native.as_deref(), Some("2106"));
    }

    #[test]
    fn idx_asii_200_corrected() {
        // ASII 1.181.770/5.900 -> qty 200 fee 1.770.
        let mut e = idx_entry(Some("200"), "5900", Some("1181770"));
        e.symbol = Some("ASII".into());
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("200"));
        assert_eq!(e.fee_native.as_deref(), Some("1770"));
    }

    #[test]
    fn idx_recovers_even_when_quantity_missing() {
        // If the LLM only captured amount + price, derive qty + fee from them.
        let mut e = idx_entry(None, "2870", Some("2012014"));
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("700"));
        assert_eq!(e.fee_native.as_deref(), Some("3014"));
    }

    #[test]
    fn idx_already_clean_quantity_with_fee_and_no_amount_is_untouched() {
        // No amount column captured and qty already a clean lot with fee -> leave alone.
        let mut e = idx_entry(Some("700"), "2870", None);
        e.fee_native = Some("3014".into());
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("700"));
        assert_eq!(e.fee_native.as_deref(), Some("3014"));
        assert!(!e.force_attention);
    }

    #[test]
    fn crypto_fractional_usd_is_untouched() {
        // Fractional crypto in USD must never be snapped to a lot.
        let mut e = idx_entry(Some("0.5"), "60000", Some("30000"));
        e.symbol = Some("BTC".into());
        e.currency = Some("USD".into());
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("0.5"));
        assert_eq!(e.fee_native, None);
        assert!(!e.force_attention);
    }

    #[test]
    fn idx_low_price_below_threshold_not_treated_as_idx_lot() {
        // price < 50 heuristic: not an IDX share, leave alone.
        let mut e = idx_entry(Some("123"), "10", Some("1230"));
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("123"));
        assert!(e.fee_native.is_none());
        assert!(!e.force_attention);
    }

    #[test]
    fn idx_implausible_fee_flagged_not_mangled() {
        // amount/price floored to lot leaves a "fee" far larger than 1% of amount.
        // We must not fabricate that fee; leave data and force attention instead.
        // amount 2_000_000, price 2_870 -> amount/price=696.8 -> lot 600, qty*price=1_722_000,
        // implied fee 278_000 (~14%) -> implausible.
        let mut e = idx_entry(None, "2870", Some("2000000"));
        normalize_entry(&mut e);
        assert!(e.force_attention);
    }

    #[test]
    fn idx_sell_also_corrected() {
        let mut e = idx_entry(Some("701"), "2870", Some("2012014"));
        e.entry_type = "sell".into();
        normalize_entry(&mut e);
        assert_eq!(e.quantity.as_deref(), Some("700"));
        assert_eq!(e.fee_native.as_deref(), Some("3014"));
    }

    #[test]
    fn tolerates_reasoning_prose_before_fenced_json() {
        // Real failure: the model "shows its work" before the JSON despite the
        // ONLY-JSON instruction (seen with the IDX lot-rounding prompt).
        let raw = "I'll process each IDX stock transaction using the lot-rounding rules.\n\
            **BUY TLKM**: amount=2012014, price=2870 -> 2012014/2870=701.05 -> qty=700, fee=3014\n\
            ```json\n\
            {\"doc_type\":\"txn_history\",\"entries\":[\n\
              {\"entry_type\":\"buy\",\"symbol\":\"TLKM\",\"quantity\":\"700\",\"price_native\":\"2870\",\"fee_native\":\"3014\",\"amount_native\":\"2012014\",\"currency\":\"IDR\",\"confidence\":0.95}\n\
            ]}\n\
            ```";
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "txn_history");
        assert_eq!(e.entries.len(), 1);
        assert_eq!(e.entries[0].quantity.as_deref(), Some("700"));
        assert_eq!(e.entries[0].fee_native.as_deref(), Some("3014"));
    }

    #[test]
    fn tolerates_prose_around_bare_json() {
        let raw = "Here is the extraction:\n{\"doc_type\":\"txn_history\",\"entries\":[]}\nLet me know if you need anything else.";
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "txn_history");
        assert!(e.entries.is_empty());
    }

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
    fn prose_only_refusal_is_classified_as_refused() {
        // The exact production failure: gpt-4o declined and replied with prose
        // instead of JSON. Must be a Refused (so the caller retries / explains),
        // not a vague NotJson parse error.
        let raw = "I'm sorry, I cannot fulfill this request.";
        match parse_extraction(raw) {
            Err(ExtractError::Refused(snippet)) => assert!(snippet.contains("I cannot fulfill")),
            other => panic!("expected Refused, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_with_braces_is_not_a_refusal() {
        // Has a brace but is broken JSON -> NotJson, not Refused (retrying a
        // truncated/garbled response is a different concern).
        let raw = "{ this is not valid json ";
        assert!(matches!(parse_extraction(raw), Err(ExtractError::NotJson(_))));
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

    #[test]
    fn bibit_amount_only_fund_buys_parse_and_stay_untouched() {
        // Verbatim raw_llm_json from production review items 54-56 (asdc.jpeg,
        // Bibit "Transaksi -> Order" tab): three fund buys with an IDR amount
        // and no units/NAV. normalize_entry must not invent quantity/price
        // (with no price_native it returns early at the missing-price guard,
        // never reaching lot recovery) or flag them.
        let raw = r#"{"doc_type": "txn_history", "entries": [{"entry_type": "buy", "instrument_name": "Pendidikan Noah - Obligasi (Sucorinvest Bond Fund)", "amount_native": "13000000", "currency": "IDR", "note": "Goal: Pendidikan Noah, Obligasi category. Fund: Sucorinvest Bond Fund. Pembelian Berhasil.", "confidence": 0.72}, {"entry_type": "buy", "instrument_name": "Mobil - Pasar Uang (Majoris Pasar Uang Indonesia)", "amount_native": "20000000", "currency": "IDR", "note": "Goal: Mobil, Pasar Uang category. Fund: Majoris Pasar Uang Indonesia. Pembelian Berhasil.", "confidence": 0.72}, {"entry_type": "buy", "instrument_name": "Dana Darurat - Pasar Uang (Majoris Pasar Uang Indonesia)", "amount_native": "3000000", "currency": "IDR", "note": "Goal: Dana Darurat, Pasar Uang category. Fund: Majoris Pasar Uang Indonesia. Pembelian Berhasil.", "confidence": 0.72}]}"#;
        let e = parse_extraction(raw).unwrap();
        assert_eq!(e.doc_type, "txn_history");
        assert_eq!(e.entries.len(), 3);
        for entry in &e.entries {
            assert_eq!(entry.quantity, None, "must not invent units");
            assert_eq!(entry.price_native, None, "must not invent NAV");
            assert!(!entry.force_attention);
        }
        assert_eq!(e.entries[0].amount_native.as_deref(), Some("13000000"));
        assert_eq!(e.entries[1].amount_native.as_deref(), Some("20000000"));
        assert_eq!(e.entries[2].amount_native.as_deref(), Some("3000000"));
    }
}
