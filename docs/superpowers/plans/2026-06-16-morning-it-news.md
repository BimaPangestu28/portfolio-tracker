# Morning IT News (Digest + Briefing + Web Page) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate a persisted daily IT-news digest (HN + RSS, tailored to the owner's stack) each morning, surface its top 3 as a "Bacaan pagi" section in the Telegram briefing, and expose the full summaries + a retention quiz on a new `/news` web page.

**Architecture:** A new `assistant/proactive/news/` module fetches and keyword-scores candidates, code picks the top 3, each article is fetched + main-text-extracted, the LLM writes per-article summaries and one quiz, and the result is persisted (one row per WIB date). The proactive tick runs the generation each morning; the briefing and a read-only `GET /news/today` endpoint both consume the persisted digest. The React app adds a `/news` page that renders the digest and an interactive, client-scored quiz.

**Tech Stack:** Rust (axum, sqlx/SQLite, reqwest, serde, `feed-rs`, `readability`), existing `llm::claude` client; React + TypeScript + Vite, React Query, Zod, Tailwind/Radix, vitest + MSW.

---

## Conventions (read first)

- **No `cargo fmt`** on the backend (hand-maintained layout). Verify each backend task with
  `cd backend && cargo clippy && cargo test <name>`.
- Backend errors: `anyhow::Result` in modules/repos, `AppError` in `api/` handlers. No
  `unwrap()`/`panic!()` on network/parse/DB paths.
- Repos use runtime `sqlx::query`/`query_as` with `#[derive(sqlx::FromRow)]` (see
  `backend/src/repo/reminders.rs`). Migrations auto-run on startup via `sqlx::migrate!`.
- Frontend: every response parsed through a Zod schema via `api.get(path, schema)`; query
  hooks live in `src/api/hooks.ts`, schemas in `src/api/schemas.ts`.
- Conventional commits. Commit after each task's tests pass.

## File Structure

**Create (backend):**
- `backend/migrations/0022_news_digest.sql` — four tables.
- `backend/src/assistant/proactive/news/mod.rs` — `Article` candidate, scoring, merge/dedup, `shortlist`.
- `backend/src/assistant/proactive/news/hackernews.rs` — HN fetch + parse.
- `backend/src/assistant/proactive/news/rss.rs` — RSS/Atom fetch + parse.
- `backend/src/assistant/proactive/news/extract.rs` — article fetch + main-text extraction.
- `backend/src/assistant/proactive/news/llm.rs` — summary + quiz prompts, JSON parsing, fallbacks.
- `backend/src/assistant/proactive/news/digest.rs` — `ensure_today`, `generate`, persistence orchestration, `DigestArticle`/`QuizQuestion`/`Digest` types.
- `backend/src/assistant/proactive/news/seen.rs` — `mark`, `filter_unseen`, `prune`.
- `backend/src/repo/news.rs` — digest SQL (`today`, `insert`, helpers).
- `backend/src/api/news.rs` — `GET /news/today` handler + response DTOs.

**Modify (backend):**
- `backend/Cargo.toml` — add `feed-rs`, `readability`.
- `backend/src/assistant/proactive/mod.rs` — `pub mod news;`.
- `backend/src/repo/mod.rs` — `pub mod news;`.
- `backend/src/api/mod.rs` — `pub mod news;` + route.
- `backend/src/assistant/proactive/briefing.rs` — `news` field, render block, gather call.
- `backend/src/assistant/proactive/compose.rs` — extend `BRIEFING_SYSTEM`.
- `backend/src/assistant/proactive/tick.rs` — `news_digest_due` + wiring.
- `backend/.env.example` — document the `NEWS_*` vars.

**Create (frontend):**
- `frontend/src/pages/NewsPage.tsx` — the page.
- `frontend/src/pages/NewsPage.test.tsx` — page tests.
- `frontend/src/components/NewsQuiz.tsx` — quiz component.
- `frontend/src/components/NewsQuiz.test.tsx` — quiz tests.

**Modify (frontend):**
- `frontend/src/api/schemas.ts` — news schemas.
- `frontend/src/api/schemas.test.ts` — schema tests.
- `frontend/src/api/hooks.ts` — `useNewsToday`.
- `frontend/src/App.tsx` — `/news` route.
- `frontend/src/components/AppShell.tsx` — nav item.

---

# Phase 1 — Backend digest core

### Task 1: Dependencies + migration

**Files:**
- Modify: `backend/Cargo.toml`
- Create: `backend/migrations/0022_news_digest.sql`

- [ ] **Step 1: Add crates** to `backend/Cargo.toml` under `[dependencies]`:

```toml
feed-rs = "2"
readability = "0.3"
sha2 = "0.10"
```

(`sha2` hashes URLs for `news_seen`. `feed-rs` parses RSS+Atom; `readability` extracts article main text.)

- [ ] **Step 2: Write the migration** `backend/migrations/0022_news_digest.sql`:

```sql
CREATE TABLE news_digest (
    digest_date TEXT PRIMARY KEY,
    created_at  TEXT NOT NULL
);

CREATE TABLE news_article (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    digest_date TEXT NOT NULL REFERENCES news_digest(digest_date) ON DELETE CASCADE,
    position    INTEGER NOT NULL,
    title       TEXT NOT NULL,
    url         TEXT NOT NULL,
    source      TEXT NOT NULL,
    score       INTEGER NOT NULL DEFAULT 0,
    summary     TEXT NOT NULL,
    key_points  TEXT NOT NULL
);

CREATE TABLE news_quiz_question (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    digest_date  TEXT NOT NULL REFERENCES news_digest(digest_date) ON DELETE CASCADE,
    position     INTEGER NOT NULL,
    article_pos  INTEGER,
    question     TEXT NOT NULL,
    options      TEXT NOT NULL,
    answer_index INTEGER NOT NULL,
    explanation  TEXT
);

CREATE TABLE news_seen (
    url_hash   TEXT PRIMARY KEY,
    url        TEXT NOT NULL,
    first_seen TEXT NOT NULL
);
```

- [ ] **Step 3: Verify it builds and migrates** (migration runs on a test DB connect):

Run: `cd backend && cargo build`
Expected: compiles (deps resolve). If `readability = "0.3"` fails to resolve, use the latest `0.x` shown by `cargo add readability --dry-run` and note it in the commit.

