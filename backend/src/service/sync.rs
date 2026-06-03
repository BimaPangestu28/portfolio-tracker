use crate::connectors::ExternalTxn;
use crate::db::Db;
use crate::repo::transactions::NewTransaction;
use std::collections::HashSet;

/// Drop ExternalTxns whose external_id already exists for this source.
pub fn dedup_new(existing: &HashSet<String>, batch: &[ExternalTxn]) -> Vec<ExternalTxn> {
    batch.iter().filter(|t| !existing.contains(&t.external_id)).cloned().collect()
}

/// Map an ExternalTxn to a ledger NewTransaction for a resolved account+instrument.
pub fn external_to_new_txn(t: &ExternalTxn, account_id: i64, instrument_id: i64, source: &str) -> anyhow::Result<NewTransaction> {
    Ok(NewTransaction {
        account_id, instrument_id, txn_type: t.kind.clone(),
        executed_at: chrono::DateTime::parse_from_rfc3339(&t.occurred_at).map_err(|e| anyhow::anyhow!("bad ts: {e}"))?.with_timezone(&chrono::Utc),
        quantity: t.quantity.clone(), price_native: "0".into(), fee_native: t.fee.clone(),
        currency: t.currency.clone(), fx_to_idr: "1".into(), fx_to_usd: "1".into(), note: Some("auto-sync".into()),
        source: Some(source.to_string()), external_id: Some(t.external_id.clone()),
    })
}

use crate::connectors::Connector;
use crate::repo::{connectors, review_items, transactions};
use crate::ingestion::matching::suggest_instrument;

#[derive(Debug, serde::Serialize, Default)]
pub struct SyncReport { pub inserted: usize, pub staged: usize, pub skipped: usize }

/// Run one connector: fetch_new, dedup vs existing ledger external_ids, insert known instruments,
/// stage unknown symbols into review_item. `source` is e.g. "evm:<connector_id>".
pub async fn run_sync(db: &Db, conn_row: &connectors::ConnectorRow, connector: &dyn Connector) -> anyhow::Result<SyncReport> {
    let source = format!("{}:{}", conn_row.kind, conn_row.id);
    let batch = connector.fetch_new(conn_row.cursor.as_deref()).await.map_err(|e| anyhow::anyhow!("connector: {e}"))?;
    let existing = transactions::existing_external_ids(db, &source).await?;
    let fresh = dedup_new(&existing, &batch.txns);
    let mut report = SyncReport::default();
    for t in &fresh {
        match suggest_instrument(db, &t.symbol).await? {
            Some(instrument_id) => {
                let nt = external_to_new_txn(t, conn_row.account_id, instrument_id, &source)?;
                match transactions::create(db, &nt).await {
                    Ok(_) => report.inserted += 1,
                    Err(_) => report.skipped += 1, // unique index collision = already present
                }
            }
            None => {
                let payload = serde_json::json!({
                    "entry_type": t.kind, "symbol": t.symbol, "quantity": t.quantity,
                    "currency": t.currency, "executed_at": t.occurred_at, "confidence": 1.0
                }).to_string();
                review_items::create(db, &review_items::NewReviewItem {
                    batch_id: &source, source_kind: "connector", source_filename: &conn_row.label, source_path: "(connector)",
                    doc_type: "connector_sync", needs_attention: true, payload_json: &payload, raw_llm_json: "{}",
                    suggested_instrument_id: None, suggested_account_id: Some(conn_row.account_id),
                }).await?;
                report.staged += 1;
            }
        }
    }
    if let Some(c) = batch.next_cursor { let _ = connectors::update_cursor(db, conn_row.id, &c).await; }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ext(id: &str) -> ExternalTxn { ExternalTxn { external_id:id.into(), occurred_at:"2026-01-01T00:00:00Z".into(), kind:"deposit".into(), symbol:"ETH".into(), quantity:"1".into(), fee:None, currency:"ETH".into() } }
    #[test]
    fn dedup_removes_existing() {
        let mut existing = HashSet::new(); existing.insert("a".to_string());
        let out = dedup_new(&existing, &[ext("a"), ext("b")]);
        assert_eq!(out.len(), 1); assert_eq!(out[0].external_id, "b");
    }
    #[test]
    fn maps_external_to_ledger_txn() {
        let nt = external_to_new_txn(&ext("z"), 1, 2, "evm").unwrap();
        assert_eq!(nt.txn_type, "deposit");
        assert_eq!(nt.source.as_deref(), Some("evm"));
        assert_eq!(nt.external_id.as_deref(), Some("z"));
        assert_eq!(nt.price_native, "0");
    }
}

#[cfg(test)]
mod db_tests {
    use super::*;
    use crate::repo::instruments;
    use crate::connectors::mock::MockConnector;
    use crate::connectors::ExternalTxn;
    #[tokio::test]
    async fn sync_inserts_known_and_stages_unknown() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount{ name:"W".into(), account_type:"wallet".into(), institution:None, native_currency:"USD".into(), note:None }).await.unwrap();
        instruments::create(&db, &instruments::NewInstrument{ symbol:"ETH".into(), name:"e".into(), instrument_type:"crypto".into(), native_currency:"USD".into(), category_id:None, price_source:"manual".into(), decimals:Some(18), note:None }).await.unwrap();
        let conn = connectors::create(&db, &connectors::NewConnector{ account_id: acc.id, kind:"mock".into(), label:"m".into(), config_json:"{}".into() }).await.unwrap();
        let mc = MockConnector { txns: vec![
            ExternalTxn{ external_id:"k1".into(), occurred_at:"2026-01-01T00:00:00Z".into(), kind:"deposit".into(), symbol:"ETH".into(), quantity:"1".into(), fee:None, currency:"ETH".into() },
            ExternalTxn{ external_id:"u1".into(), occurred_at:"2026-01-01T00:00:00Z".into(), kind:"deposit".into(), symbol:"DOGE".into(), quantity:"5".into(), fee:None, currency:"DOGE".into() },
        ]};
        let r = run_sync(&db, &conn, &mc).await.unwrap();
        assert_eq!(r.inserted, 1); // ETH known
        assert_eq!(r.staged, 1);   // DOGE unknown
        // re-sync is idempotent
        let r2 = run_sync(&db, &connectors::get(&db, conn.id).await.unwrap(), &mc).await.unwrap();
        assert_eq!(r2.inserted, 0);
    }
}
