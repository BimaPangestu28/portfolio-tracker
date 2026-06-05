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
    /// Absent for media-only messages.
    pub text: Option<String>,
    /// Photo messages: one entry per resolution, smallest first.
    pub photo: Option<Vec<TgPhotoSize>>,
    /// File attachments (PDF statements, forwarded images, ...).
    pub document: Option<TgDocument>,
}

#[derive(Debug, Deserialize)]
pub struct TgPhotoSize {
    pub file_id: String,
    pub width: i64,
    pub height: i64,
}

#[derive(Debug, Deserialize)]
pub struct TgDocument {
    pub file_id: String,
    pub file_name: Option<String>,
    pub mime_type: Option<String>,
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

/// Extract the downloadable path from a getFile response body.
pub fn parse_file_path(body: &serde_json::Value) -> Result<String, TgError> {
    body.get("result")
        .and_then(|r| r.get("file_path"))
        .and_then(|p| p.as_str())
        .map(String::from)
        .ok_or_else(|| TgError::Shape(format!("no file_path: {body}")))
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

    /// Resolve a file_id to its downloadable path (getFile).
    pub async fn get_file_path(&self, file_id: &str) -> Result<String, TgError> {
        let resp = self
            .client
            .get(self.url("getFile"))
            .query(&[("file_id", file_id)])
            .send()
            .await
            .map_err(|e| TgError::Http(e.to_string()))?;
        let body = Self::check(resp).await?;
        parse_file_path(&body)
    }

    /// Download a file by the path returned from getFile. Bot API caps
    /// downloads at 20 MB, well above any statement photo or PDF.
    pub async fn download_file(&self, file_path: &str) -> Result<Vec<u8>, TgError> {
        let url = format!("https://api.telegram.org/file/bot{}/{}", self.token, file_path);
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| TgError::Http(e.to_string()))?;
        let status = resp.status();
        if status.as_u16() == 401 {
            return Err(TgError::Unauthorized);
        }
        if !status.is_success() {
            return Err(TgError::Api { status: status.as_u16(), body: "file download failed".into() });
        }
        let bytes = resp.bytes().await.map_err(|e| TgError::Http(e.to_string()))?;
        Ok(bytes.to_vec())
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
    fn parse_updates_extracts_photo_messages() {
        let body = serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 44,
                "message": {
                    "message_id": 8,
                    "chat": { "id": 12345, "type": "private" },
                    "caption": "Tolong tambahin ini",
                    "photo": [
                        { "file_id": "small", "width": 90, "height": 160 },
                        { "file_id": "big", "width": 720, "height": 1280 }
                    ]
                }
            }]
        });
        let updates = parse_updates(&body).unwrap();
        let msg = updates[0].message.as_ref().unwrap();
        assert!(msg.text.is_none());
        let photos = msg.photo.as_ref().unwrap();
        assert_eq!(photos.len(), 2);
        assert_eq!(photos[1].file_id, "big");
        assert_eq!(photos[1].width, 720);
    }

    #[test]
    fn parse_updates_extracts_document_messages() {
        let body = serde_json::json!({
            "ok": true,
            "result": [{
                "update_id": 45,
                "message": {
                    "message_id": 9,
                    "chat": { "id": 12345, "type": "private" },
                    "document": {
                        "file_id": "doc1",
                        "file_name": "statement.pdf",
                        "mime_type": "application/pdf"
                    }
                }
            }]
        });
        let updates = parse_updates(&body).unwrap();
        let doc = updates[0].message.as_ref().unwrap().document.as_ref().unwrap();
        assert_eq!(doc.file_id, "doc1");
        assert_eq!(doc.file_name.as_deref(), Some("statement.pdf"));
        assert_eq!(doc.mime_type.as_deref(), Some("application/pdf"));
    }

    #[test]
    fn parse_file_path_extracts_the_download_path() {
        let body = serde_json::json!({
            "ok": true,
            "result": { "file_id": "big", "file_size": 1234, "file_path": "photos/file_1.jpg" }
        });
        assert_eq!(parse_file_path(&body).unwrap(), "photos/file_1.jpg");
    }

    #[test]
    fn parse_file_path_rejects_missing_path() {
        let body = serde_json::json!({ "ok": true, "result": {} });
        assert!(matches!(parse_file_path(&body), Err(TgError::Shape(_))));
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
