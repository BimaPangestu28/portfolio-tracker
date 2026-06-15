# Upwork Job & Invitation Notifications Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Poll Upwork on an interval for new direct invitations and skill-relevant marketplace jobs, and push the relevant ones to the owner's Telegram.

**Architecture:** Extend the mockable `UpworkClient` with job/invitation fetches; add a `upwork/jobs.rs` module of pure helpers (query-derivation + relevance-scoring prompts/parsers + alert formatting) plus an orchestration `notify_cycle` built on three seams (`UpworkClient`, an LLM `JobIntel`, a `Notifier`) so it is fully testable with fakes. Dedup reuses `proactive_log::try_claim`; delivery reuses `TelegramClient`. A dedicated 30-minute loop drives it.

**Tech Stack:** Rust, async-trait, reqwest, serde_json, the existing `upwork`/`assistant::memory`/`llm::claude`/`telegram`/`repo::proactive_log`/`repo::telegram_link` modules. No DB migration, no frontend.

---

## File Structure

| Path | Create/Modify | Responsibility |
|---|---|---|
| `backend/src/upwork/client.rs` | Modify | Add `MarketplaceJob`/`Invitation`/`InvitationBatch`; two trait methods; `HttpUpwork` impls; extend `FakeUpwork`. |
| `backend/src/upwork/mod.rs` | Modify | `pub mod jobs;`. |
| `backend/src/upwork/jobs.rs` | Create | Pure prompt/parse/format helpers, `JobIntel`/`Notifier` seams + real impls, `run_pass`, `notify_cycle`, `spawn`. |
| `backend/src/upwork/engine.rs` | Modify | `ensure_access_token` → `pub(crate)` (DRY, shared with jobs). |
| `backend/src/main.rs` | Modify | `upwork::jobs::spawn(db.clone());`. |

**Verified facts (do not re-derive):**
- `UpworkClient` trait + `ClientError { Http(String), Parse(String) }` + `HttpUpwork { access_token, http }` + `testkit::FakeUpwork` live in `upwork/client.rs`. `HttpUpwork` posts to `https://api.upwork.com/graphql` with `.bearer_auth(&self.access_token)`.
- `upwork::engine::ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8;32]) -> anyhow::Result<String>` (currently private). `upwork::oauth::OAuthConfig::from_env() -> anyhow::Result<OAuthConfig>`. `upwork::crypto::key_from_env() -> anyhow::Result<[u8;32]>`. `upwork::engine::spawn` does NOT exist (earnings is manual).
- `repo::proactive_log::try_claim(db: &Db, kind: &str, dedup_key: &str) -> anyhow::Result<bool>` (true = first claim).
- `repo::telegram_link::get(db: &Db) -> anyhow::Result<Option<TelegramLinkRow>>` where `TelegramLinkRow { chat_id: i64, username: Option<String>, linked_at: String }`.
- `telegram::client::TelegramClient::new(token: String)` and `send_message(&self, chat_id: i64, text: &str) -> Result<(), TgError>`.
- `assistant::memory::MemoryClient::from_env() -> Option<MemoryClient>`, `search(&self, query: &str, limit: u32) -> Vec<MemoryFact>`. `MemoryFact { pub fact: String, pub valid_at: Option<String>, pub name: String }`, `render_facts_block(&[MemoryFact]) -> String`.
- `llm::claude::ClaudeClient::from_env() -> Result<ClaudeClient, LlmError>`, `complete(&self, system: &str, parts: &[Part]) -> Result<String, LlmError>`, `Part::Text(String)`.
- `google::engine::spawn` loop pattern: gate on `OAuthConfig::from_env().is_err()` → return; else `tokio::spawn(async move { loop { run_cycle().await; sleep(TICK).await; } })`.

---

## Task 1: `UpworkClient` job/invitation fetch

**Files:**
- Modify: `backend/src/upwork/client.rs`

- [ ] **Step 1: Add the new types** (after the `TransactionBatch` struct):

