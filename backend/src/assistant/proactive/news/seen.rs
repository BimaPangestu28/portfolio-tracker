//! Recently-seen URLs so the same article isn't surfaced day after day.

use super::{norm_url, Article};
use crate::db::Db;
use sha2::{Digest, Sha256};

fn hash(url: &str) -> String {
    let mut h = Sha256::new();
    h.update(norm_url(url).as_bytes());
    format!("{:x}", h.finalize())
}

/// Drop candidates whose URL was already seen.
pub async fn filter_unseen(db: &Db, candidates: Vec<Article>) -> anyhow::Result<Vec<Article>> {
    let mut out = Vec::new();
    for a in candidates {
        let exists: Option<(String,)> =
            sqlx::query_as("SELECT url_hash FROM news_seen WHERE url_hash = ?")
                .bind(hash(&a.url))
                .fetch_optional(db)
                .await?;
        if exists.is_none() {
            out.push(a);
        }
    }
    Ok(out)
}

/// Record URLs as seen (insert-or-ignore), stamped now (UTC RFC3339).
pub async fn mark(db: &Db, urls: &[String], now_utc: &str) -> anyhow::Result<()> {
    for url in urls {
        sqlx::query("INSERT OR IGNORE INTO news_seen (url_hash, url, first_seen) VALUES (?, ?, ?)")
            .bind(hash(url))
            .bind(url)
            .bind(now_utc)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Delete seen rows older than `cutoff_utc` (RFC3339).
pub async fn prune(db: &Db, cutoff_utc: &str) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM news_seen WHERE first_seen < ?")
        .bind(cutoff_utc)
        .execute(db)
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(url: &str) -> Article {
        Article { title: "t".into(), url: url.into(), source: "s".into(), score: 0, published_at: None, relevance: 1, hn_object_id: None }
    }

    #[tokio::test]
    async fn mark_then_filter_suppresses_seen() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        mark(&db, &["https://a.com/x".into()], "2026-06-16T00:00:00Z").await.unwrap();
        let fresh = filter_unseen(&db, vec![art("https://a.com/x/"), art("https://b.com/y")]).await.unwrap();
        assert_eq!(fresh.len(), 1);
        assert_eq!(fresh[0].url, "https://b.com/y");
    }

    #[tokio::test]
    async fn prune_drops_old_rows() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        mark(&db, &["https://a.com/x".into()], "2026-06-01T00:00:00Z").await.unwrap();
        prune(&db, "2026-06-10T00:00:00Z").await.unwrap();
        let fresh = filter_unseen(&db, vec![art("https://a.com/x")]).await.unwrap();
        assert_eq!(fresh.len(), 1);
    }
}
