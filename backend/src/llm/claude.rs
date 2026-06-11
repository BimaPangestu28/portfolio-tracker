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

/// Build a Messages API body with tool definitions. `messages` are raw
/// message values — the agent loop threads tool_use/tool_result blocks
/// through verbatim.
pub fn build_tools_body(
    model: &str,
    system: &str,
    messages: &[serde_json::Value],
    tools: &serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "model": model,
        "max_tokens": 4096,
        "system": system,
        "messages": messages,
        "tools": tools,
    })
}

/// One content block of an API response, as the agent loop consumes it.
#[derive(Debug, PartialEq)]
pub enum ResponseBlock {
    Text(String),
    ToolUse { id: String, name: String, input: serde_json::Value },
}

/// Split a Messages API response into text and tool_use blocks.
pub fn extract_blocks(resp: &serde_json::Value) -> Result<Vec<ResponseBlock>, LlmError> {
    let content = resp
        .get("content")
        .and_then(|c| c.as_array())
        .ok_or_else(|| LlmError::Shape("no content array".into()))?;
    let mut out = Vec::new();
    for block in content {
        match block.get("type").and_then(|t| t.as_str()) {
            Some("text") => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    out.push(ResponseBlock::Text(t.to_string()));
                }
            }
            Some("tool_use") => {
                let id = block
                    .get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LlmError::Shape("tool_use without id".into()))?
                    .to_string();
                let name = block
                    .get("name")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| LlmError::Shape("tool_use without name".into()))?
                    .to_string();
                let input = block.get("input").cloned().unwrap_or(serde_json::json!({}));
                out.push(ResponseBlock::ToolUse { id, name, input });
            }
            _ => {}
        }
    }
    if out.is_empty() {
        return Err(LlmError::Shape("no usable blocks".into()));
    }
    Ok(out)
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

/// Per-request deadline. Without one, a hung connection blocks every caller
/// that awaits the LLM — including the Telegram poller running the agent loop.
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

impl ClaudeClient {
    /// Reads ANTHROPIC_API_KEY and optional INGEST_MODEL from the environment.
    pub fn from_env() -> Result<Self, LlmError> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| LlmError::NoKey)?;
        let model = std::env::var("INGEST_MODEL").unwrap_or_else(|_| "claude-sonnet-4-6".into());
        let client = reqwest::Client::builder()
            .timeout(REQUEST_TIMEOUT)
            .build()
            .map_err(|e| LlmError::Http(e.to_string()))?;
        Ok(Self { api_key, model, client })
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

    /// Send a tool-enabled conversation; returns the FULL response JSON so
    /// the agent loop can replay assistant content verbatim.
    pub async fn complete_tools(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value, LlmError> {
        let body = build_tools_body(&self.model, system, messages, tools);
        self.post_json(body).await
    }

    async fn post(&self, body: serde_json::Value) -> Result<String, LlmError> {
        let json = self.post_json(body).await?;
        extract_text(&json)
    }

    /// POST to the Messages API and return the raw success-response JSON.
    async fn post_json(&self, body: serde_json::Value) -> Result<serde_json::Value, LlmError> {
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
        Ok(json)
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

    #[test]
    fn build_tools_body_includes_tools_and_messages_verbatim() {
        let messages = vec![serde_json::json!({ "role": "user", "content": "hi" })];
        let tools = serde_json::json!([{ "name": "create_todo" }]);
        let body = build_tools_body("claude-sonnet-4-6", "sys", &messages, &tools);
        assert_eq!(body["model"], "claude-sonnet-4-6");
        assert_eq!(body["system"], "sys");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["tools"][0]["name"], "create_todo");
    }

    #[test]
    fn extract_blocks_splits_text_and_tool_use() {
        let resp = serde_json::json!({ "content": [
            { "type": "text", "text": "checking" },
            { "type": "tool_use", "id": "tu_1", "name": "create_todo",
              "input": { "title": "bayar listrik" } }
        ]});
        let blocks = extract_blocks(&resp).unwrap();
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0], ResponseBlock::Text("checking".into()));
        let ResponseBlock::ToolUse { id, name, input } = &blocks[1] else {
            panic!("expected tool_use")
        };
        assert_eq!(id, "tu_1");
        assert_eq!(name, "create_todo");
        assert_eq!(input["title"], "bayar listrik");
    }

    #[test]
    fn extract_blocks_rejects_empty_content() {
        let resp = serde_json::json!({ "content": [] });
        assert!(matches!(extract_blocks(&resp), Err(LlmError::Shape(_))));
    }

    #[test]
    fn extract_blocks_rejects_tool_use_without_name() {
        let resp = serde_json::json!({ "content": [
            { "type": "tool_use", "id": "tu_1", "input": {} }
        ]});
        assert!(matches!(extract_blocks(&resp), Err(LlmError::Shape(_))));
    }
}
