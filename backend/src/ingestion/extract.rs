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
