use crate::pricing::{PriceError, Quote};
use rust_decimal::Decimal;
use std::str::FromStr;

pub struct Hyperliquid {
    base: String,
    client: reqwest::Client,
}

impl Hyperliquid {
    pub fn new(network: &str) -> Self {
        let base = match network {
            "testnet" => "https://api.hyperliquid-testnet.xyz",
            _ => "https://api.hyperliquid.xyz",
        }
        .to_string();
        Self { base, client: reqwest::Client::new() }
    }

    /// Total account equity (USD) for `wallet` via clearinghouseState.
    pub async fn account_equity(&self, wallet: &str) -> Result<Quote, PriceError> {
        let url = format!("{}/info", self.base);
        let body = serde_json::json!({ "type": "clearinghouseState", "user": wallet });
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| PriceError::Http(e.to_string()))?;
        let status = resp.status().as_u16();
        if status >= 400 {
            return Err(PriceError::Http(format!("hyperliquid info status {status}")));
        }
        let json: serde_json::Value =
            resp.json().await.map_err(|e| PriceError::Parse(e.to_string()))?;
        parse_account_equity(&json)
    }
}

/// Pull `marginSummary.accountValue` out of a clearinghouseState response.
pub fn parse_account_equity(body: &serde_json::Value) -> Result<Quote, PriceError> {
    let raw = body
        .get("marginSummary")
        .and_then(|m| m.get("accountValue"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| PriceError::Parse("missing marginSummary.accountValue".into()))?;
    let price = Decimal::from_str(raw)
        .map_err(|e| PriceError::Parse(format!("bad accountValue '{raw}': {e}")))?;
    Ok(Quote { price, currency: "USD".into() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn parses_account_value_from_margin_summary() {
        let body = serde_json::json!({
            "marginSummary": { "accountValue": "1234.56", "totalNtlPos": "0.0" },
            "assetPositions": []
        });
        let q = parse_account_equity(&body).unwrap();
        assert_eq!(q.price, dec!(1234.56));
        assert_eq!(q.currency, "USD");
    }

    #[test]
    fn missing_account_value_is_parse_error() {
        let body = serde_json::json!({ "marginSummary": {} });
        let err = parse_account_equity(&body).unwrap_err();
        assert!(matches!(err, PriceError::Parse(_)));
    }
}