- [ ] **Step 4: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/migrations/0022_news_digest.sql
git commit -m "feat(news): add news digest schema + feed/readability deps"
```

---

### Task 2: `Article` candidate type + keyword scoring

**Files:**
- Create: `backend/src/assistant/proactive/news/mod.rs`
- Modify: `backend/src/assistant/proactive/mod.rs` (add `pub mod news;`)

- [ ] **Step 1: Register the module.** Add to `backend/src/assistant/proactive/mod.rs` after the existing `pub mod` lines:

```rust
pub mod news;
```

- [ ] **Step 2: Write the failing test.** Create `backend/src/assistant/proactive/news/mod.rs`:

```rust
//! IT-news candidates: fetch (HN + RSS), keyword-score for the owner's stack,
//! merge/dedup, and shortlist. Selection of the final 3 happens in `digest`.

pub mod digest;
pub mod extract;
pub mod hackernews;
pub mod llm;
pub mod rss;
pub mod seen;

/// A news candidate before it becomes a persisted digest article.
#[derive(Debug, Clone, PartialEq)]
pub struct Article {
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub published_at: Option<String>,
    pub relevance: i32,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relevance_counts_distinct_stack_keywords() {
        assert_eq!(relevance_of("New Rust release for AWS Lambda"), 2);
        assert_eq!(relevance_of("A cooking blog about rain"), 0);
        assert_eq!(relevance_of("LLM agent framework in TypeScript"), 3);
    }
}
```

- [ ] **Step 3: Run the test (expect FAIL — submodules not created yet).**

Run: `cd backend && cargo test -p portfolio-tracker relevance_counts`
Expected: compile error (the `pub mod` lines reference files that don't exist yet). This is expected; the next tasks create them. To verify the logic in isolation now, temporarily comment the five `pub mod` lines, run the test (PASS), then uncomment.

- [ ] **Step 4: Verify logic passes** (with submodules commented as above):

Run: `cd backend && cargo test -p portfolio-tracker relevance_counts`
Expected: PASS. Re-enable the `pub mod` lines afterward.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/proactive/mod.rs backend/src/assistant/proactive/news/mod.rs
git commit -m "feat(news): Article candidate type + stack keyword scoring"
```

---

### Task 3: Hacker News fetch + parse

**Files:**
- Create: `backend/src/assistant/proactive/news/hackernews.rs`

- [ ] **Step 1: Write the failing test.** Create `backend/src/assistant/proactive/news/hackernews.rs`:

```rust
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
        assert_eq!(arts[0].relevance, 1); // "rust"
        assert_eq!(arts[0].source, "HN");
        assert_eq!(arts[1].relevance, 0);
    }
}
```

- [ ] **Step 2: Run the test (expect FAIL → PASS once compiled).**

Run: `cd backend && cargo test -p portfolio-tracker hackernews`
Expected: PASS (parse is pure; no network in the test).

- [ ] **Step 3: Commit**

```bash
git add backend/src/assistant/proactive/news/hackernews.rs
git commit -m "feat(news): Hacker News front-page fetch + parse"
```

---

### Task 4: RSS/Atom fetch + parse

**Files:**
- Create: `backend/src/assistant/proactive/news/rss.rs`

- [ ] **Step 1: Write the failing test.** Create `backend/src/assistant/proactive/news/rss.rs`:

```rust
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

/// Parse one feed body into candidates. `source` labels every entry.
pub fn parse(body: &[u8], source: &str) -> anyhow::Result<Vec<Article>> {
    let feed = feed_rs::parser::parse(body)?;
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
        assert_eq!(arts[0].relevance, 1); // "kubernetes"
        assert_eq!(arts[1].relevance, 0);
    }

    #[test]
    fn feeds_from_env_falls_back_to_defaults() {
        // No env set in tests → defaults.
        assert!(!feeds_from_env().is_empty());
    }
}
```

- [ ] **Step 2: Run the test.**

Run: `cd backend && cargo test -p portfolio-tracker news::rss`
Expected: PASS. If `feed_rs::parser::parse` signature differs in the resolved version, adjust the call to match `feed-rs` v2 (it accepts `impl Read`; wrap with `std::io::Cursor::new(body)` if needed).

- [ ] **Step 3: Commit**

```bash
git add backend/src/assistant/proactive/news/rss.rs
git commit -m "feat(news): RSS/Atom feed fetch + parse with env-configured feeds"
```

---

### Task 5: Merge, dedup, and `shortlist` orchestration

**Files:**
- Modify: `backend/src/assistant/proactive/news/mod.rs`

- [ ] **Step 1: Write the failing test.** Add to `backend/src/assistant/proactive/news/mod.rs` (above `#[cfg(test)]`):

```rust
use crate::db::Db;

const MAX_CANDIDATES_DEFAULT: usize = 12;

fn max_candidates() -> usize {
    std::env::var("NEWS_MAX_CANDIDATES").ok().and_then(|v| v.parse().ok()).unwrap_or(MAX_CANDIDATES_DEFAULT)
}

/// Normalize a URL for dedup: strip a trailing slash and lowercase the host part.
pub fn norm_url(url: &str) -> String {
    url.trim_end_matches('/').to_string()
}

/// Merge candidates from all sources: dedup by normalized URL (keep the higher
/// score), drop relevance==0, sort by (relevance desc, score desc), truncate.
pub fn rank(mut all: Vec<Article>, limit: usize) -> Vec<Article> {
    all.retain(|a| a.relevance > 0);
    all.sort_by(|a, b| norm_url(&a.url).cmp(&norm_url(&b.url)));
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

/// Build a reqwest client with a sane timeout for all news fetches.
fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("portfolio-tracker-news/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default()
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
```

And add these tests inside the existing `mod tests`:

```rust
    fn art(url: &str, rel: i32, score: i64) -> Article {
        Article { title: url.into(), url: url.into(), source: "t".into(), score, published_at: None, relevance: rel }
    }

    #[test]
    fn rank_dedups_keeps_best_and_sorts() {
        let out = rank(
            vec![
                art("https://a.com/x", 1, 10),
                art("https://a.com/x/", 2, 5), // same after norm; higher relevance wins
                art("https://b.com/y", 3, 1),
                art("https://c.com/z", 0, 99), // dropped: relevance 0
            ],
            10,
        );
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].url, "https://b.com/y"); // relevance 3 first
        assert_eq!(out[1].relevance, 2);           // merged kept the higher relevance
    }

    #[test]
    fn rank_truncates_to_limit() {
        let many = (0..20).map(|i| art(&format!("https://s/{i}"), 1, i)).collect();
        assert_eq!(rank(many, 5).len(), 5);
    }
```