```rust
/// A marketplace job posting surfaced by search.
#[derive(Debug, Clone, PartialEq)]
pub struct MarketplaceJob {
    pub id: String,
    pub title: String,
    pub description: String,
    pub budget: Option<String>,
    pub url: String,
    pub skills: Vec<String>,
}

/// A direct invitation from a client.
#[derive(Debug, Clone, PartialEq)]
pub struct Invitation {
    pub id: String,
    pub job_title: String,
    pub client_note: Option<String>,
    pub url: String,
}

#[derive(Debug, Default)]
pub struct InvitationBatch {
    pub invitations: Vec<Invitation>,
    pub next_cursor: Option<String>,
}
```

- [ ] **Step 2: Add two methods to the `UpworkClient` trait** (inside the `pub trait UpworkClient` block, after `fetch_transactions`):

```rust
    /// Search the marketplace for jobs matching `query`.
    async fn fetch_marketplace_jobs(&self, query: &str) -> Result<Vec<MarketplaceJob>, ClientError>;
    /// Fetch the freelancer's direct invitations (None cursor = from the beginning).
    async fn fetch_invitations(&self, cursor: Option<&str>) -> Result<InvitationBatch, ClientError>;
```

- [ ] **Step 3: Implement them for `HttpUpwork`** (inside `impl UpworkClient for HttpUpwork`, after `fetch_transactions`):

```rust
    async fn fetch_marketplace_jobs(&self, query: &str) -> Result<Vec<MarketplaceJob>, ClientError> {
        let gql = r#"
            query($q: String!) {
              marketplaceJobPostingsSearch(query: $q) {
                edges { node { id title description amount { rawValue } ciphertext skills { name } } }
              }
            }"#;
        let body = serde_json::json!({ "query": gql, "variables": { "q": query } });
        let resp = self.http.post(GRAPHQL_ENDPOINT).bearer_auth(&self.access_token).json(&body)
            .send().await.map_err(|e| ClientError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("{}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| ClientError::Parse(e.to_string()))?;
        let edges = v["data"]["marketplaceJobPostingsSearch"]["edges"]
            .as_array().ok_or_else(|| ClientError::Parse("missing marketplace edges".into()))?;
        let mut jobs = Vec::with_capacity(edges.len());
        for e in edges {
            let n = &e["node"];
            let id = n["id"].as_str().unwrap_or_default().to_string();
            let cipher = n["ciphertext"].as_str().unwrap_or(&id);
            let skills = n["skills"].as_array().map(|a| {
                a.iter().filter_map(|s| s["name"].as_str().map(|x| x.to_string())).collect()
            }).unwrap_or_default();
            jobs.push(MarketplaceJob {
                title: n["title"].as_str().unwrap_or_default().to_string(),
                description: n["description"].as_str().unwrap_or_default().to_string(),
                budget: n["amount"]["rawValue"].as_str().map(|s| s.to_string()),
                url: format!("https://www.upwork.com/jobs/{cipher}"),
                skills,
                id,
            });
        }
        Ok(jobs)
    }

    async fn fetch_invitations(&self, cursor: Option<&str>) -> Result<InvitationBatch, ClientError> {
        let gql = r#"
            query($after: String) {
              freelancerInvitations(after: $after) {
                edges { node { id jobTitle clientNote ciphertext } }
                pageInfo { endCursor }
              }
            }"#;
        let body = serde_json::json!({ "query": gql, "variables": { "after": cursor } });
        let resp = self.http.post(GRAPHQL_ENDPOINT).bearer_auth(&self.access_token).json(&body)
            .send().await.map_err(|e| ClientError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(ClientError::Http(format!("{}", resp.status())));
        }
        let v: serde_json::Value = resp.json().await.map_err(|e| ClientError::Parse(e.to_string()))?;
        let edges = v["data"]["freelancerInvitations"]["edges"]
            .as_array().ok_or_else(|| ClientError::Parse("missing invitation edges".into()))?;
        let mut invitations = Vec::with_capacity(edges.len());
        for e in edges {
            let n = &e["node"];
            let id = n["id"].as_str().unwrap_or_default().to_string();
            let cipher = n["ciphertext"].as_str().unwrap_or(&id);
            invitations.push(Invitation {
                job_title: n["jobTitle"].as_str().unwrap_or_default().to_string(),
                client_note: n["clientNote"].as_str().map(|s| s.to_string()),
                url: format!("https://www.upwork.com/jobs/{cipher}"),
                id,
            });
        }
        let next_cursor = v["data"]["freelancerInvitations"]["pageInfo"]["endCursor"].as_str().map(|s| s.to_string());
        Ok(InvitationBatch { invitations, next_cursor })
    }
```

