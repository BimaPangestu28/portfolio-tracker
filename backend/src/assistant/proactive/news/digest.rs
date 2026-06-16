//! The daily digest: pick top 3 candidates, summarize each, build a quiz, and
//! persist. `ensure_today` is the single idempotent generation path.

use super::{llm, seen, shortlist};
use crate::db::Db;
use crate::repo::news as repo;

const TOP_N: usize = 3;
const SEEN_RETENTION_DAYS: i64 = 14;

/// A digest article shaped for the briefing/API (decoded key_points).
/// A faithful in-memory article from the persisted digest. The briefing only
/// reads `title`/`summary`/`url` today; the other fields are kept for
/// completeness (and a future briefing that shows source/points) — hence the
/// targeted `allow(dead_code)` rather than dropping data the store already holds.
#[derive(Debug, Clone)]
pub struct DigestArticle {
    #[allow(dead_code)]
    pub position: i64,
    pub title: String,
    pub url: String,
    #[allow(dead_code)]
    pub source: String,
    pub summary: String,
    #[allow(dead_code)]
    pub key_points: Vec<String>,
}

fn news_enabled() -> bool {
    !std::env::var("NEWS_ENABLED").map(|v| v.eq_ignore_ascii_case("off") || v == "false").unwrap_or(false)
}

/// Return today's digest articles, generating + persisting the digest if absent.
/// Idempotent per WIB date; safe to call from both the job and the briefing.
pub async fn ensure_today(db: &Db) -> anyhow::Result<Vec<DigestArticle>> {
    if !news_enabled() {
        return Ok(vec![]);
    }
    let now_wib = chrono::Utc::now().with_timezone(&crate::assistant::time::wib());
    let date = now_wib.format("%Y-%m-%d").to_string();

    // The claim is permanent (like the other proactive jobs): if `generate`
    // fails — e.g. the LLM is down at this hour — the day is forfeited and there
    // is no retry, only the warning below. A missed news digest is a low-stakes
    // outcome (no reading material that morning); it recovers the next day.
    if !repo::exists(db, &date).await?
        && crate::repo::proactive_log::try_claim(db, "news_digest", &format!("news_digest:{date}")).await?
    {
        if let Err(e) = generate(db, &date).await {
            tracing::warn!("news digest generation for {date} failed: {e:#}");
        }
    }
    load(db, &date).await
}

/// Load persisted articles for a date into DigestArticle (decoding key_points).
pub async fn load(db: &Db, date: &str) -> anyhow::Result<Vec<DigestArticle>> {
    Ok(repo::articles(db, date)
        .await?
        .into_iter()
        .map(|a| {
            let key_points = serde_json::from_str::<Vec<String>>(&a.key_points).unwrap_or_else(|e| {
                tracing::warn!("news: malformed key_points (article {}): {e}", a.position);
                Vec::new()
            });
            DigestArticle {
                position: a.position,
                title: a.title,
                url: a.url,
                source: a.source,
                summary: a.summary,
                key_points,
            }
        })
        .collect())
}

/// Fetch candidates, summarize the top 3, build the quiz, persist, mark seen.
async fn generate(db: &Db, date: &str) -> anyhow::Result<()> {
    let candidates = shortlist(db).await;
    if candidates.is_empty() {
        tracing::info!("news digest {date}: no candidates");
        return Ok(());
    }
    let client = super::http_client();

    let chosen: Vec<_> = candidates.iter().take(TOP_N).cloned().collect();
    let mut new_articles = Vec::new();
    let mut summaries_block = String::new();

    for (i, a) in chosen.iter().enumerate() {
        let text = super::extract::fetch_main_text(&client, &a.url).await.unwrap_or_default();
        let snippet = if text.is_empty() { a.title.clone() } else { String::new() };
        let s = llm::summarize(&a.title, &a.source, &text, &snippet).await;
        summaries_block.push_str(&format!(
            "Artikel {i} — {}\nRingkasan: {}\nPoin: {}\n\n",
            a.title, s.summary, s.key_points.join("; ")
        ));
        new_articles.push(repo::NewArticle {
            position: i as i64,
            title: a.title.clone(),
            url: a.url.clone(),
            source: a.source.clone(),
            score: a.score,
            summary: s.summary,
            key_points_json: serde_json::to_string(&s.key_points).unwrap_or_else(|_| "[]".into()),
        });
    }

    let quiz_items = llm::quiz(&summaries_block).await.unwrap_or_default();
    let new_quiz: Vec<_> = quiz_items
        .into_iter()
        .enumerate()
        .map(|(i, q)| repo::NewQuiz {
            position: i as i64,
            article_pos: Some(q.article_position),
            question: q.question,
            options_json: serde_json::to_string(&q.options).unwrap_or_else(|_| "[]".into()),
            answer_index: q.answer_index,
            explanation: if q.explanation.is_empty() { None } else { Some(q.explanation) },
        })
        .collect();

    let now_utc = chrono::Utc::now().to_rfc3339();
    repo::insert(db, date, &now_utc, &new_articles, &new_quiz).await?;

    let urls: Vec<String> = candidates.iter().map(|a| a.url.clone()).collect();
    if let Err(e) = seen::mark(db, &urls, &now_utc).await {
        tracing::warn!("news: mark seen failed: {e:#}");
    }
    let cutoff = (chrono::Utc::now() - chrono::Duration::days(SEEN_RETENTION_DAYS)).to_rfc3339();
    let _ = seen::prune(db, &cutoff).await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[serial_test::serial]
    async fn ensure_today_is_noop_when_disabled() {
        std::env::set_var("NEWS_ENABLED", "off");
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let arts = ensure_today(&db).await.unwrap();
        assert!(arts.is_empty());
        std::env::remove_var("NEWS_ENABLED");
    }

    #[tokio::test]
    async fn load_decodes_key_points() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        repo::insert(&db, "2026-06-16", "2026-06-16T00:00:00Z",
            &[repo::NewArticle {
                position: 0, title: "t".into(), url: "u".into(), source: "HN".into(),
                score: 1, summary: "s".into(), key_points_json: "[\"a\",\"b\"]".into(),
            }], &[]).await.unwrap();
        let arts = load(&db, "2026-06-16").await.unwrap();
        assert_eq!(arts[0].key_points, vec!["a", "b"]);
    }
}
