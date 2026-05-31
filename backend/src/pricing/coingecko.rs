use super::{PriceError, PriceProvider, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct CoinGecko { base: String, client: reqwest::Client }

impl CoinGecko {
    pub fn new() -> Self {
        Self { base: "https://api.coingecko.com/api/v3".into(), client: reqwest::Client::new() }
    }
}

/// Pure parser: CoinGecko simple/price JSON -> Quote. Unit-tested without network.
pub fn parse_simple_price(body: &serde_json::Value, ext_id: &str, vs: &str) -> Result<Quote, PriceError> {
    let v = body.get(ext_id).and_then(|o| o.get(vs))
        .ok_or_else(|| PriceError::NotFound(ext_id.into()))?;
    let s = v.to_string();
    let price = Decimal::from_str(s.trim_matches('"')).map_err(|e| PriceError::Parse(e.to_string()))?;
    Ok(Quote { price, currency: vs.to_uppercase() })
}

#[async_trait::async_trait]
impl PriceProvider for CoinGecko {
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError> {
        let url = format!("{}/simple/price?ids={}&vs_currencies=usd", self.base, ext_id);
        let resp = self.client.get(&url).send().await.map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_simple_price(&body, ext_id, "usd")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;
    #[test]
    fn parses_simple_price() {
        let body = serde_json::json!({ "bitcoin": { "usd": 67000.5 } });
        let q = parse_simple_price(&body, "bitcoin", "usd").unwrap();
        assert_eq!(q.price, dec!(67000.5));
        assert_eq!(q.currency, "USD");
    }
    #[test]
    fn missing_id_is_not_found() {
        let body = serde_json::json!({});
        assert!(matches!(parse_simple_price(&body, "bitcoin", "usd"), Err(PriceError::NotFound(_))));
    }
}