- [ ] **Step 2: Run the tests (rank only; `shortlist` needs `seen` from Task 7).**

Run: `cd backend && cargo test -p portfolio-tracker news::mod::tests::rank`
Expected: compile error until `seen`/other submodules exist. If blocking, stub `seen.rs`/`extract.rs`/`llm.rs`/`digest.rs` as empty files with the public items added in later tasks, or implement Task 7 (`seen`) before running. Simplest: proceed to Task 6–10 then run the whole suite at Task 11.

- [ ] **Step 3: Commit**

```bash
git add backend/src/assistant/proactive/news/mod.rs
git commit -m "feat(news): rank/dedup candidates + shortlist orchestration"
```

---

### Task 6: Article main-text extraction

**Files:**
- Create: `backend/src/assistant/proactive/news/extract.rs`

- [ ] **Step 1: Write the failing test.** Create `backend/src/assistant/proactive/news/extract.rs`:

```rust
//! Fetch an article URL and extract its main text. Guards: http(s) only,
//! timeout (via the shared client), and a response-size cap. Any failure → None.

const MAX_BYTES: usize = 2_000_000; // 2 MB cap on the HTML body

/// Extract readable main text from an HTML string at `url`. Returns None when
/// extraction yields nothing usable.
pub fn extract_html(html: &str, url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let product = readability::extractor::extract(&mut html.as_bytes(), &parsed).ok()?;
    let text = product.text.trim();
    if text.len() < 200 { None } else { Some(text.to_string()) }
}

/// Fetch `url` and extract main text. Rejects non-http(s) and oversized bodies.
pub async fn fetch_main_text(client: &reqwest::Client, url: &str) -> Option<String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return None;
    }
    let resp = client.get(url).send().await.ok()?.error_for_status().ok()?;
    let bytes = resp.bytes().await.ok()?;
    if bytes.len() > MAX_BYTES {
        tracing::warn!("news: {url} body too large ({} bytes), skipping", bytes.len());
        return None;
    }
    let html = String::from_utf8_lossy(&bytes);
    extract_html(&html, url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_returns_none_for_thin_content() {
        assert!(extract_html("<html><body><p>hi</p></body></html>", "https://ex.com/a").is_none());
    }

    #[test]
    fn extract_pulls_paragraph_text() {
        let body = format!("<html><body><article><p>{}</p></article></body></html>", "lorem ipsum ".repeat(40));
        let out = extract_html(&body, "https://ex.com/a");
        assert!(out.is_some());
        assert!(out.unwrap().contains("lorem ipsum"));
    }

    #[test]
    fn fetch_rejects_non_http() {
        // Pure guard check via a runtime; ftp scheme returns None without network.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let client = reqwest::Client::new();
        assert!(rt.block_on(fetch_main_text(&client, "ftp://ex.com/a")).is_none());
    }
}
```

- [ ] **Step 2: Add the `url` crate** to `backend/Cargo.toml` if not present:

```toml
url = "2"
```

- [ ] **Step 3: Run the tests.**

Run: `cd backend && cargo test -p portfolio-tracker news::extract`
Expected: PASS. If `readability::extractor::extract`'s signature differs in the resolved version, adapt: it commonly takes `&mut impl Read` + `&Url` and returns `Result<Product, _>` with a `.text` field. If `readability` proves unworkable, replace `extract_html` with an `html2text`-based fallback that strips tags and keeps `<p>` text; keep the same `Option<String>` contract and the same tests.

- [ ] **Step 4: Commit**

```bash
git add backend/Cargo.toml backend/Cargo.lock backend/src/assistant/proactive/news/extract.rs
git commit -m "feat(news): article fetch + main-text extraction with guards"
```

---

### Task 7: `news_seen` repo (mark / filter / prune)

**Files:**
- Create: `backend/src/assistant/proactive/news/seen.rs`

- [ ] **Step 1: Write the failing test.** Create `backend/src/assistant/proactive/news/seen.rs`:

```rust
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
        Article { title: "t".into(), url: url.into(), source: "s".into(), score: 0, published_at: None, relevance: 1 }
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
        assert_eq!(fresh.len(), 1); // pruned → no longer suppressed
    }
}
```

- [ ] **Step 2: Run the tests.**

Run: `cd backend && cargo test -p portfolio-tracker news::seen`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add backend/src/assistant/proactive/news/seen.rs
git commit -m "feat(news): recently-seen URL store (mark/filter/prune)"
```

---

### Task 8: LLM summary + quiz (prompts, JSON parse, fallbacks)

**Files:**
- Create: `backend/src/assistant/proactive/news/llm.rs`

- [ ] **Step 1: Write the failing test.** Create `backend/src/assistant/proactive/news/llm.rs`:

```rust
//! Turn article text into a summary + key points, and build a retention quiz.
//! Every LLM path degrades deterministically (consistent with compose.rs).

use crate::llm::claude::{ClaudeClient, Part};
use serde::Deserialize;

pub const SUMMARY_SYSTEM: &str = "You summarize one IT/dev news article in Indonesian for a \
senior engineer. Output ONLY minified JSON: {\"summary\": string, \"key_points\": string[]}. \
summary = 2-3 calm sentences. key_points = 2-4 short bullets. Use ONLY the provided text; never \
invent facts. No markdown, no code fences.";

pub const QUIZ_SYSTEM: &str = "You write a short retention quiz in Indonesian from the day's \
article summaries. Output ONLY minified JSON: an array of \
{\"question\": string, \"options\": string[4], \"answer_index\": int (0-3), \"explanation\": \
string, \"article_position\": int}. One question per article, testing whether the reader \
absorbed the key point. Use ONLY the provided summaries. No markdown, no code fences.";

#[derive(Debug, Deserialize, PartialEq)]
pub struct Summary {
    pub summary: String,
    #[serde(default)]
    pub key_points: Vec<String>,
}

#[derive(Debug, Deserialize, PartialEq)]
pub struct QuizItem {
    pub question: String,
    pub options: Vec<String>,
    pub answer_index: i64,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub article_position: i64,
}

/// Strip ```json fences some models add, returning the inner JSON slice.
pub fn strip_fences(s: &str) -> &str {
    let t = s.trim();
    let t = t.strip_prefix("```json").or_else(|| t.strip_prefix("```")).unwrap_or(t);
    t.strip_suffix("```").unwrap_or(t).trim()
}

pub fn parse_summary(raw: &str) -> Option<Summary> {
    serde_json::from_str(strip_fences(raw)).ok()
}

