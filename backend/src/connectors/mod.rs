pub mod evm;
pub mod factory;
pub mod hyperliquid;
pub mod mock;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq)]
pub struct ExternalTxn {
    pub external_id: String,
    pub occurred_at: String,   // rfc3339
    pub kind: String,          // deposit | withdrawal | buy | sell
    pub symbol: String,
    pub quantity: String,
    pub fee: Option<String>,
    pub currency: String,
}

#[derive(Debug)]
pub struct SyncBatch {
    pub txns: Vec<ExternalTxn>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ConnectorError {
    #[error("http error: {0}")] Http(String),
    #[error("parse error: {0}")] Parse(String),
    #[error("config error: {0}")] Config(String),
}

#[async_trait]
pub trait Connector: Send + Sync {
    async fn fetch_new(&self, cursor: Option<&str>) -> Result<SyncBatch, ConnectorError>;
}
