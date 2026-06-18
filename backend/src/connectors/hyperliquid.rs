use crate::connectors::{Connector, ConnectorError, ExternalTxn, SyncBatch};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};

pub struct HyperliquidConnector {
    wallet: String,
    base: String,
    client: reqwest::Client,
}

impl HyperliquidConnector {
    pub fn new(wallet: String, network: String) -> Self {
        let base = if network == "testnet" {
            "https://api.hyperliquid-testnet.xyz"
        } else {
            "https://api.hyperliquid.xyz"
        }
        .to_string();
        Self { wallet, base, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl Connector for HyperliquidConnector {
    async fn fetch_new(&self, _cursor: Option<&str>) -> Result<SyncBatch, ConnectorError> {
        let url = format!("{}/info", self.base);
        let body = serde_json::json!({
            "type": "userNonFundingLedgerUpdates",
            "user": self.wallet,
        });
        let resp = self
            .client
            .post(&url)
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| ConnectorError::Http(e.to_string()))?;
        let json: serde_json::Value =
            resp.json().await.map_err(|e| ConnectorError::Parse(e.to_string()))?;
        let txns = parse_ledger(&json, &self.wallet)?;
        Ok(SyncBatch { txns, next_cursor: None })
    }
}

/// Map non-funding ledger updates to deposit/withdrawal ExternalTxns (USDC only).
pub fn parse_ledger(body: &serde_json::Value, _wallet: &str) -> Result<Vec<ExternalTxn>, ConnectorError> {
    let rows = body
        .as_array()
        .ok_or_else(|| ConnectorError::Parse("expected ledger array".into()))?;
    let mut out = Vec::new();
    for row in rows {
        let delta = match row.get("delta") {
            Some(d) => d,
            None => continue,
        };
        let kind = match delta.get("type").and_then(|v| v.as_str()) {
            Some("deposit") => "deposit",
            Some("withdraw") => "withdrawal",
            _ => continue,
        };
        let usdc = match delta.get("usdc").and_then(|v| v.as_str()) {
            Some(q) => q.to_string(),
            None => continue,
        };
        let time_ms = row.get("time").and_then(|v| v.as_i64()).unwrap_or(0);
        let occurred_at = Utc
            .timestamp_millis_opt(time_ms)
            .single()
            .unwrap_or_else(Utc::now)
            .to_rfc3339();
        let hash = row.get("hash").and_then(|v| v.as_str()).unwrap_or("").to_string();
        out.push(ExternalTxn {
            external_id: format!("{hash}:{kind}"),
            occurred_at,
            kind: kind.to_string(),
            symbol: "USDC".into(),
            quantity: usdc,
            fee: None,
            currency: "USD".into(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_usdc_deposit_and_withdrawal() {
        // Shape: userNonFundingLedgerUpdates → [{ time, hash, delta: { type, usdc } }]
        let body = serde_json::json!([
            { "time": 1700000000000_i64, "hash": "0xa",
              "delta": { "type": "deposit", "usdc": "500.0" } },
            { "time": 1700000100000_i64, "hash": "0xb",
              "delta": { "type": "withdraw", "usdc": "200.0" } },
            { "time": 1700000200000_i64, "hash": "0xc",
              "delta": { "type": "liquidation", "usdc": "1.0" } }
        ]);
        let out = parse_ledger(&body, "0xme").unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].quantity, "500.0");
        assert_eq!(out[0].symbol, "USDC");
        assert_eq!(out[0].currency, "USD");
        assert_eq!(out[1].kind, "withdrawal");
    }
}
