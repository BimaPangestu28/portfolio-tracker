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
}