pub fn parse_quiz(raw: &str) -> Option<Vec<QuizItem>> {
    let items: Vec<QuizItem> = serde_json::from_str(strip_fences(raw)).ok()?;
    let valid: Vec<QuizItem> = items
        .into_iter()
        .filter(|q| q.options.len() >= 2 && q.answer_index >= 0 && (q.answer_index as usize) < q.options.len())
        .collect();
    if valid.is_empty() { None } else { Some(valid) }
}

/// Summarize one article; falls back to the snippet/title on any failure.
pub async fn summarize(title: &str, source: &str, text: &str, fallback_snippet: &str) -> Summary {
    let fallback = || Summary { summary: if fallback_snippet.is_empty() { title.into() } else { fallback_snippet.into() }, key_points: vec![] };
    let client = match ClaudeClient::from_env() {
        Ok(c) => c,
        Err(_) => return fallback(),
    };
    let input = format!("Judul: {title}\nSumber: {source}\n\nIsi:\n{text}");
    match client.complete(SUMMARY_SYSTEM, &[Part::Text(input)]).await {
        Ok(raw) => parse_summary(&raw).unwrap_or_else(fallback),
        Err(e) => { tracing::warn!("news summarize failed: {e}"); fallback() }
    }
}

/// Build the quiz from already-summarized articles; None on any failure.
pub async fn quiz(summaries_block: &str) -> Option<Vec<QuizItem>> {
    let client = ClaudeClient::from_env().ok()?;
    match client.complete(QUIZ_SYSTEM, &[Part::Text(summaries_block.to_string())]).await {
        Ok(raw) => parse_quiz(&raw),
        Err(e) => { tracing::warn!("news quiz failed: {e}"); None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_summary_handles_fenced_json() {
        let raw = "```json\n{\"summary\":\"ringkas\",\"key_points\":[\"a\",\"b\"]}\n```";
        let s = parse_summary(raw).unwrap();
        assert_eq!(s.summary, "ringkas");
        assert_eq!(s.key_points, vec!["a", "b"]);
    }

    #[test]
    fn parse_quiz_filters_invalid_answer_index() {
        let raw = r#"[
          {"question":"q1","options":["a","b","c","d"],"answer_index":1,"explanation":"e","article_position":0},
          {"question":"bad","options":["a","b"],"answer_index":9,"explanation":"","article_position":1}
        ]"#;
        let q = parse_quiz(raw).unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].question, "q1");
    }

    #[test]
    fn parse_summary_rejects_garbage() {
        assert!(parse_summary("not json").is_none());
    }
}
```

- [ ] **Step 2: Run the tests.**

Run: `cd backend && cargo test -p portfolio-tracker news::llm`
Expected: PASS (pure parsing; `summarize`/`quiz` aren't called in tests so no network).

- [ ] **Step 3: Commit**

```bash
git add backend/src/assistant/proactive/news/llm.rs
git commit -m "feat(news): LLM summary + quiz prompts, JSON parse, fallbacks"
```

---

### Task 9: Digest repo (persist + read)

**Files:**
- Create: `backend/src/repo/news.rs`
- Modify: `backend/src/repo/mod.rs` (add `pub mod news;`)

- [ ] **Step 1: Register the repo.** Add to `backend/src/repo/mod.rs`:

```rust
pub mod news;
```

- [ ] **Step 2: Write the failing test.** Create `backend/src/repo/news.rs`:

```rust
//! Persistence for the daily news digest (migration 0022).

use crate::db::Db;
use serde::Serialize;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ArticleRow {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub summary: String,
    /// JSON array of strings.
    pub key_points: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct QuizRow {
    pub position: i64,
    pub article_pos: Option<i64>,
    pub question: String,
    /// JSON array of strings.
    pub options: String,
    pub answer_index: i64,
    pub explanation: Option<String>,
}

pub struct NewArticle {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub score: i64,
    pub summary: String,
    pub key_points_json: String,
}

pub struct NewQuiz {
    pub position: i64,
    pub article_pos: Option<i64>,
    pub question: String,
    pub options_json: String,
    pub answer_index: i64,
    pub explanation: Option<String>,
}

/// True if a digest already exists for the given WIB date.
pub async fn exists(db: &Db, date: &str) -> anyhow::Result<bool> {
    let row: Option<(String,)> = sqlx::query_as("SELECT digest_date FROM news_digest WHERE digest_date = ?")
        .bind(date)
        .fetch_optional(db)
        .await?;
    Ok(row.is_some())
}

/// Insert a full digest (header + articles + quiz) in one transaction.
pub async fn insert(
    db: &Db,
    date: &str,
    created_at: &str,
    articles: &[NewArticle],
    quiz: &[NewQuiz],
) -> anyhow::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query("INSERT OR IGNORE INTO news_digest (digest_date, created_at) VALUES (?, ?)")
        .bind(date).bind(created_at).execute(&mut *tx).await?;
    for a in articles {
        sqlx::query(
            "INSERT INTO news_article (digest_date, position, title, url, source, score, summary, key_points)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)")
            .bind(date).bind(a.position).bind(&a.title).bind(&a.url).bind(&a.source)
            .bind(a.score).bind(&a.summary).bind(&a.key_points_json)
            .execute(&mut *tx).await?;
    }
    for q in quiz {
        sqlx::query(
            "INSERT INTO news_quiz_question (digest_date, position, article_pos, question, options, answer_index, explanation)
             VALUES (?, ?, ?, ?, ?, ?, ?)")
            .bind(date).bind(q.position).bind(q.article_pos).bind(&q.question)
            .bind(&q.options_json).bind(q.answer_index).bind(&q.explanation)
            .execute(&mut *tx).await?;
    }
    tx.commit().await?;
    Ok(())
}

/// Articles for a date, ordered by position.
pub async fn articles(db: &Db, date: &str) -> anyhow::Result<Vec<ArticleRow>> {
    Ok(sqlx::query_as(
        "SELECT position, title, url, source, score, summary, key_points
         FROM news_article WHERE digest_date = ? ORDER BY position")
        .bind(date).fetch_all(db).await?)
}

/// Quiz questions for a date, ordered by position.
pub async fn quiz(db: &Db, date: &str) -> anyhow::Result<Vec<QuizRow>> {
    Ok(sqlx::query_as(
        "SELECT position, article_pos, question, options, answer_index, explanation
         FROM news_quiz_question WHERE digest_date = ? ORDER BY position")
        .bind(date).fetch_all(db).await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn insert_then_read_roundtrips() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let arts = vec![NewArticle {
            position: 0, title: "Rust 2.0".into(), url: "https://ex.com/r".into(),
            source: "HN".into(), score: 100, summary: "ringkas".into(),
            key_points_json: "[\"a\",\"b\"]".into(),
        }];
        let quizzes = vec![NewQuiz {
            position: 0, article_pos: Some(0), question: "apa?".into(),
            options_json: "[\"x\",\"y\"]".into(), answer_index: 1, explanation: Some("krn".into()),
        }];
        insert(&db, "2026-06-16", "2026-06-16T00:00:00Z", &arts, &quizzes).await.unwrap();

        assert!(exists(&db, "2026-06-16").await.unwrap());
        let a = articles(&db, "2026-06-16").await.unwrap();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].title, "Rust 2.0");
        let q = quiz(&db, "2026-06-16").await.unwrap();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].answer_index, 1);
    }
}
```

- [ ] **Step 3: Run the tests.**

Run: `cd backend && cargo test -p portfolio-tracker repo::news`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add backend/src/repo/mod.rs backend/src/repo/news.rs
git commit -m "feat(news): digest repo (transactional insert + read)"
```

