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
    #[serde(rename = "objectID")]
    object_id: Option<String>,
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
                hn_object_id: h.object_id,
            })
        })
        .collect())
}

/// Fetch + parse. Network errors propagate; the caller degrades.
pub async fn fetch(client: &reqwest::Client) -> anyhow::Result<Vec<Article>> {
    let body = client.get(ENDPOINT).send().await?.error_for_status()?.text().await?;
    parse(&body)
}

#[derive(Deserialize)]
struct HnItem {
    #[serde(default)]
    children: Vec<HnComment>,
}

#[derive(Deserialize)]
struct HnComment {
    text: Option<String>,
}

/// Strip HTML tags from a comment to plain, whitespace-collapsed text.
fn strip_html(html: &str) -> String {
    let frag = scraper::Html::parse_fragment(html);
    let text = frag.root_element().text().collect::<Vec<_>>().join(" ");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Top-level HN comments as plain text: skips deleted/empty and trivially short
/// ones, truncates each, and caps the count. Pure (testable without network).
pub fn parse_comments(body: &str, max: usize) -> Vec<String> {
    let Ok(item) = serde_json::from_str::<HnItem>(body) else { return vec![] };
    item.children
        .into_iter()
        .filter_map(|c| c.text)
        .map(|t| strip_html(&t))
        .filter(|t| t.len() >= 40)
        .map(|t| t.chars().take(800).collect::<String>())
        .take(max)
        .collect()
}

/// Fetch an HN item's top comments (Algolia items API). Empty vec on any failure.
pub async fn fetch_top_comments(client: &reqwest::Client, object_id: &str) -> Vec<String> {
    let url = format!("https://hn.algolia.com/api/v1/items/{object_id}");
    let Ok(resp) = client.get(&url).send().await.and_then(|r| r.error_for_status()) else {
        return vec![];
    };
    let Ok(body) = resp.text().await else { return vec![] };
    parse_comments(&body, 6)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
      "hits": [
        {"title":"Rust 2.0 announced","url":"https://example.com/rust","points":420,"created_at":"2026-06-16T01:00:00Z","objectID":"123"},
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
        assert_eq!(arts[0].hn_object_id.as_deref(), Some("123"));
        assert_eq!(arts[1].relevance, 0);
    }

    const ITEM: &str = r#"{"children":[
      {"text":"<p>This tool snapshots agent state to disk so you can resume workflows. Works with LangGraph and similar frameworks.</p>"},
      {"text":"<p>How does it compare to framework X checkpointing? Seems lighter weight.</p>"},
      {"text":null},
      {"text":"hi"}
    ]}"#;

    #[test]
    fn parse_comments_strips_html_and_filters_trivial() {
        let c = parse_comments(ITEM, 6);
        assert_eq!(c.len(), 2); // null and the too-short "hi" are dropped
        assert!(c[0].contains("snapshots agent state"));
        assert!(!c[0].contains("<p>"));
    }
}
