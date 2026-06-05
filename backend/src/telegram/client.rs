//! Minimal Telegram Bot API client: long-poll getUpdates + sendMessage.
//! https://core.telegram.org/bots/api

use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum TgError {
    #[error("telegram rejected the bot token (401)")]
    Unauthorized,
    #[error("http error: {0}")]
    Http(String),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("unexpected response shape: {0}")]
    Shape(String),
}

#[derive(Debug, Deserialize)]
pub struct TgUpdate {
    pub update_id: i64,
    /// Absent for non-message updates (edits, joins, ...), which we ignore.
    pub message: Option<TgMessage>,
}

#[derive(Debug, Deserialize)]
pub struct TgMessage {
    pub chat: TgChat,
    pub from: Option<TgUser>,
    /// Absent for media-only messages, which we ignore.
    pub text: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TgChat {
    pub id: i64,
}

#[derive(Debug, Deserialize)]
pub struct TgUser {
    pub username: Option<String>,
}

/// Parse a getUpdates response body into updates.
pub fn parse_updates(body: &serde_json::Value) -> Result<Vec<TgUpdate>, TgError> {
    if body.get("ok").and_then(|v| v.as_bool()) != Some(true) {
        return Err(TgError::Shape(format!("ok != true: {body}")));
    }
    let result = body
        .get("result")
        .cloned()
        .ok_or_else(|| TgError::Shape("no result array".into()))?;
    serde_json::from_value(result).map_err(|e| TgError::Shape(e.to_string()))
}

pub struct TelegramClient {
    token: String,
    client: reqwest::Client,
}

/// Long-poll wait passed to getUpdates (seconds). The HTTP client timeout
/// must comfortably exceed this so the long poll is not cut short.
const POLL_TIMEOUT_SECS: u64 = 30;

impl TelegramClient {
    pub fn new(token: String) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(POLL_TIMEOUT_SECS + 20))
            .build()
            .expect("reqwest client");
        Self { token, client }
    }

    fn url(&self, method: &str) -> String {
        format!("https://api.telegram.org/bot{}/{}", self.token, method)
    }

    async fn check(resp: reqwest::Response) -> Result<serde_json::Value, TgError> {
        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(TgError::Unauthorized);
        }
        let body: serde_json::Value =
            resp.json().await.map_err(|e| TgError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(TgError::Api { status: status.as_u16(), body: body.to_string() });
        }
        Ok(body)
    }

    /// Long-poll for updates after `offset` (pass last update_id + 1).
    pub async fn get_updates(&self, offset: i64) -> Result<Vec<TgUpdate>, TgError> {
        let resp = self
            .client
            .get(self.url("getUpdates"))
            .query(&[("offset", offset.to_string()), ("timeout", POLL_TIMEOUT_SECS.to_string())])
            .send()
            .await
            .map_err(|e| TgError::Http(e.to_string()))?;
        let body = Self::check(resp).await?;
        parse_updates(&body)
    }

    /// Send a plain-text reply to a chat.
    pub async fn send_message(&self, chat_id: i64, text: &str) -> Result<(), TgError> {
        let resp = self
            .client
            .post(self.url("sendMessage"))
            .json(&serde_json::json!({ "chat_id": chat_id, "text": text }))
            .send()
            .await
            .map_err(|e| TgError::Http(e.to_string()))?;
        Self::check(resp).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_updates_extracts_text_messages() {
        let body = serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 42,
                "message": {
                    "message_id": 7,
                    "chat": { "id": 12345, "type": "private" },
                    "from": { "id": 12345, "is_bot": false, "username": "bima" },
                    "text": "halo"
                }
            }]
        });
        let updates = parse_updates(&body).unwrap();
        assert_eq!(updates.len(), 1);
        assert_eq!(updates[0].update_id, 42);
        let msg = updates[0].message.as_ref().unwrap();
        assert_eq!(msg.chat.id, 12345);
        assert_eq!(msg.from.as_ref().unwrap().username.as_deref(), Some("bima"));
        assert_eq!(msg.text.as_deref(), Some("halo"));
    }

    #[test]
    fn parse_updates_tolerates_non_message_updates() {
        let body = serde_json::json!({
            "ok": true,
            "result": [{ "update_id": 43, "my_chat_member": {} }]
        });
        let updates = parse_updates(&body).unwrap();
        assert_eq!(updates.len(), 1);
        assert!(updates[0].message.is_none());
    }

    #[test]
    fn parse_updates_rejects_not_ok() {
        let body = serde_json::json!({ "ok": false, "description": "bad" });
        assert!(matches!(parse_updates(&body), Err(TgError::Shape(_))));
    }
}