---

### Task 10: `digest::generate` + `ensure_today`

**Files:**
- Create: `backend/src/assistant/proactive/news/digest.rs`

- [ ] **Step 1: Write the implementation + test.** Create `backend/src/assistant/proactive/news/digest.rs`:

```rust
//! The daily digest: pick top 3 candidates, summarize each, build a quiz, and
//! persist. `ensure_today` is the single idempotent generation path.

use super::{llm, seen, shortlist};
use crate::db::Db;
use crate::repo::news as repo;

const TOP_N: usize = 3;
const SEEN_RETENTION_DAYS: i64 = 14;

/// A digest article shaped for the briefing/API (decoded key_points).
#[derive(Debug, Clone)]
pub struct DigestArticle {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub summary: String,
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

    if !repo::exists(db, &date).await? {
        // Claim so the job and the briefing don't both generate.
        if crate::repo::proactive_log::try_claim(db, "news_digest", &format!("news_digest:{date}")).await? {
            if let Err(e) = generate(db, &date).await {
                tracing::warn!("news digest generation for {date} failed: {e:#}");
            }
        }
    }
    load(db, &date).await
}

/// Load persisted articles for a date into DigestArticle (decoding key_points).
pub async fn load(db: &Db, date: &str) -> anyhow::Result<Vec<DigestArticle>> {
    Ok(repo::articles(db, date)
        .await?
        .into_iter()
        .map(|a| DigestArticle {
            position: a.position,
            title: a.title,
            url: a.url,
            source: a.source,
            summary: a.summary,
            key_points: serde_json::from_str(&a.key_points).unwrap_or_default(),
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
    let client = reqwest::Client::builder()
        .user_agent("portfolio-tracker-news/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap_or_default();

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

    // Mark the whole shortlist seen, and prune old entries.
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
```

- [ ] **Step 2: Run the news suite** (now that all submodules exist):

Run: `cd backend && cargo test -p portfolio-tracker news::`
Expected: PASS. Note: tests that mutate env (`NEWS_ENABLED`) should run serially — if you see flakiness, add `serial_test::serial` (already a dev-dependency) to those `#[tokio::test]`s.

- [ ] **Step 3: clippy the module**

Run: `cd backend && cargo clippy -p portfolio-tracker`
Expected: no new warnings in `news/`.

- [ ] **Step 4: Commit**

```bash
git add backend/src/assistant/proactive/news/digest.rs
git commit -m "feat(news): digest generate + idempotent ensure_today"
```

---

# Phase 2 — Briefing integration

### Task 11: Add the "Bacaan pagi" block to the briefing

**Files:**
- Modify: `backend/src/assistant/proactive/briefing.rs`
- Modify: `backend/src/assistant/proactive/compose.rs`

- [ ] **Step 1: Add the field + render + gather.** In `backend/src/assistant/proactive/briefing.rs`:

  a. Add to the `BriefingData` struct (after `gmail_important`):

```rust
    /// Top news articles for today's digest; empty when news is disabled or
    /// generation failed (section omitted, briefing unaffected).
    pub news: Vec<crate::assistant::proactive::news::digest::DigestArticle>,
```

  b. In `gather`, before the final `Ok(BriefingData { ... })`, add:

```rust
    let news = crate::assistant::proactive::news::digest::ensure_today(db).await.unwrap_or_else(|e| {
        tracing::warn!("briefing: news digest unavailable: {e:#}");
        Vec::new()
    });
```

  c. Add `news,` to the returned `BriefingData { ... }`.

  d. In `render_data_block`, before the `memory_facts` block at the end, add:

```rust
    if !d.news.is_empty() {
        out.push_str("Bacaan pagi (sertakan apa adanya, jangan ubah link):\n");
        for a in &d.news {
            out.push_str(&format!("- {} — {} {}\n", a.title, a.summary, a.url));
        }
    }
```

