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
                    let _ = crate::repo::snapshots::upsert(
                        &db,
                        &today,
                        &s.net_worth_idr.to_string(),
                        &s.net_worth_usd.to_string(),
                        &breakdown,
                        Some(&s.total_unrealized_price_pnl_idr.to_string()),
                        Some(&s.total_unrealized_fx_pnl_idr.to_string()),
                    ).await;
                }
                Err(e) => tracing::warn!("snapshot build error: {e}"),
            }
            match crate::repo::connectors::list_enabled(&db).await {
                Ok(rows) => for row in rows {
                    if let Ok(conn) = crate::connectors::factory::build(&row) {
                        if let Err(e) = crate::service::sync::run_sync(&db, &row, conn.as_ref()).await {
                            tracing::warn!("connector {} sync failed: {e}", row.id);
                        }
                    }
                },
                Err(e) => tracing::warn!("list connectors failed: {e}"),
            }
            if let Err(e) = crate::service::hyperliquid_sync::run(&db).await {
                tracing::warn!("hyperliquid perp sync failed: {e:#}");
            }
            tokio::time::sleep(interval).await;
        }
    });
}
