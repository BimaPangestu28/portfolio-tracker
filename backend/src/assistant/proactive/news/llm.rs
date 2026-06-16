//! Turn article text into a summary + key points, and build a retention quiz.
//! Every LLM path degrades deterministically (consistent with compose.rs).

use crate::llm::claude::{ClaudeClient, Part};
use serde::Deserialize;

pub const SUMMARY_SYSTEM: &str = "You summarize one IT/dev news item in Indonesian for a senior \
engineer who wants the substance, not fluff. Output ONLY minified JSON: {\"summary\": string, \
\"key_points\": string[]}. Write summary as ONE substantial paragraph of about 5-7 sentences that \
explains concretely WHAT the thing is, WHAT it actually does, WHY it matters, and the key technical \
or practical detail — extract specifics. Do NOT merely restate or paraphrase the title. If the \
provided text includes a 'Diskusi Hacker News' section, use those comments to explain what the \
project really does and the community's reaction. key_points = 3-5 concrete, specific bullets (not \
vague restatements). Use ONLY the provided text; never invent facts. No markdown, no code fences.";

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

/// A deterministic retention quiz used when the LLM path yields nothing: one
/// question per article, "which article covers this key point?", options = the
/// article titles. Needs >= 2 articles to have distractors; else empty.
pub fn fallback_quiz(items: &[(String, Vec<String>)]) -> Vec<QuizItem> {
    let titles: Vec<String> = items.iter().map(|(t, _)| t.clone()).collect();
    if titles.len() < 2 {
        return vec![];
    }
    items
        .iter()
        .enumerate()
        .filter_map(|(i, (_title, key_points))| {
            let kp = key_points.first()?;
            Some(QuizItem {
                question: format!("Artikel mana yang membahas: \"{kp}\"?"),
                options: titles.clone(),
                answer_index: i as i64,
                explanation: String::new(),
                article_position: i as i64,
            })
        })
        .collect()
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

    #[test]
    fn strip_fences_handles_plain_and_unlabelled_fences() {
        // No fence.
        assert_eq!(strip_fences("{\"a\":1}"), "{\"a\":1}");
        // Unlabelled ``` fence (some models omit the `json` tag).
        assert_eq!(strip_fences("```\n{\"a\":1}\n```"), "{\"a\":1}");
        // A summary fenced without a language tag still parses.
        let s = parse_summary("```\n{\"summary\":\"r\",\"key_points\":[]}\n```").unwrap();
        assert_eq!(s.summary, "r");
    }

    #[test]
    fn fallback_quiz_builds_one_question_per_article_with_key_point() {
        let items = vec![
            ("Rust 2.0".to_string(), vec!["lebih cepat".to_string()]),
            ("K8s news".to_string(), vec!["operator baru".to_string()]),
            ("No points".to_string(), vec![]),
        ];
        let q = fallback_quiz(&items);
        assert_eq!(q.len(), 2); // the keypoint-less one is skipped
        assert_eq!(q[0].answer_index, 0);
        assert_eq!(q[0].options.len(), 3);
        assert!(q[0].question.contains("lebih cepat"));
        assert_eq!(q[1].answer_index, 1);
    }

    #[test]
    fn fallback_quiz_needs_two_articles() {
        assert!(fallback_quiz(&[("solo".into(), vec!["x".into()])]).is_empty());
    }
}