- [ ] **Step 4: Extend `FakeUpwork`** so it can also serve jobs + invitations. Replace the entire `testkit` module with:

```rust
#[cfg(test)]
pub mod testkit {
    use super::*;
    use std::sync::Mutex;

    /// In-memory client. `with(...)` seeds transactions; `jobs`/`invitations`
    /// seed the notification sources. Records the last query/cursor seen.
    pub struct FakeUpwork {
        pub batch: Mutex<TransactionBatch>,
        pub jobs: Mutex<Vec<MarketplaceJob>>,
        pub invitations: Mutex<Vec<Invitation>>,
        pub seen_cursor: Mutex<Option<String>>,
        pub seen_query: Mutex<Option<String>>,
    }
    impl FakeUpwork {
        pub fn with(txns: Vec<UpworkTransaction>, next_cursor: Option<String>) -> Self {
            Self {
                batch: Mutex::new(TransactionBatch { txns, next_cursor }),
                jobs: Mutex::new(Vec::new()),
                invitations: Mutex::new(Vec::new()),
                seen_cursor: Mutex::new(None),
                seen_query: Mutex::new(None),
            }
        }
        pub fn with_notifications(jobs: Vec<MarketplaceJob>, invitations: Vec<Invitation>) -> Self {
            let mut f = Self::with(Vec::new(), None);
            *f.jobs.get_mut().unwrap() = jobs;
            *f.invitations.get_mut().unwrap() = invitations;
            f
        }
    }
    #[async_trait]
    impl UpworkClient for FakeUpwork {
        async fn fetch_transactions(&self, cursor: Option<&str>) -> Result<TransactionBatch, ClientError> {
            *self.seen_cursor.lock().unwrap() = cursor.map(|c| c.to_string());
            let b = self.batch.lock().unwrap();
            Ok(TransactionBatch { txns: b.txns.clone(), next_cursor: b.next_cursor.clone() })
        }
        async fn fetch_marketplace_jobs(&self, query: &str) -> Result<Vec<MarketplaceJob>, ClientError> {
            *self.seen_query.lock().unwrap() = Some(query.to_string());
            Ok(self.jobs.lock().unwrap().clone())
        }
        async fn fetch_invitations(&self, cursor: Option<&str>) -> Result<InvitationBatch, ClientError> {
            *self.seen_cursor.lock().unwrap() = cursor.map(|c| c.to_string());
            Ok(InvitationBatch { invitations: self.invitations.lock().unwrap().clone(), next_cursor: None })
        }
    }
}
```

- [ ] **Step 5: Run + commit**

Run: `cd backend && cargo test upwork::client::`
Expected: the existing `fake_returns_preset_batch_and_records_cursor` test still PASSES; compiles clean.

```bash
git add backend/src/upwork/client.rs
git commit -m "feat(jobs): UpworkClient marketplace-jobs + invitations fetch"
```

---

## Task 2: `jobs.rs` — query derivation (prompt + parse)

**Files:**
- Create: `backend/src/upwork/jobs.rs`
- Modify: `backend/src/upwork/mod.rs`

- [ ] **Step 1: Declare the module.** In `backend/src/upwork/mod.rs`, add after `pub mod engine;`:

```rust
pub mod jobs;
```

- [ ] **Step 2: Create `backend/src/upwork/jobs.rs` with the query helpers + tests:**

