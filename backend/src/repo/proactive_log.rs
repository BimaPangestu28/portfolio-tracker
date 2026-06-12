//! Dedup log for proactive sends (see migration 0012). Claim-before-send:
//! a successful claim means "this dedup_key is now spoken for, forever".

use crate::db::Db;

/// Claim a dedup key. Returns true exactly once per key (INSERT OR IGNORE);
/// false means it was already claimed — by this run or any earlier one.
pub async fn try_claim(db: &Db, kind: &str, dedup_key: &str) -> anyhow::Result<bool> {
    let now = chrono::Utc::now().to_rfc3339();
    let result = sqlx::query(
        "INSERT OR IGNORE INTO proactive_log (kind, dedup_key, sent_at) VALUES (?, ?, ?)",
    )
    .bind(kind)
    .bind(dedup_key)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn first_claim_wins_second_loses() {
        let db = mem_db().await;
        assert!(try_claim(&db, "briefing", "briefing:2026-06-13").await.unwrap());
        assert!(!try_claim(&db, "briefing", "briefing:2026-06-13").await.unwrap());
    }

    #[tokio::test]
    async fn different_keys_claim_independently() {
        let db = mem_db().await;
        assert!(try_claim(&db, "alert", "mover:BBCA:2026-06-13").await.unwrap());
        assert!(try_claim(&db, "alert", "mover:BTC:2026-06-13").await.unwrap());
        assert!(try_claim(&db, "alert", "milestone:1550000000").await.unwrap());
        assert!(!try_claim(&db, "alert", "milestone:1550000000").await.unwrap());
    }
}
