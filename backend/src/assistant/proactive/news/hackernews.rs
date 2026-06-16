//! Hacker News front page via the Algolia API (no key required).

use super::{relevance_of, Article};
use serde::Deserialize;

const ENDPOINT: &str = "https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=50";

#[derive(Deserialize)]
struct AlgoliaResponse {
    hits: Vec<Hit>,
}

#[derive(Deserialize)]
struct Hit {
    title: Option<String>,
    url: Option<String>,
    points: Option<i64>,
    created_at: Option<String>,
}

/// Parse an Algolia front-page payload into candidates. Hits without a `url`
/// (Ask HN / text posts) or without a title are dropped.
pub fn parse(body: &str) -> anyhow::Result<Vec<Article>> {
    let resp: AlgoliaResponse = serde_json::from_str(body)?;
    Ok(resp
        .hits
        .into_iter()
        .filter_map(|h| {
            let title = h.title?;
            let url = h.url?;
            Some(Article {
                relevance: relevance_of(&title),
                title,
                url,
                source: "HN".into(),
                score: h.points.unwrap_or(0),
                published_at: h.created_at,
            })
        })
        .collect())
}

/// Fetch + parse. Network errors propagate; the caller degrades.
pub async fn fetch(client: &reqwest::Client) -> anyhow::Result<Vec<Article>> {
    let body = client.get(ENDPOINT).send().await?.error_for_status()?.text().await?;
    parse(&body)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "hits": [
        {"title":"Rust 2.0 announced","url":"https://example.com/rust","points":420,"created_at":"2026-06-16T01:00:00Z"},
        {"title":"Ask HN: best editor?","url":null,"points":5,"created_at":"2026-06-16T02:00:00Z"},
        {"title":"A new database","url":"https://example.com/db","points":88,"created_at":"2026-06-16T03:00:00Z"}
      ]
    }"#;

    #[test]
    fn parse_drops_urlless_hits_and_scores_relevance() {
        let arts = parse(FIXTURE).unwrap();
        assert_eq!(arts.len(), 2);
        assert_eq!(arts[0].title, "Rust 2.0 announced");
        assert_eq!(arts[0].score, 420);
        assert_eq!(arts[0].relevance, 1);
        assert_eq!(arts[0].source, "HN");
        assert_eq!(arts[1].relevance, 0);
    }
}
