//! Gmail client behind a trait seam (mirrors `calendar.rs`): read important
//! mail, fetch a message, and create reply drafts. The owner sends drafts in
//! Gmail — this client never sends.

use async_trait::async_trait;
use base64::Engine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailSummary {
    pub id: String,
    pub thread_id: String,
    pub from: String,
    pub subject: String,
    pub snippet: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmailDetail {
    pub id: String,
    pub thread_id: String,
    pub from: String,
    pub subject: String,
    pub body: String,
}

#[derive(Debug)]
pub enum GmailError {
    Http(String),
    ScopeMissing,
    Api { status: u16, body: String },
}

impl std::fmt::Display for GmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GmailError::Http(e) => write!(f, "gangguan jaringan Gmail: {e}"),
            GmailError::ScopeMissing => write!(f, "akses Gmail belum diizinkan (sambungin ulang Google)"),
            GmailError::Api { status, body } => write!(f, "Gmail error {status}: {body}"),
        }
    }
}
impl std::error::Error for GmailError {}

/// Value of a named header from a Gmail message `payload.headers` array.
fn header<'a>(v: &'a serde_json::Value, name: &str) -> &'a str {
    v["payload"]["headers"]
        .as_array()
        .and_then(|hs| hs.iter().find(|h| h["name"].as_str().is_some_and(|n| n.eq_ignore_ascii_case(name))))
        .and_then(|h| h["value"].as_str())
        .unwrap_or("")
}

/// Decode a Gmail base64url body segment (padding-tolerant).
fn decode_b64url(s: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s.trim_end_matches('='))
        .ok()
}

/// Walk a message payload for the first text/plain body; fall back to the
/// top-level body, then the snippet.
fn extract_body(msg: &serde_json::Value) -> String {
    fn find_plain(part: &serde_json::Value) -> Option<String> {
        if part["mimeType"].as_str() == Some("text/plain") {
            if let Some(data) = part["body"]["data"].as_str() {
                if let Some(bytes) = decode_b64url(data) {
                    return Some(String::from_utf8_lossy(&bytes).to_string());
                }
            }
        }
        if let Some(parts) = part["parts"].as_array() {
            for p in parts {
                if let Some(found) = find_plain(p) {
                    return Some(found);
                }
            }
        }
        None
    }
    if let Some(text) = find_plain(&msg["payload"]) {
        return text;
    }
    if let Some(data) = msg["payload"]["body"]["data"].as_str() {
        if let Some(bytes) = decode_b64url(data) {
            return String::from_utf8_lossy(&bytes).to_string();
        }
    }
    msg["snippet"].as_str().unwrap_or("").to_string()
}

pub fn parse_summary(msg: &serde_json::Value) -> EmailSummary {
    EmailSummary {
        id: msg["id"].as_str().unwrap_or_default().to_string(),
        thread_id: msg["threadId"].as_str().unwrap_or_default().to_string(),
        from: header(msg, "From").to_string(),
        subject: header(msg, "Subject").to_string(),
        snippet: msg["snippet"].as_str().unwrap_or("").to_string(),
    }
}

pub fn parse_detail(msg: &serde_json::Value) -> EmailDetail {
    EmailDetail {
        id: msg["id"].as_str().unwrap_or_default().to_string(),
        thread_id: msg["threadId"].as_str().unwrap_or_default().to_string(),
        from: header(msg, "From").to_string(),
        subject: header(msg, "Subject").to_string(),
        body: extract_body(msg),
    }
}

