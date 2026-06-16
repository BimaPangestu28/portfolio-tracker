//! IT-news candidates: fetch (HN + RSS), keyword-score for the owner's stack,
//! merge/dedup, and shortlist. Selection of the final 3 happens in `digest`.

pub mod digest;
pub mod extract;
pub mod hackernews;
pub mod llm;
pub mod rss;
pub mod seen;

use crate::db::Db;

/// A news candidate before it becomes a persisted digest article.
#[derive(Debug, Clone, PartialEq)]
pub struct Article {
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub published_at: Option<String>,
    pub relevance: i32,
    pub hn_object_id: Option<String>,
}

/// Stack keywords (lowercase). Each distinct match in a title adds 1 to relevance.
const KEYWORDS: &[&str] = &[
    "rust",
    "blockchain", "web3", "solidity", "ethereum",
    "ai", "llm", "agent", "model",
    "cloud", "azure", "aws", "kubernetes", "databricks",
    "typescript", "react",
];

/// Count distinct stack keywords appearing in the title (case-insensitive,
/// word-ish boundaries so "ai" doesn't match "rain").
pub fn relevance_of(title: &str) -> i32 {
    let lower = format!(" {} ", title.to_lowercase());
    KEYWORDS
        .iter()
        .filter(|kw| {
            let needle = format!(" {} ", kw);
            lower.contains(&needle)
                || lower.contains(&format!(" {}.", kw))
                || lower.contains(&format!(" {},", kw))
                || lower.contains(&format!("{}:", kw))
        })
        .count() as i32
}

const MAX_CANDIDATES_DEFAULT: usize = 12;

fn max_candidates() -> usize {
    std::env::var("NEWS_MAX_CANDIDATES").ok().and_then(|v| v.parse().ok()).unwrap_or(MAX_CANDIDATES_DEFAULT)
}

/// Normalize a URL for dedup: strip a trailing slash.
pub fn norm_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// Merge candidates from all sources: drop relevance==0, dedup by normalized URL
/// (keep the higher score/relevance), sort by (relevance desc, score desc), truncate.
pub fn rank(mut all: Vec<Article>, limit: usize) -> Vec<Article> {
    all.retain(|a| a.relevance > 0);
    all.sort_by_key(|a| norm_url(&a.url));
    all.dedup_by(|a, b| {
        if norm_url(&a.url) == norm_url(&b.url) {
            b.score = b.score.max(a.score);
            b.relevance = b.relevance.max(a.relevance);
            true
        } else {
            false
        }
    });
    all.sort_by(|a, b| b.relevance.cmp(&a.relevance).then(b.score.cmp(&a.score)));
    all.truncate(limit);
    all
}

/// Build a reqwest client with a sane timeout for all news fetches. The builder
/// only fails on TLS-backend init (effectively never); a clear panic beats
/// silently dropping the timeout via `unwrap_or_default`.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("portfolio-tracker-news/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .expect("news: failed to build HTTP client")
}

/// Fetch all sources (degrading each independently), rank, and drop
/// recently-seen URLs. Returns up to `NEWS_MAX_CANDIDATES` candidates.
pub async fn shortlist(db: &Db) -> Vec<Article> {
    let client = http_client();
    let mut all = Vec::new();

    match hackernews::fetch(&client).await {
        Ok(mut v) => all.append(&mut v),
        Err(e) => tracing::warn!("news: HN fetch failed: {e:#}"),
    }
    for feed in rss::feeds_from_env() {
        match rss::fetch_one(&client, &feed).await {
            Ok(mut v) => all.append(&mut v),
            Err(e) => tracing::warn!("news: rss '{feed}' failed: {e:#}"),
        }
    }

    let ranked = rank(all, max_candidates());
    match seen::filter_unseen(db, ranked.clone()).await {
        Ok(fresh) => fresh,
        Err(e) => {
            tracing::warn!("news: seen filter failed, using unfiltered: {e:#}");
            ranked
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_counts_distinct_stack_keywords() {
        assert_eq!(relevance_of("New Rust release for AWS Lambda"), 2);
        assert_eq!(relevance_of("A cooking blog about rain"), 0);
        assert_eq!(relevance_of("LLM agent framework in TypeScript"), 3);
    }

    fn art(url: &str, rel: i32, score: i64) -> Article {
        Article { title: url.into(), url: url.into(), source: "t".into(), score, published_at: None, relevance: rel, hn_object_id: None }
    }

    #[test]
    fn rank_dedups_keeps_best_and_sorts() {
        let out = rank(
            vec![
                art("https://a.com/x", 1, 10),
                art("https://a.com/x/", 2, 5),
                art("https://b.com/y", 3, 1),
                art("https://c.com/z", 0, 99),
            ],
            10,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://b.com/y");
        assert_eq!(out[1].relevance, 2);
    }

    #[test]
    fn rank_truncates_to_limit() {
        let many = (0..20).map(|i| art(&format!("https://s/{i}"), 1, i)).collect();
        assert_eq!(rank(many, 5).len(), 5);
    }
}
