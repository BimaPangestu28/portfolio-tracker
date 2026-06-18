use super::{Connector, ConnectorError};
use super::evm::EvmConnector;
use super::mock::MockConnector;
use crate::repo::connectors::ConnectorRow;

pub fn build(row: &ConnectorRow) -> Result<Box<dyn Connector>, ConnectorError> {
    let cfg: serde_json::Value = serde_json::from_str(&row.config_json)
        .map_err(|e| ConnectorError::Config(e.to_string()))?;
    match row.kind.as_str() {
        "evm_wallet" => {
            let address = cfg.get("address")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing address".into()))?
                .to_string();
            let base_url = cfg.get("base_url")
                .and_then(|v| v.as_str())
                .unwrap_or("https://api.etherscan.io/api")
                .to_string();
            let api_key = cfg.get("api_key").and_then(|v| v.as_str()).map(String::from);
            let native = cfg.get("native_symbol")
                .and_then(|v| v.as_str())
                .unwrap_or("ETH")
                .to_string();
            Ok(Box::new(EvmConnector::new(address, base_url, api_key, native)))
        }
        "hyperliquid" => {
            let base_url = cfg.get("base_url")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing base_url".into()))?
                .to_string();
            let token = cfg.get("token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing token".into()))?
                .to_string();
            Ok(Box::new(crate::connectors::hyperliquid::HyperliquidConnector::new(base_url, token)))
        }
        "mock" => Ok(Box::new(MockConnector { txns: vec![] })),
        other => Err(ConnectorError::Config(format!("unsupported kind: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_row(kind: &str, config_json: &str) -> ConnectorRow {
        ConnectorRow {
            id: 1,
            account_id: 1,
            kind: kind.to_string(),
            label: "test".to_string(),
            config_json: config_json.to_string(),
            cursor: None,
            last_synced_at: None,
            enabled: 1,
            created_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn builds_hyperliquid_connector_from_config() {
        let row = make_row(
            "hyperliquid",
            r#"{"base_url":"https://bot.example.com","token":"secret-token"}"#,
        );
        assert!(build(&row).is_ok());
    }

    #[test]
    fn hyperliquid_missing_base_url_returns_config_error() {
        let row = make_row("hyperliquid", r#"{"token":"secret-token"}"#);
        match build(&row) {
            Err(ConnectorError::Config(msg)) => {
                assert!(msg.contains("missing base_url"), "unexpected: {msg}");
            }
            _ => panic!("expected Config error"),
        }
    }

    #[test]
    fn hyperliquid_missing_token_returns_config_error() {
        let row = make_row("hyperliquid", r#"{"base_url":"https://bot.example.com"}"#);
        match build(&row) {
            Err(ConnectorError::Config(msg)) => {
                assert!(msg.contains("missing token"), "unexpected: {msg}");
            }
            _ => panic!("expected Config error"),
        }
    }

    #[test]
    fn unsupported_kind_returns_config_error() {
        let row = make_row("unknown_kind", r#"{}"#);
        match build(&row) {
            Err(ConnectorError::Config(msg)) => {
                assert!(msg.contains("unsupported kind"), "unexpected: {msg}");
            }
            _ => panic!("expected Config error"),
        }
    }
}
