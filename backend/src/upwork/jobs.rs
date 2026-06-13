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

/// Strip a leading list marker — an ordered `N.`/`N)` or a `-`/`*` bullet plus
/// following whitespace — without consuming digits that are part of the term
/// (e.g. "3D modeling" is left intact).
fn strip_list_marker(line: &str) -> &str {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix('-').or_else(|| t.strip_prefix('*')) {
        return rest.trim_start();
    }
    let digit_len = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digit_len > 0 {
        let after = &t[digit_len..];
        if let Some(rest) = after.strip_prefix('.').or_else(|| after.strip_prefix(')')) {
            return rest.trim_start();
        }
    }
    t
}

/// Parse search queries from the model's reply: one per line, strip a leading
/// list marker, trim, drop blanks, de-dupe case-insensitively, cap at `max`.
pub fn parse_queries(resp: &str, max: usize) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in resp.lines() {
        let cleaned = strip_list_marker(line);
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

use crate::upwork::client::{Invitation, MarketplaceJob};

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

    #[test]
    fn parse_queries_preserves_numeric_prefixed_terms() {
        let q = parse_queries("3D modeling\n2D artist", 5);
        assert_eq!(q, vec!["3D modeling", "2D artist"]);
    }

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

        let n2 = run_pass(&db, &client, &intel, &notifier, 42, &[], 7, 3).await.unwrap();
        assert_eq!(n2, 0);
    }
}
