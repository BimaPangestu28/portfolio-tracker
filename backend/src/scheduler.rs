use crate::db::Db;
use std::time::Duration;

/// Background loop: refresh prices, then snapshot valuation, every `interval`.
pub fn spawn(db: Db, interval: Duration) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = crate::pricing::service::refresh_all(&db).await {
                tracing::warn!("price refresh error: {e}");
            }
            match crate::service::portfolio::build_summary(&db).await {
                Ok(s) => {
                    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
                    let breakdown = serde_json::to_string(&s.allocation).unwrap_or_else(|_| "[]".into());
                    let _ = crate::repo::snapshots::upsert(&db, &today, &s.net_worth_idr.to_string(), &s.net_worth_usd.to_string(), &breakdown).await;
                }
                Err(e) => tracing::warn!("snapshot build error: {e}"),
            }
            tokio::time::sleep(interval).await;
        }
    });
}
