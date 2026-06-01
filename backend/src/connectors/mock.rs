use super::{Connector, ConnectorError, ExternalTxn, SyncBatch};
use async_trait::async_trait;

/// Returns a fixed batch — used for tests and scheduler smoke.
pub struct MockConnector { pub txns: Vec<ExternalTxn> }

#[async_trait]
impl Connector for MockConnector {
    async fn fetch_new(&self, _cursor: Option<&str>) -> Result<SyncBatch, ConnectorError> {
        Ok(SyncBatch { txns: self.txns.clone(), next_cursor: Some("done".into()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn mock_returns_its_txns() {
        let c = MockConnector { txns: vec![ ExternalTxn { external_id:"x1".into(), occurred_at:"2026-01-01T00:00:00Z".into(), kind:"deposit".into(), symbol:"ETH".into(), quantity:"1".into(), fee:None, currency:"ETH".into() } ] };
        let b = c.fetch_new(None).await.unwrap();
        assert_eq!(b.txns.len(), 1);
        assert_eq!(b.next_cursor.as_deref(), Some("done"));
    }
}
