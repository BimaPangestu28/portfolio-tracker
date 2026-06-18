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
        "mock" => Ok(Box::new(MockConnector { txns: vec![] })),
        "hyperliquid" => {
            let wallet = cfg
                .get("wallet")
                .and_then(|v| v.as_str())
                .ok_or_else(|| ConnectorError::Config("missing wallet".into()))?
                .to_string();
            let network = cfg
                .get("network")
                .and_then(|v| v.as_str())
                .unwrap_or("mainnet")
                .to_string();
            Ok(Box::new(crate::connectors::hyperliquid::HyperliquidConnector::new(wallet, network)))
        }
        other => Err(ConnectorError::Config(format!("unsupported kind: {other}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_connector_row(kind: &str, config_json: &str) -> ConnectorRow {
        ConnectorRow {
            id: 1,
            account_id: 1,
            kind: kind.into(),
            label: "test".into(),
            config_json: config_json.into(),
            cursor: None,
            last_synced_at: None,
            enabled: 1,
            created_at: "2024-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn builds_hyperliquid_connector_from_config() {
        let row = make_connector_row(
            "hyperliquid",
            r#"{"wallet":"0xabc","network":"testnet"}"#,
        );
        assert!(build(&row).is_ok());
    }
}
