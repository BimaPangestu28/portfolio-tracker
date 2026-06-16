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
