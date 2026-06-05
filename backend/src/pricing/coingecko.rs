use super::{PriceError, PriceProvider, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct CoinGecko { base: String, client: reqwest::Client }

impl CoinGecko {
    pub fn new() -> Self {
        Self { base: "https://api.coingecko.com/api/v3".into(), client: reqwest::Client::new() }
    }
}

/// Map an instrument's native currency to a CoinGecko vs_currency.
/// CoinGecko supports a fixed set; anything we don't recognize falls back to USD.
pub fn vs_currency(native: &str) -> &'static str {
    match native.to_ascii_lowercase().as_str() {
        "idr" => "idr",
        "usd" => "usd",
        "eur" => "eur",
        "sgd" => "sgd",
        _ => "usd",
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

impl CoinGecko {
    /// Latest quote in the given vs currency (see `vs_currency`).
    pub async fn latest_in(&self, ext_id: &str, vs: &str) -> Result<Quote, PriceError> {
        let url = format!("{}/simple/price?ids={}&vs_currencies={}", self.base, ext_id, vs);
        let resp = self.client.get(&url).send().await.map_err(|e| PriceError::Http(e.to_string()))?;
        let body: serde_json::Value = resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_simple_price(&body, ext_id, vs)
    }
}

#[async_trait::async_trait]
impl PriceProvider for CoinGecko {
    async fn latest(&self, ext_id: &str) -> Result<Quote, PriceError> {
        self.latest_in(ext_id, "usd").await
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
    fn parses_simple_price_in_idr() {
        // IDR-native instruments (local exchanges) need the IDR quote directly.
        let body = serde_json::json!({ "bitcoin": { "idr": 1182000000.0_f64 } });
        let q = parse_simple_price(&body, "bitcoin", "idr").unwrap();
        assert_eq!(q.price, dec!(1182000000));
        assert_eq!(q.currency, "IDR");
    }

    #[test]
    fn vs_currency_is_lowercased_and_defaults_to_usd() {
        assert_eq!(vs_currency("IDR"), "idr");
        assert_eq!(vs_currency("usd"), "usd");
        // CoinGecko supports a fixed set; anything unknown falls back to USD.
        assert_eq!(vs_currency("XYZ"), "usd");
    }
    #[test]
    fn missing_id_is_not_found() {
        let body = serde_json::json!({});
        assert!(matches!(parse_simple_price(&body, "bitcoin", "usd"), Err(PriceError::NotFound(_))));
    }
}
