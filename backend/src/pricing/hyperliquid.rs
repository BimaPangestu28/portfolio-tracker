use crate::pricing::{PriceError, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

/// Read-only client for the agent-hyperliquid bot API.
pub struct BotClient {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl BotClient {
    /// Built only when both env vars are set; `None` disables the integration.
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var("HYPERLIQUID_API_URL").ok().filter(|s| !s.is_empty())?;
        let token = std::env::var("HYPERLIQUID_API_TOKEN").ok().filter(|s| !s.is_empty())?;
        Some(Self { base_url, token, client: reqwest::Client::new() })
    }

    /// Total account equity (USD) from `GET /balance`.
    pub async fn account_equity(&self) -> Result<Quote, PriceError> {
        let url = format!("{}/balance", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            return Err(PriceError::Http(format!("hyperliquid bot /balance status {status}")));
        }
        let json: serde_json::Value =
            resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_balance(&json)
    }
}

/// Pull `equity_usd` out of a `/balance` response.
pub fn parse_balance(body: &serde_json::Value) -> Result<Quote, PriceError> {
    let raw = body
        .get("equity_usd")
        .ok_or_else(|| PriceError::Parse("missing equity_usd".into()))?;
    // Accept either a JSON number or a numeric string.
    let price = match raw {
        serde_json::Value::Number(n) => Decimal::from_str(&n.to_string()),
        serde_json::Value::String(s) => Decimal::from_str(s),
        _ => return Err(PriceError::Parse("equity_usd not numeric".into())),
    }
    .map_err(|e| PriceError::Parse(format!("bad equity_usd: {e}")))?;
    Ok(Quote { price, currency: "USD".into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_equity_from_balance_response() {
        let body = serde_json::json!({ "equity_usd": 1234.56, "as_of_ms": 1700000000000_i64 });
        let q = parse_balance(&body).unwrap();
        assert_eq!(q.price, dec!(1234.56));
        assert_eq!(q.currency, "USD");
    }

    #[test]
    fn missing_equity_is_parse_error() {
        let body = serde_json::json!({ "as_of_ms": 1 });
        assert!(matches!(parse_balance(&body).unwrap_err(), PriceError::Parse(_)));
    }
}
