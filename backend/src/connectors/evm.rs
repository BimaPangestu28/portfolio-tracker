use super::{Connector, ConnectorError, ExternalTxn, SyncBatch};
use async_trait::async_trait;
use rust_decimal::prelude::*;
use rust_decimal::Decimal;

pub struct EvmConnector {
    pub address: String,
    pub base_url: String,           // e.g. https://api.etherscan.io/api
    pub api_key: Option<String>,
    pub native_symbol: String,      // "ETH"
    pub client: reqwest::Client,
}

/// Convert an integer string of base units + decimals to a decimal-string quantity.
fn from_base_units(raw: &str, decimals: u32) -> String {
    let v = Decimal::from_str(raw).unwrap_or(Decimal::ZERO);
    let scale = Decimal::from(10u64).powu(decimals as u64);
    (v / scale).normalize().to_string()
}

/// Parse Etherscan `txlist` (native transfers) for `address` into ExternalTxn.
pub fn parse_txlist(json: &serde_json::Value, address: &str, native_symbol: &str) -> Result<Vec<ExternalTxn>, ConnectorError> {
    let arr = json.get("result").and_then(|r| r.as_array()).ok_or_else(|| ConnectorError::Parse("no result array".into()))?;
    let addr = address.to_lowercase();
    let mut out = Vec::new();
    for t in arr {
        let hash = t.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string();
        let value = t.get("value").and_then(|v| v.as_str()).unwrap_or("0");
        if value == "0" { continue; }
        let to = t.get("to").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let from = t.get("from").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let ts = t.get("timeStamp").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let kind = if to == addr { "deposit" } else if from == addr { "withdrawal" } else { continue };
        out.push(ExternalTxn {
            external_id: format!("{hash}:native"),
            occurred_at: chrono::DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()).unwrap_or_default(),
            kind: kind.into(), symbol: native_symbol.to_string(),
            quantity: from_base_units(value, 18), fee: None, currency: native_symbol.to_string(),
        });
    }
    Ok(out)
}

/// Parse Etherscan `tokentx` (ERC-20 transfers) for `address`.
pub fn parse_tokentx(json: &serde_json::Value, address: &str) -> Result<Vec<ExternalTxn>, ConnectorError> {
    let arr = json.get("result").and_then(|r| r.as_array()).ok_or_else(|| ConnectorError::Parse("no result array".into()))?;
    let addr = address.to_lowercase();
    let mut out = Vec::new();
    for t in arr {
        let hash = t.get("hash").and_then(|h| h.as_str()).unwrap_or("").to_string();
        let symbol = t.get("tokenSymbol").and_then(|v| v.as_str()).unwrap_or("").to_string();
        let decimals: u32 = t.get("tokenDecimal").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()).unwrap_or(18);
        let value = t.get("value").and_then(|v| v.as_str()).unwrap_or("0");
        let to = t.get("to").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let from = t.get("from").and_then(|v| v.as_str()).unwrap_or("").to_lowercase();
        let ts = t.get("timeStamp").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
        let kind = if to == addr { "deposit" } else if from == addr { "withdrawal" } else { continue };
        out.push(ExternalTxn {
            external_id: format!("{hash}:{symbol}"),
            occurred_at: chrono::DateTime::from_timestamp(ts, 0).map(|d| d.to_rfc3339()).unwrap_or_default(),
            kind: kind.into(), symbol, quantity: from_base_units(value, decimals), fee: None, currency: "".into(),
        });
    }
    Ok(out)
}

impl EvmConnector {
    pub fn new(address: String, base_url: String, api_key: Option<String>, native_symbol: String) -> Self {
        Self { address, base_url, api_key, native_symbol, client: reqwest::Client::new() }
    }
    async fn fetch(&self, action: &str) -> Result<serde_json::Value, ConnectorError> {
        let key = self.api_key.clone().unwrap_or_default();
        let url = format!("{}?module=account&action={action}&address={}&sort=asc&apikey={key}", self.base_url, self.address);
        let resp = self.client.get(&url).send().await.map_err(|e| ConnectorError::Http(e.to_string()))?;
        resp.json().await.map_err(|e| ConnectorError::Parse(e.to_string()))
    }
}

#[async_trait]
impl Connector for EvmConnector {
    async fn fetch_new(&self, _cursor: Option<&str>) -> Result<SyncBatch, ConnectorError> {
        let mut txns = Vec::new();
        let native = self.fetch("txlist").await?;
        txns.extend(parse_txlist(&native, &self.address, &self.native_symbol)?);
        let tokens = self.fetch("tokentx").await?;
        txns.extend(parse_tokentx(&tokens, &self.address)?);
        Ok(SyncBatch { txns, next_cursor: None })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_native_deposit_and_withdrawal() {
        let me = "0xabc";
        let json = serde_json::json!({"result":[
            {"hash":"0x1","from":"0xother","to":"0xabc","value":"1000000000000000000","timeStamp":"1700000000"},
            {"hash":"0x2","from":"0xabc","to":"0xother","value":"500000000000000000","timeStamp":"1700000100"},
            {"hash":"0x3","from":"0xx","to":"0xy","value":"0","timeStamp":"1700000200"}
        ]});
        let out = parse_txlist(&json, me, "ETH").unwrap();
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].kind, "deposit");
        assert_eq!(out[0].quantity, "1");
        assert_eq!(out[1].kind, "withdrawal");
        assert_eq!(out[1].quantity, "0.5");
        assert_eq!(out[0].external_id, "0x1:native");
    }
    #[test]
    fn parses_erc20_with_token_decimals() {
        let json = serde_json::json!({"result":[
            {"hash":"0xa","from":"0xother","to":"0xabc","value":"1500000","tokenSymbol":"USDC","tokenDecimal":"6","timeStamp":"1700000000"}
        ]});
        let out = parse_tokentx(&json, "0xabc").unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].symbol, "USDC");
        assert_eq!(out[0].quantity, "1.5");
        assert_eq!(out[0].kind, "deposit");
    }
    #[test]
    fn missing_result_errors() {
        assert!(parse_txlist(&serde_json::json!({}), "0xabc", "ETH").is_err());
    }
}