```rust
//! Upwork job & invitation notifications: derive watch-queries from memory
//! skills, score marketplace jobs for relevance, format Telegram alerts, and
//! the polling orchestration. Pure helpers here; orchestration at the bottom.

use crate::assistant::memory::{render_facts_block, MemoryFact};

/// Build the prompt that turns the owner's skill facts into Upwork search terms.
pub fn build_query_prompt(facts: &[MemoryFact]) -> String {
    format!(
        "From the owner's skills/experience below, output up to 5 short Upwork marketplace \
search queries (1-3 words each) that would surface relevant jobs. Output ONE query per line, \
no numbering, no extra text. Use only skills actually present below.\n{}",
        render_facts_block(facts)
    )
}

/// Parse search queries from the model's reply: one per line, strip bullets/
/// numbering, trim, drop blanks, de-dupe case-insensitively, cap at `max`.
pub fn parse_queries(resp: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in resp.lines() {
        let cleaned = line
            .trim()
            .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c == ')' || c == '-' || c == '*' || c == ' ')
            .trim();
        if cleaned.is_empty() {
            continue;
        }
        let key = cleaned.to_ascii_lowercase();
        if seen.insert(key) {
            out.push(cleaned.to_string());
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(text: &str) -> MemoryFact {
        MemoryFact { fact: text.to_string(), valid_at: None, name: "REL".to_string() }
    }

    #[test]
    fn query_prompt_includes_facts_and_asks_for_search_terms() {
        let p = build_query_prompt(&[fact("Expert in Rust and Postgres")]);
        assert!(p.to_lowercase().contains("search"));
        assert!(p.contains("Expert in Rust and Postgres"));
    }

    #[test]
    fn parse_queries_cleans_numbering_and_caps_and_dedupes() {
        let resp = "1. rust backend\n- React\n2) rust backend\n\n  postgres  \n* GraphQL\nNext.js";
        let q = parse_queries(resp, 3);
        assert_eq!(q, vec!["rust backend", "React", "postgres"]);
    }

    #[test]
    fn parse_queries_empty_input_is_empty() {
        assert!(parse_queries("", 5).is_empty());
    }
}
```

- [ ] **Step 3: Run + commit**

Run: `cd backend && cargo test upwork::jobs::`
Expected: 3 tests PASS.

```bash
git add backend/src/upwork/jobs.rs backend/src/upwork/mod.rs
git commit -m "feat(jobs): derive watch-queries from memory skills"
```

---

## Task 3: `jobs.rs` — relevance scoring (prompt + parse)

**Files:**
- Modify: `backend/src/upwork/jobs.rs`

- [ ] **Step 1: Add scoring code** (above the `#[cfg(test)]` block):

```rust
use crate::upwork::client::MarketplaceJob;

/// A relevance verdict for one job.
#[derive(Debug, Clone, PartialEq)]
pub struct JobScore {
    pub id: String,
    pub score: u8,   // 0..=10
    pub reason: String,
}

/// Build the batch relevance-scoring prompt. Asks for one `id|score|reason`
/// line per job, scored 0-10 against the owner's skills only.
pub fn build_scoring_prompt(jobs: &[MarketplaceJob], facts: &[MemoryFact]) -> String {
    let mut listing = String::new();
    for j in jobs {
        listing.push_str(&format!(
            "JOB id={}\nTitle: {}\nSkills: {}\nDescription: {}\n\n",
            j.id, j.title, j.skills.join(", "), j.description,
        ));
    }
    format!(
        "Score how well each job below fits the owner's skills, in English, using ONLY the \
skills/experience facts provided — never assume skills not listed. For EACH job output exactly \
one line: the job id, a score 0-10, and a one-sentence reason, separated by ' | ' (a pipe). \
No header, no extra lines.\n\nOWNER SKILLS:{}\n\nJOBS:\n{}",
        render_facts_block(facts), listing,
    )
}

/// Parse `id | score | reason` lines. Lines that don't parse are dropped (that
/// job is simply not notified). Scores are clamped to 0..=10.
pub fn parse_scores(resp: &str) -> Vec<JobScore> {
    let mut out = Vec::new();
    for line in resp.lines() {
        let parts: Vec<&str> = line.splitn(3, '|').map(|s| s.trim()).collect();
        if parts.len() < 3 {
            continue;
        }
        let id = parts[0].trim_start_matches("id=").trim();
        let Ok(raw) = parts[1].parse::<i64>() else { continue };
        if id.is_empty() {
            continue;
        }
        out.push(JobScore {
            id: id.to_string(),
            score: raw.clamp(0, 10) as u8,
            reason: parts[2].to_string(),
        });
    }
    out
}
```

- [ ] **Step 2: Add tests** (inside the existing `tests` module):

