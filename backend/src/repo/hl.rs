//! Storage for Hyperliquid perp positions (open snapshot) and closed trades.

use crate::db::Db;
use sqlx::FromRow;

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct HlPosition {
    pub coin: String,
    pub direction: String,
    pub size: String,
    pub entry_px: String,
    pub mark_px: String,
    pub unrealized_pnl: String,
    pub leverage: String,
    pub notional: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, FromRow, serde::Serialize)]
pub struct HlTrade {
    pub external_id: String,
    pub coin: String,
    pub direction: String,
    pub size: String,
    pub entry_px: String,
    pub exit_px: String,
    pub realized_pnl: String,
    pub fee: String,
    pub opened_at: String,
    pub closed_at: String,
    pub leverage: Option<i64>,
    pub confidence: Option<i64>,
    pub timeframe: Option<String>,
    pub profile: Option<String>,
}

/// Replace the entire open-position snapshot in one transaction (positions that
/// have since closed simply disappear).
pub async fn replace_positions(db: &Db, positions: &[HlPosition]) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("DELETE FROM hl_position").execute(&mut *tx).await?;
    for p in positions {
        sqlx::query(
            "INSERT INTO hl_position
             (coin, direction, size, entry_px, mark_px, unrealized_pnl, leverage, notional, updated_at)
             VALUES (?,?,?,?,?,?,?,?,?)",
        )
        .bind(&p.coin).bind(&p.direction).bind(&p.size).bind(&p.entry_px)
        .bind(&p.mark_px).bind(&p.unrealized_pnl).bind(&p.leverage).bind(&p.notional)
        .bind(&p.updated_at)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Insert a closed trade; returns false (no-op) when its `external_id` exists.
pub async fn insert_trade_if_new(db: &Db, t: &HlTrade) -> anyhow::Result<bool> {
    let result = sqlx::query(
        "INSERT OR IGNORE INTO hl_trade
         (external_id, coin, direction, size, entry_px, exit_px, realized_pnl, fee,
          opened_at, closed_at, leverage, confidence, timeframe, profile)
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
    )
    .bind(&t.external_id).bind(&t.coin).bind(&t.direction).bind(&t.size)
    .bind(&t.entry_px).bind(&t.exit_px).bind(&t.realized_pnl).bind(&t.fee)
    .bind(&t.opened_at).bind(&t.closed_at).bind(t.leverage).bind(t.confidence)
    .bind(&t.timeframe).bind(&t.profile)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// List all open positions ordered alphabetically by coin.
pub async fn list_positions(db: &Db) -> anyhow::Result<Vec<HlPosition>> {
    Ok(sqlx::query_as::<_, HlPosition>("SELECT * FROM hl_position ORDER BY coin ASC")
        .fetch_all(db)
        .await?)
}

/// List closed trades newest-first, capped at `limit` rows.
pub async fn list_trades(db: &Db, limit: i64) -> anyhow::Result<Vec<HlTrade>> {
    Ok(sqlx::query_as::<_, HlTrade>(
        "SELECT * FROM hl_trade ORDER BY closed_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_trade(id: &str) -> HlTrade {
        HlTrade {
            external_id: id.into(), coin: "ETH".into(), direction: "long".into(),
            size: "1".into(), entry_px: "2000".into(), exit_px: "2100".into(),
            realized_pnl: "100".into(), fee: "2".into(),
            opened_at: "2026-06-01T00:00:00Z".into(), closed_at: "2026-06-02T00:00:00Z".into(),
            leverage: Some(5), confidence: Some(80), timeframe: Some("4h".into()), profile: Some("moderate".into()),
        }
    }

    #[tokio::test]
    async fn insert_trade_dedups_by_external_id() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        assert!(insert_trade_if_new(&db, &sample_trade("ETH:1:2000")).await.unwrap());
        assert!(!insert_trade_if_new(&db, &sample_trade("ETH:1:2000")).await.unwrap());
        assert_eq!(list_trades(&db, 10).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn replace_positions_swaps_snapshot() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let p = |coin: &str| HlPosition {
            coin: coin.into(), direction: "long".into(), size: "1".into(),
            entry_px: "100".into(), mark_px: "110".into(), unrealized_pnl: "10".into(),
            leverage: "5".into(), notional: "110".into(), updated_at: "2026-06-02T00:00:00Z".into(),
        };
        replace_positions(&db, &[p("ETH"), p("BTC")]).await.unwrap();
        replace_positions(&db, &[p("ETH")]).await.unwrap(); // BTC closed
        let rows = list_positions(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].coin, "ETH");
    }
}
