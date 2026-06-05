use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("missing ANTHROPIC_API_KEY")]
    NoKey,
    #[error("http error: {0}")]
    Http(String),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

#[derive(Debug, Clone)]
pub enum Part {
    Text(String),
    /// (media_type, base64 data) — e.g. ("image/png", "...")
    Image(String, String),
    /// base64 PDF data
    Pdf(String),
}

#[derive(Serialize)]
struct ContentBlock {
    #[serde(rename = "type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    source: Option<Source>,
}
#[derive(Serialize)]
struct Source {
    #[serde(rename = "type")]
    kind: &'static str, // "base64"
    media_type: String,
    data: String,
}

/// Build the JSON body for the Anthropic Messages API.
pub fn build_body(model: &str, system: &str, parts: &[Part]) -> serde_json::Value {
    let blocks: Vec<ContentBlock> = parts.iter().map(|p| match p {
        Part::Text(t) => ContentBlock { kind: "text", text: Some(t.clone()), source: None },
        Part::Image(mt, data) => ContentBlock { kind: "image", text: None, source: Some(Source { kind: "base64", media_type: mt.clone(), data: data.clone() }) },
        Part::Pdf(data) => ContentBlock { kind: "document", text: None, source: Some(Source { kind: "base64", media_type: "application/pdf".into(), data: data.clone() }) },
    }).collect();
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": [{ "role": "user", "content": blocks }]
    })
}

/// Build the JSON body for a multi-turn text conversation. Leading assistant
/// turns are dropped — the Messages API requires the first message to be from
/// the user, and a recent-history window can start on an assistant reply.
pub fn build_chat_body(model: &str, system: &str, turns: &[(String, String)]) -> serde_json::Value {
    let first_user = turns.iter().position(|(role, _)| role == "user").unwrap_or(turns.len());
    let messages: Vec<serde_json::Value> = turns[first_user..]
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": messages
    })
}

/// Extract concatenated text from an Anthropic Messages API response body.
pub fn extract_text(resp: &serde_json::Value) -> Result<String, LlmError> {
    let content = resp.get("content").and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::Shape("no content array".into()))?;
    let mut out = String::new();
    for block in content {
        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) { out.push_str(t); }
        }
    }
    if out.is_empty() { return Err(LlmError::Shape("no text blocks".into())); }
    Ok(out)
}

pub struct ClaudeClient {
    api_key: String,
    model: String,
    client: reqwest::Client,
}

impl ClaudeClient {
    /// Reads ANTHROPIC_API_KEY and optional INGEST_MODEL from the environment.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| LlmError::NoKey)?;
        let model = std::env::var("INGEST_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".into());
        Ok(Self { api_key, model, client: reqwest::Client::new() })
    }

    /// Send a single user message (system + parts) and return the concatenated text output.
    pub async fn complete(&self, system: &str, parts: &[Part]) -> Result<String, LlmError> {
        let body = build_body(&self.model, system, parts);
        self.post(body).await
    }

    /// Send a multi-turn text conversation and return the text output.
    pub async fn complete_chat(
        &self,
        system: &str,
        turns: &[(String, String)],
    ) -> Result<String, LlmError> {
        let body = build_chat_body(&self.model, system, turns);
        self.post(body).await
    }

    async fn post(&self, body: serde_json::Value) -> Result<String, LlmError> {
        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send().await.map_err(|e| LlmError::Http(e.to_string()))?;
        let status = resp.status();
        let json: serde_json::Value = resp.json().await.map_err(|e| LlmError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(LlmError::Api { status: status.as_u16(), body: json.to_string() });
        }
        extract_text(&json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_body_has_model_and_image_block() {
        let body = build_body("claude-sonnet-4-6", "extract", &[Part::Text("hi".into()), Part::Image("image/png".into(), "AAAA".into())]);
        assert_eq!(body["model"], "claude-sonnet-4-6");
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "text");
        assert_eq!(blocks[1]["type"], "image");
        assert_eq!(blocks[1]["source"]["media_type"], "image/png");
    }

    #[test]
    fn pdf_part_becomes_document_block() {
        let body = build_body("m", "s", &[Part::Pdf("UEs=".into())]);
        let blocks = body["messages"][0]["content"].as_array().unwrap();
        assert_eq!(blocks[0]["type"], "document");
        assert_eq!(blocks[0]["source"]["media_type"], "application/pdf");
    }

    #[test]
    fn build_chat_body_renders_alternating_turns() {
        let turns = vec![
            ("user".to_string(), "q1".to_string()),
            ("assistant".to_string(), "a1".to_string()),
            ("user".to_string(), "q2".to_string()),
        ];
        let body = build_chat_body("claude-sonnet-4-6", "sys", &turns);
        assert_eq!(body["system"], "sys");
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[0]["content"], "q1");
        assert_eq!(messages[1]["role"], "assistant");
        assert_eq!(messages[2]["content"], "q2");
    }

    #[test]
    fn build_chat_body_drops_leading_assistant_turns() {
        // The Messages API requires the first message to be from the user; a
        // history window can start mid-conversation on an assistant reply.
        let turns = vec![
            ("assistant".to_string(), "a0".to_string()),
            ("user".to_string(), "q1".to_string()),
        ];
        let body = build_chat_body("m", "s", &turns);
        let messages = body["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["role"], "user");
    }

    #[test]
    fn extract_text_concatenates_text_blocks() {
        let resp = serde_json::json!({ "content": [ {"type":"text","text":"{\"a\":"}, {"type":"text","text":"1}"} ] });
        assert_eq!(extract_text(&resp).unwrap(), "{\"a\":1}");
    }

    #[test]
    fn extract_text_errors_without_text() {
        let resp = serde_json::json!({ "content": [] });
        assert!(matches!(extract_text(&resp), Err(LlmError::Shape(_))));
    }
}