```rust
    fn job(id: &str, title: &str) -> MarketplaceJob {
        MarketplaceJob {
            id: id.into(), title: title.into(), description: "d".into(),
            budget: None, url: "u".into(), skills: vec!["Rust".into()],
        }
    }

    #[test]
    fn scoring_prompt_demands_score_and_only_listed_skills() {
        let p = build_scoring_prompt(&[job("1", "Rust API")], &[fact("Rust expert")]);
        let lower = p.to_lowercase();
        assert!(lower.contains("0-10"));
        assert!(lower.contains("only"));
        assert!(p.contains("Rust API"));
    }

    #[test]
    fn parse_scores_reads_pipe_rows_and_drops_garbage() {
        let resp = "1 | 8 | Strong Rust fit\ngarbage line\n2 | 99 | clamped high\n3 | x | bad score";
        let scores = parse_scores(resp);
        assert_eq!(scores.len(), 2);
        assert_eq!(scores[0], JobScore { id: "1".into(), score: 8, reason: "Strong Rust fit".into() });
        assert_eq!(scores[1].score, 10); // 99 clamped
    }
```

- [ ] **Step 3: Run + commit**

Run: `cd backend && cargo test upwork::jobs::`
Expected: all PASS (5 tests).

```bash
git add backend/src/upwork/jobs.rs
git commit -m "feat(jobs): LLM relevance scoring prompt + parser"
```

---

## Task 4: `jobs.rs` — alert formatting

**Files:**
- Modify: `backend/src/upwork/jobs.rs`

- [ ] **Step 1: Add formatters** (above the `#[cfg(test)]` block):

```rust
use crate::upwork::client::Invitation;

/// Plain-text Telegram alert for a relevant marketplace job (no Markdown).
pub fn format_job_alert(job: &MarketplaceJob, score: u8, reason: &str) -> String {
    let mut msg = format!("🧑‍💻 New Upwork job (match {score}/10)\n{}\n", job.title);
    if let Some(b) = &job.budget {
        msg.push_str(&format!("💰 {b}\n"));
    }
    msg.push_str(&format!("📝 {reason}\n🔗 {}", job.url));
    msg
}

/// Plain-text Telegram alert for a direct invitation (no Markdown).
pub fn format_invitation_alert(inv: &Invitation) -> String {
    let mut msg = format!("📨 Upwork invitation\n{}\n", inv.job_title);
    if let Some(note) = &inv.client_note {
        msg.push_str(&format!("🗒 {note}\n"));
    }
    msg.push_str(&format!("🔗 {}", inv.url));
    msg
}
```

- [ ] **Step 2: Add tests** (inside the `tests` module):

```rust
    #[test]
    fn job_alert_has_title_score_url_no_markdown() {
        let mut j = job("1", "Senior Rust Engineer");
        j.budget = Some("$50/hr".into());
        j.url = "https://www.upwork.com/jobs/abc".into();
        let msg = format_job_alert(&j, 9, "Great Rust match");
        assert!(msg.contains("Senior Rust Engineer"));
        assert!(msg.contains("9/10"));
        assert!(msg.contains("$50/hr"));
        assert!(msg.contains("https://www.upwork.com/jobs/abc"));
        assert!(!msg.contains("**"));
    }

    #[test]
    fn invitation_alert_has_title_and_url() {
        let inv = Invitation {
            id: "i1".into(), job_title: "Build an API".into(),
            client_note: Some("saw your profile".into()), url: "https://www.upwork.com/jobs/xyz".into(),
        };
        let msg = format_invitation_alert(&inv);
        assert!(msg.contains("Build an API"));
        assert!(msg.contains("saw your profile"));
        assert!(msg.contains("https://www.upwork.com/jobs/xyz"));
        assert!(!msg.contains("**"));
    }
```

- [ ] **Step 3: Run + commit**

Run: `cd backend && cargo test upwork::jobs::`
Expected: all PASS (7 tests).

```bash
git add backend/src/upwork/jobs.rs
git commit -m "feat(jobs): Telegram alert formatting"
```

---

## Task 5: `jobs.rs` — seams + `run_pass` orchestration

**Files:**
- Modify: `backend/src/upwork/jobs.rs`

- [ ] **Step 1: Add the seams, real impls, and `run_pass`** (above the `#[cfg(test)]` block):

