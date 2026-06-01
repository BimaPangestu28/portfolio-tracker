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
        other => Err(ConnectorError::Config(format!("unsupported kind: {other}"))),
    }
}
