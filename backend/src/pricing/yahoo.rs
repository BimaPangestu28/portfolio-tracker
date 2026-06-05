use super::{PriceError, PriceProvider, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct Yahoo {
    base: String,
    client: reqwest::Client,
}

impl Yahoo {
    pub fn new() -> Self {
        Self {
            base: "https://query1.finance.yahoo.com/v8/finance/chart".into(),
            client: reqwest::Client::new(),
        }
    }
}

/// Parse Yahoo chart JSON -> Quote using meta.regularMarketPrice + meta.currency.
pub fn parse_chart(body: &serde_json::Value) -> Result<Quote, PriceError> {
    let meta = body
        .pointer("/chart/result/0/meta")
        .ok_or_else(|| PriceError::NotFound("meta".into()))?;
    let price = meta
        .get("regularMarketPrice")
        .ok_or_else(|| PriceError::NotFound("regularMarketPrice".into()))?;
    let currency = meta
        .get("currency")
        .and_then(|c| c.as_str())
        .unwrap_or("USD")
        .to_string();
    let price = Decimal::from_str(price.to_string().trim_matches('"'))
        .map_err(|e| PriceError::Parse(e.to_string()))?;
    Ok(Quote { price, currency })
}

/// Compute the period return from a Yahoo chart response with daily closes.
/// Nulls (market holidays) are skipped; needs at least two valid closes.
pub fn parse_range_return(body: &serde_json::Value) -> Result<f64, PriceError> {
    let closes = body
        .pointer("/chart/result/0/indicators/quote/0/close")
        .and_then(|c| c.as_array())
        .ok_or_else(|| PriceError::NotFound("close series".into()))?;
    let valid: Vec<f64> = closes.iter().filter_map(|v| v.as_f64()).collect();
    match (valid.first(), valid.last()) {
        (Some(first), Some(last)) if valid.len() >= 2 && *first != 0.0 => Ok(last / first - 1.0),
        _ => Err(PriceError::NotFound("not enough closes".into())),
    }
}

impl Yahoo {
    /// Period return for a symbol over a Yahoo range ("1y", "6mo", "3mo", ...).
    pub async fn range_return(&self, ext_id: &str, range: &str) -> Result<f64, PriceError> {
        let url = format!("{}/{}?range={}&interval=1d", self.base, ext_id, range);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_range_return(&body)
    }
}

#[async_trait::async_trait]
impl PriceProvider for Yahoo {
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError> {
        // ext_id is a Yahoo symbol, e.g. "BBCA.JK" (IDX) or "VOO" (US ETF).
        let url = format!("{}/{}", self.base, ext_id);
        let resp = self
            .client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_chart(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_chart_price_and_currency() {
        let body = serde_json::json!({
            "chart": {
                "result": [
                    { "meta": { "regularMarketPrice": 9500, "currency": "IDR" } }
                ]
            }
        });
        let q = parse_chart(&body).unwrap();
        assert_eq!(q.price, dec!(9500));
        assert_eq!(q.currency, "IDR");
    }

    #[test]
    fn missing_meta_is_not_found() {
        let body = serde_json::json!({ "chart": { "result": [] } });
        assert!(matches!(parse_chart(&body), Err(PriceError::NotFound(_))));
    }

    #[test]
    fn range_return_skips_nulls_and_divides_endpoints() {
        let body = serde_json::json!({
            "chart": { "result": [ { "indicators": { "quote": [
                { "close": [7000.0, null, 7100.0, 7350.0] }
            ] } } ] }
        });
        let r = parse_range_return(&body).unwrap();
        assert!((r - 0.05).abs() < 1e-9, "{r}");
    }

    #[test]
    fn range_return_rejects_short_series() {
        let body = serde_json::json!({
            "chart": { "result": [ { "indicators": { "quote": [ { "close": [7000.0] } ] } } ] }
        });
        assert!(matches!(parse_range_return(&body), Err(PriceError::NotFound(_))));
    }
}