```rust
use crate::db::Db;
use crate::llm::claude::{ClaudeClient, Part};
use crate::repo::proactive_log;
use crate::upwork::client::UpworkClient;
use async_trait::async_trait;

/// LLM-backed intelligence seam (query derivation + job scoring). Real impl uses
/// the chat model; tests inject a fake.
#[async_trait]
pub trait JobIntel: Send + Sync {
    async fn derive_queries(&self, facts: &[MemoryFact], max: usize) -> Vec<String>;
    async fn score_jobs(&self, jobs: &[MarketplaceJob], facts: &[MemoryFact]) -> Vec<JobScore>;
}

/// Telegram delivery seam. Real impl wraps `TelegramClient`; tests record sends.
#[async_trait]
pub trait Notifier: Send + Sync {
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), String>;
}

/// Production `JobIntel`: builds the prompts and calls the chat model. Any LLM
/// failure degrades to an empty result (no queries / no scores).
pub struct LlmJobIntel;

#[async_trait]
impl JobIntel for LlmJobIntel {
    async fn derive_queries(&self, facts: &[MemoryFact], max: usize) -> Vec<String> {
        if facts.is_empty() {
            return Vec::new();
        }
        let Ok(client) = ClaudeClient::from_env() else { return Vec::new() };
        match client.complete("You output Upwork search queries.", &[Part::Text(build_query_prompt(facts))]).await {
            Ok(text) => parse_queries(&text, max),
            Err(e) => { tracing::warn!("job query derivation failed: {e}"); Vec::new() }
        }
    }
    async fn score_jobs(&self, jobs: &[MarketplaceJob], facts: &[MemoryFact]) -> Vec<JobScore> {
        if jobs.is_empty() {
            return Vec::new();
        }
        let Ok(client) = ClaudeClient::from_env() else { return Vec::new() };
        match client.complete("You score Upwork job relevance.", &[Part::Text(build_scoring_prompt(jobs, facts))]).await {
            Ok(text) => parse_scores(&text),
            Err(e) => { tracing::warn!("job scoring failed: {e}"); Vec::new() }
        }
    }
}

/// Production `Notifier`: sends over Telegram.
pub struct TelegramNotifier {
    pub client: crate::telegram::client::TelegramClient,
}
#[async_trait]
impl Notifier for TelegramNotifier {
    async fn send(&self, chat_id: i64, text: &str) -> Result<(), String> {
        self.client.send_message(chat_id, text).await.map_err(|e| e.to_string())
    }
}

/// One notification pass against injected seams. Returns the number of messages
/// sent. Pure DB + traits, so tests drive it with fakes.
pub async fn run_pass<C: UpworkClient, I: JobIntel, N: Notifier>(
    db: &Db,
    client: &C,
    intel: &I,
    notifier: &N,
    chat_id: i64,
    facts: &[MemoryFact],
    threshold: u8,
    max_queries: usize,
) -> anyhow::Result<usize> {
    let mut sent = 0usize;

    // --- Invitations: always notify newly-seen ones ---
    match client.fetch_invitations(None).await {
        Ok(batch) => {
            for inv in &batch.invitations {
                if proactive_log::try_claim(db, "upwork-invite", &inv.id).await? {
                    if notifier.send(chat_id, &format_invitation_alert(inv)).await.is_ok() {
                        sent += 1;
                    }
                }
            }
        }
        Err(e) => tracing::warn!("fetch invitations failed: {e}"),
    }

    // --- Marketplace: derive queries, fetch, claim-new, score, notify >= threshold ---
    let queries = intel.derive_queries(facts, max_queries).await;
    let mut new_jobs: Vec<MarketplaceJob> = Vec::new();
    for q in &queries {
        match client.fetch_marketplace_jobs(q).await {
            Ok(jobs) => {
                for job in jobs {
                    if proactive_log::try_claim(db, "upwork-job", &job.id).await? {
                        new_jobs.push(job);
                    }
                }
            }
            Err(e) => tracing::warn!("fetch jobs for '{q}' failed: {e}"),
        }
    }
    if !new_jobs.is_empty() {
        let scores = intel.score_jobs(&new_jobs, facts).await;
        for job in &new_jobs {
            if let Some(s) = scores.iter().find(|s| s.id == job.id) {
                if s.score >= threshold
                    && notifier.send(chat_id, &format_job_alert(job, s.score, &s.reason)).await.is_ok()
                {
                    sent += 1;
                }
            }
        }
    }
    Ok(sent)
}
```

- [ ] **Step 2: Add a testkit + orchestration tests** (inside the `tests` module):