/// RFC822 message → base64url (no padding) for the Gmail draft `raw` field.
pub fn build_raw_message(to: &str, subject: &str, body: &str) -> String {
    let raw = format!("To: {to}\r\nSubject: {subject}\r\n\r\n{body}");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

/// `Re: x` unless already prefixed.
pub fn reply_subject(subject: &str) -> String {
    if subject.to_lowercase().starts_with("re:") {
        subject.to_string()
    } else {
        format!("Re: {subject}")
    }
}

#[async_trait]
pub trait GmailApi {
    async fn list_important_unread(&self, max: u32) -> Result<Vec<EmailSummary>, GmailError>;
    async fn get_message(&self, id: &str) -> Result<EmailDetail, GmailError>;
    async fn create_draft(&self, thread_id: &str, to: &str, subject: &str, body: &str)
        -> Result<String, GmailError>;
}

pub struct HttpGmail {
    access_token: String,
    client: reqwest::Client,
}

const BASE: &str = "https://gmail.googleapis.com/gmail/v1/users/me";

impl HttpGmail {
    pub fn new(access_token: String) -> Self {
        Self { access_token, client: reqwest::Client::new() }
    }

    fn classify(status: reqwest::StatusCode, body: String) -> GmailError {
        match status.as_u16() {
            403 => GmailError::ScopeMissing,
            other => GmailError::Api { status: other, body },
        }
    }

    async fn get_json(&self, url: &str) -> Result<serde_json::Value, GmailError> {
        let resp = self.client.get(url).bearer_auth(&self.access_token)
            .send().await.map_err(|e| GmailError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| GmailError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, v.to_string()));
        }
        Ok(v)
    }
}

#[async_trait]
impl GmailApi for HttpGmail {
    async fn list_important_unread(&self, max: u32) -> Result<Vec<EmailSummary>, GmailError> {
        let list = self.get_json(&format!(
            "{BASE}/messages?q=is:unread%20is:important&maxResults={max}"
        )).await?;
        let ids: Vec<String> = list["messages"].as_array().map(|arr| {
            arr.iter().filter_map(|m| m["id"].as_str().map(|s| s.to_string())).collect()
        }).unwrap_or_default();
        let mut out = Vec::new();
        for id in ids {
            let msg = self.get_json(&format!(
                "{BASE}/messages/{id}?format=metadata&metadataHeaders=From&metadataHeaders=Subject"
            )).await?;
            out.push(parse_summary(&msg));
        }
        Ok(out)
    }

    async fn get_message(&self, id: &str) -> Result<EmailDetail, GmailError> {
        let msg = self.get_json(&format!("{BASE}/messages/{id}?format=full")).await?;
        Ok(parse_detail(&msg))
    }

    async fn create_draft(&self, thread_id: &str, to: &str, subject: &str, body: &str)
        -> Result<String, GmailError> {
        let raw = build_raw_message(to, &reply_subject(subject), body);
        let payload = serde_json::json!({ "message": { "raw": raw, "threadId": thread_id } });
        let resp = self.client.post(&format!("{BASE}/drafts")).bearer_auth(&self.access_token)
            .json(&payload).send().await.map_err(|e| GmailError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp.json().await.map_err(|e| GmailError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, v.to_string()));
        }
        Ok(v["id"].as_str().unwrap_or_default().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_full() -> serde_json::Value {
        let data = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b"halo dunia");
        serde_json::json!({
            "id": "m1", "threadId": "t1", "snippet": "halo...",
            "payload": {
                "headers": [
                    { "name": "From", "value": "Budi <budi@x.com>" },
                    { "name": "Subject", "value": "Meeting" }
                ],
                "parts": [ { "mimeType": "text/plain", "body": { "data": data } } ]
            }
        })
    }

    #[test]
    fn parse_summary_pulls_headers_and_snippet() {
        let s = parse_summary(&sample_full());
        assert_eq!(s.id, "m1");
        assert_eq!(s.thread_id, "t1");
        assert_eq!(s.from, "Budi <budi@x.com>");
        assert_eq!(s.subject, "Meeting");
    }

    #[test]
    fn parse_detail_decodes_plain_body() {
        let d = parse_detail(&sample_full());
        assert_eq!(d.body, "halo dunia");
        assert_eq!(d.subject, "Meeting");
    }

    #[test]
    fn reply_subject_prefixes_once() {
        assert_eq!(reply_subject("Meeting"), "Re: Meeting");
        assert_eq!(reply_subject("Re: Meeting"), "Re: Meeting");
    }

    #[test]
    fn build_raw_message_round_trips() {
        let raw = build_raw_message("a@b.com", "Re: Hi", "isi balasan");
        let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(raw).unwrap();
        let text = String::from_utf8(decoded).unwrap();
        assert!(text.contains("To: a@b.com"), "{text}");
        assert!(text.contains("Subject: Re: Hi"), "{text}");
        assert!(text.ends_with("isi balasan"), "{text}");
    }
}
