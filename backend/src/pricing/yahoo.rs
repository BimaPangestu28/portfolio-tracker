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
}