```rust
    use crate::upwork::client::testkit::FakeUpwork;
    use crate::upwork::client::Invitation;
    use std::sync::Mutex;

    struct FakeIntel { queries: Vec<String>, scores: Vec<JobScore> }
    #[async_trait::async_trait]
    impl JobIntel for FakeIntel {
        async fn derive_queries(&self, _f: &[MemoryFact], _m: usize) -> Vec<String> { self.queries.clone() }
        async fn score_jobs(&self, _j: &[MarketplaceJob], _f: &[MemoryFact]) -> Vec<JobScore> { self.scores.clone() }
    }
    #[derive(Default)]
    struct CapturingNotifier { sent: Mutex<Vec<String>> }
    #[async_trait::async_trait]
    impl Notifier for CapturingNotifier {
        async fn send(&self, _chat: i64, text: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }
    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn sends_invitations_and_above_threshold_jobs_then_dedupes() {
        let db = mem_db().await;
        let jobs = vec![job("j1", "Rust API"), job("j2", "WordPress")];
        let invites = vec![Invitation { id: "i1".into(), job_title: "Direct gig".into(), client_note: None, url: "u".into() }];
        let client = FakeUpwork::with_notifications(jobs, invites);
        let intel = FakeIntel {
            queries: vec!["rust".into()],
            scores: vec![
                JobScore { id: "j1".into(), score: 9, reason: "fit".into() },
                JobScore { id: "j2".into(), score: 3, reason: "weak".into() },
            ],
        };
        let notifier = CapturingNotifier::default();

        let n = run_pass(&db, &client, &intel, &notifier, 42, &[], 7, 3).await.unwrap();
        assert_eq!(n, 2, "1 invitation + 1 above-threshold job");
        let sent = notifier.sent.lock().unwrap().clone();
        assert!(sent.iter().any(|m| m.contains("Direct gig")));
        assert!(sent.iter().any(|m| m.contains("Rust API")));
        assert!(!sent.iter().any(|m| m.contains("WordPress")), "below-threshold job not sent");

        // Second pass: everything already claimed → nothing new.
        let n2 = run_pass(&db, &client, &intel, &notifier, 42, &[], 7, 3).await.unwrap();
        assert_eq!(n2, 0);
    }
```

- [ ] **Step 3: Run + commit**

Run: `cd backend && cargo test upwork::jobs::`
Expected: all PASS (8 tests).

```bash
git add backend/src/upwork/jobs.rs
git commit -m "feat(jobs): notify run_pass with intel + notifier seams"
```

---

## Task 6: `notify_cycle`, loop, token sharing, wiring

**Files:**
- Modify: `backend/src/upwork/engine.rs` (token helper visibility)
- Modify: `backend/src/upwork/jobs.rs` (cycle + spawn)
- Modify: `backend/src/main.rs` (spawn)

- [ ] **Step 1: Make the token helper shareable.** In `backend/src/upwork/engine.rs`, change the signature line:

```rust
async fn ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8; 32]) -> anyhow::Result<String> {
```
to:
```rust
pub(crate) async fn ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8; 32]) -> anyhow::Result<String> {
```
(No other change to engine.rs.)

- [ ] **Step 2: Add `notify_cycle` + `spawn` to `jobs.rs`** (above the `#[cfg(test)]` block):

