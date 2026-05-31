use super::PriceError;
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct FxClient { base: String, client: reqwest::Client }

impl FxClient {
    pub fn new() -> Self {
        Self { base: "https://api.exchangerate.host".into(), client: reqwest::Client::new() }
    }
    pub async fn usd_to_idr(&self) -> Result<Decimal, PriceError> {
        let url = format!("{}/latest?base=USD&symbols=IDR", self.base);
        let resp = self.client.get(&url).send().await.map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_fx(&body, "IDR")
    }
}

pub fn parse_fx(body: &serde_json::Value, symbol: &str) -> Result<Decimal, PriceError> {
    let v = body.get("rates").and_then(|r| r.get(symbol))
        .ok_or_else(|| PriceError::NotFound(symbol.into()))?;
    Decimal::from_str(v.to_string().trim_matches('"')).map_err(|e| PriceError::Parse(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    #[test]
    fn parses_fx_rate() {
        let body = serde_json::json!({ "rates": { "IDR": 16250.0 } });
        assert_eq!(parse_fx(&body, "IDR").unwrap(), dec!(16250));
    }
}
