//! The Upwork transaction source. `UpworkClient` is the seam: the real
//! `HttpUpwork` calls the GraphQL API; tests and pre-approval development use a
//! fake. Returns a cursor for incremental paging.

use crate::upwork::UpworkTransaction;
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("http error: {0}")] Http(String),
    #[error("parse error: {0}")] Parse(String),
}

#[derive(Debug, Default)]
pub struct TransactionBatch {
    pub txns: Vec<UpworkTransaction>,
    pub next_cursor: Option<String>,
}

#[async_trait]
pub trait UpworkClient: Send + Sync {
    /// Fetch transactions newer than `cursor` (None = from the beginning).
    async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError>;
}

const GRAPHQL_ENDPOINT: &str = "https://api.upwork.com/graphql";

/// Live client. The GraphQL query/field mapping is exercised by the gated live
/// smoke test in `engine.rs`; adjust field paths once the approved schema is
/// confirmed.
pub struct HttpUpwork {
    access_token: String,
    http: reqwest::Client,
}

impl HttpUpwork {
    pub fn new(access_token: String) -> Self {
        Self { access_token, http: reqwest::Client::new() }
    }
}

#[async_trait]
impl UpworkClient for HttpUpwork {
    async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError> {
        let query = r#"
            query($after: String) {
              transactionHistory(after: $after) {
                edges {
                  cursor
                  node { reference dateTime type amount { rawValue currency } contractTitle }
                }
                pageInfo { endCursor hasNextPage }
              }
            }"#;
        let body = serde_json::json!({ "query": query, "variables": { "after": cursor } });
        let resp = self.http
            .post(GRAPHQL_ENDPOINT)
            .bearer_auth(&self.access_token)
            .json(&body)
            .send()
            .await
            .map_err(|e| ClientError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("{}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| ClientError::Parse(e.to_string()))?;
        let edges = v["data"]["transactionHistory"]["edges"]
            .as_array()
            .ok_or_else(|| ClientError::Parse("missing transactionHistory.edges".into()))?;
        let mut txns = Vec::with_capacity(edges.len());
        for e in edges {
            let node = &e["node"];
            txns.push(UpworkTransaction {
                external_id: node["reference"].as_str().unwrap_or_default().to_string(),
                date: node["dateTime"].as_str().unwrap_or_default().to_string(),
                kind: node["type"].as_str().unwrap_or_default().to_string(),
                amount: node["amount"]["rawValue"].as_str().unwrap_or("0").to_string(),
                currency: node["amount"]["currency"].as_str().unwrap_or("USD").to_string(),
                contract: node["contractTitle"].as_str().map(|s| s.to_string()),
            });
        }
        let next_cursor = v["data"]["transactionHistory"]["pageInfo"]["endCursor"]
            .as_str()
            .map(|s| s.to_string());
        Ok(TransactionBatch { txns, next_cursor })
    }
}

#[cfg(test)]
pub mod testkit {
    use super::*;
    use std::sync::Mutex;

    /// In-memory client returning a preset batch; records the cursor it was called with.
    pub struct FakeUpwork {
        pub batch: Mutex<TransactionBatch>,
        pub seen_cursor: Mutex<Option<String>>,
    }
    impl FakeUpwork {
        pub fn with(txns: Vec<UpworkTransaction>, next_cursor: Option<String>) -> Self {
            Self { batch: Mutex::new(TransactionBatch { txns, next_cursor }), seen_cursor: Mutex::new(None) }
        }
    }
    #[async_trait]
    impl UpworkClient for FakeUpwork {
        async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError> {
            *self.seen_cursor.lock().unwrap() = cursor.map(|c| c.to_string());
            let b = self.batch.lock().unwrap();
            Ok(TransactionBatch { txns: b.txns.clone(), next_cursor: b.next_cursor.clone() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testkit::FakeUpwork;
    use super::*;

    #[tokio::test]
    async fn fake_returns_preset_batch_and_records_cursor() {
        let fake = FakeUpwork::with(
            vec![UpworkTransaction {
                external_id: "t1".into(), date: "2026-06-10".into(), kind: "Hourly".into(),
                amount: "120.00".into(), currency: "USD".into(), contract: None,
            }],
            Some("cur-2".into()),
        );
        let batch = fake.fetch_transactions(Some("cur-1")).await.unwrap();
        assert_eq!(batch.txns.len(), 1);
        assert_eq!(batch.next_cursor.as_deref(), Some("cur-2"));
        assert_eq!(fake.seen_cursor.lock().unwrap().as_deref(), Some("cur-1"));
    }
}
