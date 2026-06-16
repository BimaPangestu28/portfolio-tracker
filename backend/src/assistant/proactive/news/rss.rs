//! RSS/Atom feeds via feed-rs. One feed = one source; failures are skipped by
//! the caller so a broken feed never sinks the digest.

use super::{relevance_of, Article};

/// Default feeds, tailored to the owner's stack. Overridable via NEWS_RSS_FEEDS.
const DEFAULT_FEEDS: &[&str] = &[
    "https://feed.infoq.com/",
    "https://thenewstack.io/feed/",
    "https://www.reddit.com/r/rust/.rss",
    "https://www.reddit.com/r/programming/.rss",
];

/// Configured feed URLs: NEWS_RSS_FEEDS (comma-separated) or the defaults.
pub fn feeds_from_env() -> Vec<String> {
    match std::env::var("NEWS_RSS_FEEDS") {
        Ok(v) if !v.trim().is_empty() => {
            v.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        }
        _ => DEFAULT_FEEDS.iter().map(|s| s.to_string()).collect(),
    }
}

/// Parse one feed body into candidates. `source` labels every entry; when empty,
/// the feed's own title (or "RSS") is used.
pub fn parse(body: &[u8], source: &str) -> anyhow::Result<Vec<Article>> {
    let feed = feed_rs::parser::parse(std::io::Cursor::new(body))?;
    let src = if source.is_empty() {
        feed.title.as_ref().map(|t| t.content.clone()).unwrap_or_else(|| "RSS".into())
    } else {
        source.to_string()
    };
    Ok(feed
        .entries
        .into_iter()
        .filter_map(|e| {
            let title = e.title.map(|t| t.content)?;
            let url = e.links.into_iter().map(|l| l.href).next()?;
            Some(Article {
                relevance: relevance_of(&title),
                title,
                url,
                source: src.clone(),
                score: 0,
                published_at: e.published.or(e.updated).map(|d| d.to_rfc3339()),
            })
        })
        .collect())
}

/// Fetch + parse one feed.
pub async fn fetch_one(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<Article>> {
    let bytes = client.get(url).send().await?.error_for_status()?.bytes().await?;
    parse(&bytes, "")
}

#[cfg(test)]
mod tests {
    use super::*;

    const RSS: &str = r#"<?xml version="1.0"?>
    <rss version="2.0"><channel><title>Dev News</title>
      <item><title>Kubernetes 2.0 ships</title><link>https://ex.com/k8s</link>
        <pubDate>Mon, 15 Jun 2026 10:00:00 GMT</pubDate></item>
      <item><title>A gardening story</title><link>https://ex.com/garden</link></item>
    </channel></rss>"#;

    #[test]
    fn parse_rss_maps_entries_and_scores() {
        let arts = parse(RSS.as_bytes(), "Dev News").unwrap();
        assert_eq!(arts.len(), 2);
        assert_eq!(arts[0].title, "Kubernetes 2.0 ships");
        assert_eq!(arts[0].url, "https://ex.com/k8s");
        assert_eq!(arts[0].source, "Dev News");
        assert_eq!(arts[0].relevance, 1);
        assert_eq!(arts[1].relevance, 0);
    }

    #[test]
    fn feeds_from_env_falls_back_to_defaults() {
        assert!(!feeds_from_env().is_empty());
    }
}
