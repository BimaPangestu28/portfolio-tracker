//! OpenAI-format vision client used for multimodal ingestion.
//!
//! The chat/proactive paths run on DeepSeek V4 (text-only) via
//! [`crate::llm::claude::ClaudeClient`]. DeepSeek V4 has no vision capability,
//! so receipt/screenshot extraction goes to a vision-capable provider over the
//! OpenAI `chat/completions` API shape — OpenAI `gpt-4o` by default, but any
//! OpenAI-compatible vision endpoint works by overriding the env config.
//!
//! Scope is deliberately narrow: a single-turn `system + parts -> text` call,
//! matching what the ingestion pipeline needs. Tool calling lives on the
//! Anthropic-shaped client and is intentionally not duplicated here.

use crate::llm::claude::{LlmError, Part};
use serde::Serialize;

/// One element of an OpenAI multimodal `content` array.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum NativeContent {
    Text { text: String },
    ImageUrl { image_url: ImageUrl },
}
#[derive(Serialize)]
struct ImageUrl {
    /// `data:<media_type>;base64,<data>` per the OpenAI vision convention.
    url: String,
}

/// Build the OpenAI `chat/completions` request body for a single user turn.
///
/// The system prompt becomes a leading `system` message; the parts become the
/// user message's multimodal `content` array. PDF parts are rejected by the
/// caller before reaching here — DeepSeek's vision model has no PDF input.
pub fn build_native_body(model: &str, system: &str, parts: &[Part]) -> Result<serde_json::Value, LlmError> {
    let content: Result<Vec<NativeContent>, LlmError> = parts.iter().map(|part| match part {
        Part::Text(text) => Ok(NativeContent::Text { text: text.clone() }),
        Part::Image(media_type, data) => Ok(NativeContent::ImageUrl {
            image_url: ImageUrl { url: format!("data:{media_type};base64,{data}") },
        }),
        Part::Pdf(_) => Err(LlmError::Unsupported(
            "PDF input is not supported by the DeepSeek vision model".into(),
        )),
    }).collect();
    let content = content?;
    Ok(serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "messages": [
            { "role": "system", "content": system },
            { "role": "user", "content": content },
        ],
    }))
}

/// Extract the assistant message text from an OpenAI `chat/completions` response.
pub fn extract_native_text(resp: &serde_json::Value) -> Result<String, LlmError> {
    let text = resp
        .get("choices")
        .and_then(|choices| choices.as_array())
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(|content| content.as_str())
        .ok_or_else(|| LlmError::Shape("no choices[0].message.content string".into()))?;
    if text.is_empty() {
        return Err(LlmError::Shape("empty message content".into()));
    }
    Ok(text.to_string())
}

/// Per-request deadline, mirroring the Anthropic-shaped client.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

/// OpenAI-format vision client for multimodal extraction.
pub struct NativeLlmClient {
    api_key: String,
    model: String,
    base_url: String,
    client: reqwest::Client,
}

impl NativeLlmClient {
    /// Reads provider config from the environment:
    /// - `OPENAI_API_KEY` — required (the vision provider's key; also used by
    ///   the memory-service for embeddings).
    /// - `INGEST_MODEL` — vision model, default `gpt-4o`.
    /// - `INGEST_BASE_URL` — OpenAI-format base URL, default
    ///   `https://api.openai.com`. `/v1/chat/completions` is appended.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("OPENAI_API_KEY").map_err(|_| LlmError::NoKey)?;
        let model = std::env::var("INGEST_MODEL").unwrap_or_else(|_| "gpt-4o".into());
        let base_url = std::env::var("INGEST_BASE_URL")
            .unwrap_or_else(|_| "https://api.openai.com".into())
            .trim_end_matches('/')
            .to_string();
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
        Ok(Self { api_key, model, base_url, client })
    }

    /// Send a single user message (system + parts) and return the text output.
    pub async fn complete(&self, system: &str, parts: &[Part]) -> Result<String, LlmError> {
        let body = build_native_body(&self.model, system, parts)?;
        let url = format!("{}/v1/chat/completions", self.base_url);
        let resp = self.client
            .post(&url)
            .header("authorization", format!("Bearer {}", self.api_key))
            .header("content-type", "application/json")
            .json(&body)
            .send().await.map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), body: json.to_string() });
        }
        extract_native_text(&json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_native_body_renders_system_and_image_content() {
        let body = build_native_body(
            "gpt-4o",
            "extract",
            &[Part::Text("hi".into()), Part::Image("image/png".into(), "AAAA".into())],
        ).unwrap();
        assert_eq!(body["model"], "gpt-4o");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages[0]["role"], "system");
        assert_eq!(messages[0]["content"], "extract");
        let content = messages[1]["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "hi");
        assert_eq!(content[1]["type"], "image_url");
        assert_eq!(content[1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn build_native_body_rejects_pdf() {
        let err = build_native_body("m", "s", &[Part::Pdf("UEs=".into())]).unwrap_err();
        assert!(matches!(err, LlmError::Unsupported(_)));
    }

    #[test]
    fn extract_native_text_reads_message_content() {
        let resp = serde_json::json!({
            "choices": [ { "message": { "role": "assistant", "content": "{\"a\":1}" } } ]
        });
        assert_eq!(extract_native_text(&resp).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn extract_native_text_errors_without_choices() {
        let resp = serde_json::json!({ "choices": [] });
        assert!(matches!(extract_native_text(&resp), Err(LlmError::Shape(_))));
    }

    #[test]
    fn extract_native_text_errors_on_empty_content() {
        let resp = serde_json::json!({ "choices": [ { "message": { "content": "" } } ] });
        assert!(matches!(extract_native_text(&resp), Err(LlmError::Shape(_))));
    }
}