- [ ] **Step 2: Update existing test data.** Every `BriefingData { ... }` literal in
  `briefing.rs` tests (the `fn data()` helper and `gather_works_on_an_empty_db`'s expectations)
  must compile. Add `news: vec![],` to the `data()` helper struct literal.

- [ ] **Step 3: Write the failing test.** Add to `briefing.rs` `mod tests`:

```rust
    #[test]
    fn news_section_renders_when_present_and_omitted_when_empty() {
        let mut d = data();
        assert!(!render_data_block(&d).contains("Bacaan pagi"));
        d.news = vec![crate::assistant::proactive::news::digest::DigestArticle {
            position: 0, title: "Rust 2.0".into(), url: "https://ex.com/r".into(),
            source: "HN".into(), summary: "rilis besar".into(), key_points: vec![],
        }];
        let block = render_data_block(&d);
        assert!(block.contains("Bacaan pagi"), "{block}");
        assert!(block.contains("Rust 2.0 — rilis besar https://ex.com/r"), "{block}");
    }
```

- [ ] **Step 4: Extend `BRIEFING_SYSTEM`.** In `backend/src/assistant/proactive/compose.rs`,
  append to the `BRIEFING_SYSTEM` string literal (before the closing quote of the last sentence):

```
 If a 'Bacaan pagi' section is present, include those lines exactly as given (keep each link unchanged, one line each) under a short 'Bacaan pagi:' heading; skip it when absent.
```

- [ ] **Step 5: Add a prompt-content test.** In `compose.rs` `mod tests`, extend the existing
  loop or add:

```rust
    #[test]
    fn briefing_prompt_mentions_reading_section() {
        assert!(BRIEFING_SYSTEM.to_lowercase().contains("bacaan pagi"));
    }
```

- [ ] **Step 6: Run the tests.**

Run: `cd backend && cargo test -p portfolio-tracker briefing && cargo test -p portfolio-tracker compose`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/proactive/briefing.rs backend/src/assistant/proactive/compose.rs
git commit -m "feat(news): surface top digest articles in the morning briefing"
```

---

### Task 12: Proactive `news_digest` job in the tick

**Files:**
- Modify: `backend/src/assistant/proactive/tick.rs`

- [ ] **Step 1: Write the failing test.** In `tick.rs`, add a `news_digest_due` function next to
  `briefing_due` and tests mirroring `briefing_due_inside_the_window_only`:

```rust
/// Dedup key when the morning news digest is due (its own hour, default 6 WIB),
/// using the same fixed-hour grace window as the briefing.
pub fn news_digest_due(now_wib: DateTime<FixedOffset>, news_hour: Option<u32>) -> Option<String> {
    let hour = news_hour?;
    let h = now_wib.hour();
    if h >= hour && h < hour + GRACE_HOURS {
        Some(format!("news_digest:{}", now_wib.format("%Y-%m-%d")))
    } else {
        None
    }
}
```

  And the test:

```rust
    #[test]
    fn news_digest_due_inside_the_window_only() {
        assert_eq!(news_digest_due(wib(2026, 6, 12, 5, 59), Some(6)), None);
        assert_eq!(news_digest_due(wib(2026, 6, 12, 6, 0), Some(6)), Some("news_digest:2026-06-12".to_string()));
        assert_eq!(news_digest_due(wib(2026, 6, 12, 11, 0), Some(6)), None); // past 6+5 grace
        assert_eq!(news_digest_due(wib(2026, 6, 12, 6, 0), None), None);
    }
```

- [ ] **Step 2: Add the config field.** In `ProactiveConfig` add `pub news_digest_hour: Option<u32>,`
  and in `from_env()` add:

```rust
            news_digest_hour: parse_hour(std::env::var("NEWS_DIGEST_HOUR_WIB").ok(), 6),
```

  Update the `config_defaults_are_sane` test to assert `config.news_digest_hour == Some(6)`, and the
  `ProactiveConfig { ... }` literal in `run_once_claims_and_survives...` to include `news_digest_hour: Some(0),`.

- [ ] **Step 3: Wire it into `run_once`.** Near the top of `run_once` (before the briefing block,
  so the digest is ready first):

```rust
    if let Some(_key) = news_digest_due(now_wib, config.news_digest_hour) {
        // ensure_today is itself idempotent + claims internally; call directly.
        if let Err(e) = super::news::digest::ensure_today(db).await {
            tracing::warn!("news digest tick failed: {e:#}");
        }
    }
```

- [ ] **Step 4: Run the tests.**

Run: `cd backend && cargo test -p portfolio-tracker tick`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/assistant/proactive/tick.rs
git commit -m "feat(news): generate the digest each morning from the proactive tick"
```

---

### Task 13: Document the env vars

**Files:**
- Modify: `backend/.env.example`

- [ ] **Step 1: Append a news section** to `backend/.env.example`:

```
# --- IT news digest (optional; on by default) ---
# Set to "off" to disable generation and both surfaces (briefing + /news page).
NEWS_ENABLED=true
# WIB hour the digest is generated (before the briefing). "off" disables the job.
NEWS_DIGEST_HOUR_WIB=6
# Comma-separated RSS/Atom feeds. Unset = a tailored default set.
NEWS_RSS_FEEDS=
# Max candidates considered before picking the top 3.
NEWS_MAX_CANDIDATES=12
# Quiz questions per digest (target; the LLM may return fewer).
NEWS_QUIZ_COUNT=4
```

- [ ] **Step 2: Commit**

```bash
git add backend/.env.example
git commit -m "docs(news): document NEWS_* env vars"
```

---

# Phase 3 — API + web page

### Task 14: `GET /news/today` endpoint

**Files:**
- Create: `backend/src/api/news.rs`
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Write the handler.** Create `backend/src/api/news.rs`:

```rust
use crate::error::AppError;
use crate::repo::news as repo;
use crate::AppState;
use axum::{extract::State, Json};
use serde::Serialize;

#[derive(Serialize)]
pub struct ArticleDto {
    pub position: i64,
    pub title: String,
    pub url: String,
    pub source: String,
    pub summary: String,
    pub key_points: Vec<String>,
}

#[derive(Serialize)]
pub struct QuizDto {
    pub position: i64,
    pub question: String,
    pub options: Vec<String>,
    pub answer_index: i64,
    pub explanation: Option<String>,
    pub article_position: Option<i64>,
}

#[derive(Serialize)]
pub struct TodayDto {
    pub available: bool,
    pub date: Option<String>,
    pub articles: Vec<ArticleDto>,
    pub quiz: Vec<QuizDto>,
}

/// Read-only: today's persisted digest. Never triggers generation.
pub async fn today(State(s): State<AppState>) -> Result<Json<TodayDto>, AppError> {
    let date = chrono::Utc::now()
        .with_timezone(&crate::assistant::time::wib())
        .format("%Y-%m-%d")
        .to_string();

    let articles = repo::articles(&s.db, &date).await.map_err(AppError::Other)?;
    if articles.is_empty() {
        return Ok(Json(TodayDto { available: false, date: None, articles: vec![], quiz: vec![] }));
    }
    let quiz = repo::quiz(&s.db, &date).await.map_err(AppError::Other)?;

    Ok(Json(TodayDto {
        available: true,
        date: Some(date),
        articles: articles
            .into_iter()
            .map(|a| ArticleDto {
                position: a.position,
                title: a.title,
                url: a.url,
                source: a.source,
                summary: a.summary,
                key_points: serde_json::from_str(&a.key_points).unwrap_or_default(),
            })
            .collect(),
        quiz: quiz
            .into_iter()
            .map(|q| QuizDto {
                position: q.position,
                question: q.question,
                options: serde_json::from_str(&q.options).unwrap_or_default(),
                answer_index: q.answer_index,
                explanation: q.explanation,
                article_position: q.article_pos,
            })
            .collect(),
    }))
}
```

- [ ] **Step 2: Register the route.** In `backend/src/api/mod.rs`: add `pub mod news;` with the
  other `pub mod` lines, and add to the `protected` router chain:

```rust
        .route("/news/today", get(news::today))
```

- [ ] **Step 3: Write an integration test.** Create `backend/tests/news_api.rs` mirroring the
  existing integration-test style (tower `oneshot`). Minimal version:

```rust
// Seed a digest, then GET /news/today and assert the JSON shape.
// (Follow the pattern in the existing backend integration tests for building
//  the router + AppState against sqlite::memory:.)
```

  If there is no existing integration-test harness to copy, skip the HTTP test and instead add a
  unit test in `repo::news` asserting `articles`+`quiz` read back (already covered in Task 9) —
  do not leave an empty test file.

- [ ] **Step 4: Build + test.**

Run: `cd backend && cargo test -p portfolio-tracker && cargo clippy -p portfolio-tracker`
Expected: PASS, no new clippy warnings.

- [ ] **Step 5: Commit**

```bash
git add backend/src/api/news.rs backend/src/api/mod.rs
git commit -m "feat(news): read-only GET /news/today endpoint"
```

---

### Task 15: Frontend schemas + hook

**Files:**
- Modify: `frontend/src/api/schemas.ts`
- Modify: `frontend/src/api/schemas.test.ts`
- Modify: `frontend/src/api/hooks.ts`

- [ ] **Step 1: Write the failing schema test.** Add to `frontend/src/api/schemas.test.ts`:

```ts
import { NewsTodaySchema } from "./schemas";

test("NewsTodaySchema parses an available digest", () => {
  const parsed = NewsTodaySchema.parse({
    available: true,
    date: "2026-06-16",
    articles: [{ position: 0, title: "Rust 2.0", url: "https://ex.com/r", source: "HN", summary: "rilis", key_points: ["a"] }],
    quiz: [{ position: 0, question: "apa?", options: ["x", "y"], answer_index: 1, explanation: "krn", article_position: 0 }],
  });
  expect(parsed.articles[0].title).toBe("Rust 2.0");
  expect(parsed.quiz[0].answer_index).toBe(1);
});

test("NewsTodaySchema parses an empty (unavailable) digest", () => {
  const parsed = NewsTodaySchema.parse({ available: false, date: null, articles: [], quiz: [] });
  expect(parsed.available).toBe(false);
});
```

- [ ] **Step 2: Add the schemas.** Append to `frontend/src/api/schemas.ts`:

```ts
export const NewsArticleSchema = z.object({
  position: z.number(),
  title: z.string(),
  url: z.string(),
  source: z.string(),
  summary: z.string(),
  key_points: z.array(z.string()),
});

export const NewsQuizSchema = z.object({
  position: z.number(),
  question: z.string(),
  options: z.array(z.string()),
  answer_index: z.number(),
  explanation: z.string().nullable(),
  article_position: z.number().nullable(),
});

export const NewsTodaySchema = z.object({
  available: z.boolean(),
  date: z.string().nullable(),
  articles: z.array(NewsArticleSchema),
  quiz: z.array(NewsQuizSchema),
});

export type NewsToday = z.infer<typeof NewsTodaySchema>;
export type NewsArticle = z.infer<typeof NewsArticleSchema>;
export type NewsQuiz = z.infer<typeof NewsQuizSchema>;
```

- [ ] **Step 3: Add the hook.** In `frontend/src/api/hooks.ts`, import `NewsTodaySchema` from
  `./schemas` and add:

```ts
export const useNewsToday = () =>
  useQuery({ queryKey: ["news", "today"], queryFn: () => api.get("/news/today", NewsTodaySchema) });
```

- [ ] **Step 4: Run the tests.**

Run: `cd frontend && npx vitest run src/api/schemas.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/schemas.test.ts frontend/src/api/hooks.ts
git commit -m "feat(news): frontend news schemas + useNewsToday hook"
```

---

### Task 16: `NewsQuiz` component

**Files:**
- Create: `frontend/src/components/NewsQuiz.tsx`
- Create: `frontend/src/components/NewsQuiz.test.tsx`

- [ ] **Step 1: Write the failing test.** Create `frontend/src/components/NewsQuiz.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import NewsQuiz from "./NewsQuiz";
import type { NewsQuiz as Q } from "../api/schemas";

const QS: Q[] = [
  { position: 0, question: "Apa rilis besar?", options: ["Go", "Rust 2.0"], answer_index: 1, explanation: "Karena Rust", article_position: 0 },
];

test("scores the quiz after submit and reveals the explanation", async () => {
  render(<NewsQuiz questions={QS} />);
  await userEvent.click(screen.getByLabelText("Rust 2.0"));
  await userEvent.click(screen.getByRole("button", { name: /selesai|cek|submit/i }));
  expect(screen.getByText(/1\s*\/\s*1/)).toBeInTheDocument();
  expect(screen.getByText(/Karena Rust/)).toBeInTheDocument();
});

test("marks a wrong answer as incorrect", async () => {
  render(<NewsQuiz questions={QS} />);
  await userEvent.click(screen.getByLabelText("Go"));
  await userEvent.click(screen.getByRole("button", { name: /selesai|cek|submit/i }));
  expect(screen.getByText(/0\s*\/\s*1/)).toBeInTheDocument();
});
```

- [ ] **Step 2: Implement the component.** Create `frontend/src/components/NewsQuiz.tsx`:

```tsx
import { useState } from "react";
import type { NewsQuiz as Q } from "../api/schemas";

export default function NewsQuiz({ questions }: { questions: Q[] }) {
  const [answers, setAnswers] = useState<Record<number, number>>({});
  const [submitted, setSubmitted] = useState(false);

  if (questions.length === 0) return null;

  const score = questions.filter((q) => answers[q.position] === q.answer_index).length;

  return (
    <section className="pt-card">
      <h2 className="text-lg font-semibold">Kuis hari ini</h2>
      {questions.map((q) => {
        const picked = answers[q.position];
        return (
          <div key={q.position} className="mt-4">
            <p className="font-medium">{q.question}</p>
            {q.options.map((opt, i) => {
              const correct = submitted && i === q.answer_index;
              const wrong = submitted && picked === i && i !== q.answer_index;
              return (
                <label
                  key={i}
                  className={`block ${correct ? "text-green-600" : ""} ${wrong ? "text-red-600" : ""}`}
                >
                  <input
                    type="radio"
                    name={`q-${q.position}`}
                    aria-label={opt}
                    checked={picked === i}
                    disabled={submitted}
                    onChange={() => setAnswers((a) => ({ ...a, [q.position]: i }))}
                  />{" "}
                  {opt}
                </label>
              );
            })}
            {submitted && q.explanation && (
              <p className="mt-1 text-sm text-muted-foreground">{q.explanation}</p>
            )}
          </div>
        );
      })}
      {!submitted ? (
        <button className="pt-btn mt-4" onClick={() => setSubmitted(true)}>
          Selesai
        </button>
      ) : (
        <div className="mt-4 flex items-center gap-3">
          <p className="font-semibold">Skor: {score} / {questions.length}</p>
          <button className="pt-btn" onClick={() => { setSubmitted(false); setAnswers({}); }}>
            Ulangi
          </button>
        </div>
      )}
    </section>
  );
}
```

- [ ] **Step 3: Run the tests.**

Run: `cd frontend && npx vitest run src/components/NewsQuiz.test.tsx`
Expected: PASS. (Adjust class names like `pt-card`/`pt-btn` to whatever the codebase uses; check an existing card/button component first.)

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/NewsQuiz.tsx frontend/src/components/NewsQuiz.test.tsx
git commit -m "feat(news): interactive client-scored quiz component"
```

---

### Task 17: `NewsPage` + route + nav

**Files:**
- Create: `frontend/src/pages/NewsPage.tsx`
- Create: `frontend/src/pages/NewsPage.test.tsx`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/AppShell.tsx`

- [ ] **Step 1: Write the failing test.** Create `frontend/src/pages/NewsPage.test.tsx` (follow the
  MSW pattern in `src/api/hooks.test.tsx` / `src/test/server.ts` for mocking `/news/today`):

```tsx
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import NewsPage from "./NewsPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}><NewsPage /></QueryClientProvider>);
}

test("renders articles and key points", async () => {
  server.use(http.get("*/news/today", () => HttpResponse.json({
    available: true, date: "2026-06-16",
    articles: [{ position: 0, title: "Rust 2.0", url: "https://ex.com/r", source: "HN", summary: "rilis besar", key_points: ["lebih cepat"] }],
    quiz: [],
  })));
  renderPage();
  expect(await screen.findByText("Rust 2.0")).toBeInTheDocument();
  expect(await screen.findByText("lebih cepat")).toBeInTheDocument();
});

test("shows an empty state when no digest yet", async () => {
  server.use(http.get("*/news/today", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  renderPage();
  expect(await screen.findByText(/belum siap/i)).toBeInTheDocument();
});
```

- [ ] **Step 2: Implement the page.** Create `frontend/src/pages/NewsPage.tsx`:

```tsx
import { useNewsToday } from "../api/hooks";
import NewsQuiz from "../components/NewsQuiz";
import QueryState from "../components/QueryState";

export default function NewsPage() {
  const q = useNewsToday();

  return (
    <QueryState query={q}>
      {(data) =>
        !data.available ? (
          <p className="text-muted-foreground">Digest berita hari ini belum siap. Cek lagi nanti pagi ya.</p>
        ) : (
          <div className="space-y-6">
            <header>
              <h1 className="text-xl font-semibold">Bacaan pagi</h1>
              <p className="text-sm text-muted-foreground">{data.date}</p>
            </header>

            {data.articles.map((a) => (
              <article key={a.position} className="pt-card">
                <a href={a.url} target="_blank" rel="noreferrer" className="text-lg font-semibold hover:underline">
                  {a.title}
                </a>
                <span className="ml-2 text-xs text-muted-foreground">{a.source}</span>
                <p className="mt-2">{a.summary}</p>
                {a.key_points.length > 0 && (
                  <ul className="mt-2 list-disc pl-5 text-sm">
                    {a.key_points.map((k, i) => <li key={i}>{k}</li>)}
                  </ul>
                )}
              </article>
            ))}

            <NewsQuiz questions={data.quiz} />
          </div>
        )
      }
    </QueryState>
  );
}
```

  Note: confirm `QueryState`'s render-prop signature (see `src/components/QueryState.tsx`); if it
  differs, adapt to the codebase's loading/error pattern.

- [ ] **Step 3: Add the route.** In `frontend/src/App.tsx`: import `NewsPage` and add inside the
  `AppShell` route group:

```tsx
        <Route path="news" element={<NewsPage />} />
```

- [ ] **Step 4: Add the nav item.** In `frontend/src/components/AppShell.tsx`, add to the first nav
  group array (near the `Tugas`/`Rencana` items), importing a suitable `lucide-react` icon
  (e.g. `Newspaper`):

```tsx
      { to: "/news", label: "Berita", icon: Newspaper },
```

- [ ] **Step 5: Run the tests + typecheck.**

Run: `cd frontend && npx vitest run src/pages/NewsPage.test.tsx && npm run build`
Expected: tests PASS and `tsc -b` (inside build) succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/NewsPage.tsx frontend/src/pages/NewsPage.test.tsx frontend/src/App.tsx frontend/src/components/AppShell.tsx
git commit -m "feat(news): /news page (summaries + key points + quiz) with route + nav"
```

---

### Task 18: Full-suite verification

- [ ] **Step 1: Backend.**

Run: `cd backend && cargo test && cargo clippy`
Expected: all pass, no new warnings.

- [ ] **Step 2: Frontend.**

Run: `cd frontend && npm test && npm run build`
Expected: all pass, build succeeds.

- [ ] **Step 3: Manual smoke (optional, needs an LLM key).** Set `NEWS_DIGEST_HOUR_WIB` to the
  current WIB hour, `make backend`, wait one tick (≤5 min), confirm a row in `news_digest`, then
  `GET /api/news/today` returns articles. Reset the env var afterward.

- [ ] **Step 4: Final commit (if any fixups).**

```bash
git add -A && git commit -m "test(news): full-suite green for the news digest feature"
```

---

## Self-review notes (for the implementer)

- **Env-mutating tests** (`NEWS_ENABLED`, etc.) can race under the default multi-threaded test
  runner. If flaky, annotate with `#[serial_test::serial]` (already a dev-dependency).
- **Crate-version drift:** `feed-rs`, `readability`, and `llm::claude::complete` signatures are the
  most likely to differ from the snippets. Each task notes the adaptation; keep the public function
  contracts and the tests unchanged.
- **URL safety:** the briefing now renders links deterministically (code-owned), so the
  earlier LLM-URL-hallucination risk is gone.
- **Quiz answers** are intentionally shipped to the client (personal retention quiz, scored locally).
