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
}