```rust
use crate::repo::{telegram_link, upwork_integration};
use crate::upwork::client::HttpUpwork;
use crate::upwork::oauth::OAuthConfig;

const DEFAULT_POLL_SECS: u64 = 1800;
const DEFAULT_THRESHOLD: u8 = 7;
const DEFAULT_MAX_QUERIES: usize = 3;
const FACT_LIMIT: u32 = 8;

fn env_u8(key: &str, default: u8) -> u8 {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).map(|n: u8| n.min(10)).unwrap_or(default)
}
fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key).ok().and_then(|v| v.parse().ok()).unwrap_or(default)
}

/// One full notification cycle: resolve owner + token, pull skills, run a pass.
pub async fn notify_cycle(db: &Db) -> anyhow::Result<usize> {
    let Some(link) = telegram_link::get(db).await? else { return Ok(0) };
    let cfg = OAuthConfig::from_env()?;
    let key = crate::upwork::crypto::key_from_env()?;
    let token = match crate::upwork::engine::ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(0);
        }
    };
    let Ok(tg_token) = std::env::var("TELEGRAM_BOT_TOKEN") else { return Ok(0) };

    let facts = match crate::assistant::memory::MemoryClient::from_env() {
        Some(m) => m.search("skills experience expertise", FACT_LIMIT).await,
        None => Vec::new(),
    };

    let client = HttpUpwork::new(token);
    let intel = LlmJobIntel;
    let notifier = TelegramNotifier { client: crate::telegram::client::TelegramClient::new(tg_token) };
    run_pass(
        db, &client, &intel, &notifier, link.chat_id, &facts,
        env_u8("UPWORK_JOB_SCORE_THRESHOLD", DEFAULT_THRESHOLD),
        env_usize("UPWORK_MAX_WATCH_QUERIES", DEFAULT_MAX_QUERIES),
    ).await
}

/// Independent polling loop. No-op when Upwork OAuth env is unset.
pub fn spawn(db: Db) {
    if OAuthConfig::from_env().is_err() {
        tracing::info!("UPWORK_CLIENT_* not set; job notifications disabled");
        return;
    }
    let secs = std::env::var("UPWORK_JOBS_POLL_SECS").ok().and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_POLL_SECS);
    let period = std::time::Duration::from_secs(secs);
    tokio::spawn(async move {
        loop {
            match notify_cycle(&db).await {
                Ok(n) if n > 0 => tracing::info!("upwork job notif: sent {n}"),
                Ok(_) => {}
                Err(e) => tracing::warn!("upwork job notif cycle failed: {e:#}"),
            }
            tokio::time::sleep(period).await;
        }
    });
}
```

Note: confirm `upwork_integration::set_status(db, &str, Option<&str>)` exists (it is used by `engine.rs`); if its signature differs, match the call in `engine.rs::run_cycle`.

- [ ] **Step 3: Wire the loop in `main.rs`.** In `backend/src/main.rs`, after the line `google::engine::spawn(db.clone());` (line ~48), add:

```rust
    upwork::jobs::spawn(db.clone());
```

- [ ] **Step 4: Build + full test + commit**

Run: `cd backend && cargo build` then `cargo test upwork::`
Expected: clean build; all upwork tests pass.

```bash
git add backend/src/upwork/jobs.rs backend/src/upwork/engine.rs backend/src/main.rs
git commit -m "feat(jobs): notify_cycle + 30m polling loop + wiring"
```

---

## Final verification

- [ ] `cd backend && cargo test` → all green (ignored live tests stay ignored).
- [ ] `cd backend && cargo build` → clean, no new warnings.
- [ ] **Manual smoke (after API key + Telegram link + LLM configured):** set `UPWORK_*` + `TELEGRAM_BOT_TOKEN`, link the Telegram owner, wait one poll interval (or call `upwork::jobs::notify_cycle` from a temporary route); confirm invitations and high-scoring jobs arrive once, and a second cycle sends nothing (dedup). With services unconfigured, confirm `notify_cycle` returns `Ok(0)` without panicking and the loop stays quiet.

---

## Self-review notes (author)

- **Spec coverage:** client extension + fakes (Task 1), query derivation (Task 2), scoring (Task 3), alert formatting (Task 4), seams + run_pass dedup/threshold/invitation logic (Task 5), notify_cycle + loop + token-sharing + env + wiring (Task 6). Error handling: owner-unlinked/token/memory/LLM/per-query/per-send all handled in run_pass + notify_cycle. Dedup via `proactive_log` (kinds `upwork-job`/`upwork-invite`), no migration. Out-of-scope items (chat-managed queries, history, auto-apply, web UI) have no tasks.
- **Type consistency:** `MarketplaceJob`/`Invitation`/`InvitationBatch` (client.rs) → `JobScore`/`build_*`/`parse_*`/`format_*` (jobs.rs) → `JobIntel`/`Notifier`/`run_pass`/`notify_cycle`/`spawn`. `try_claim(db, kind, key)`, `telegram_link::get`, `TelegramClient::new`/`send_message`, `ensure_access_token` (now pub(crate)), `MemoryClient::search` all match the verified-facts list.
- **No DB/migration/portfolio/cashflow code touched.** Earnings `engine.rs` change is visibility-only.
