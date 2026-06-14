# Gmail Integration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Read/summarize important Gmail from chat, surface important email in the morning briefing, and draft replies (Gmail drafts — never sent), reusing the existing Google OAuth/token infrastructure.

**Architecture:** Add Gmail scopes to OAuth; expose a `current_access_token` helper; a `GmailApi` trait + `HttpGmail` client mirroring `calendar.rs`; three agent tools; a briefing section. No migration.

**Tech Stack:** Rust, reqwest, serde_json, base64, async-trait. Tests: `cargo test <filter>` from `backend/` (BIN crate — never `cargo test --lib`, never `cargo fmt`).

---

## Conventions
- Paths relative to `backend/`. Run cargo from `backend/`. Commit from repo root.
- End commit bodies with: `Co-Authored-By: Claude Opus 4.8 (1M context) <noreply@anthropic.com>`
- Gmail API base: `https://gmail.googleapis.com/gmail/v1/users/me`. Auth: `.bearer_auth(token)`.

---

## Task 1: OAuth scopes + `current_access_token` helper

**Files:**
- Modify: `src/google/oauth.rs` (`SCOPE` + test)
- Modify: `src/google/engine.rs` (public `current_access_token`)

- [ ] **Step 1: Extend `SCOPE`**

In `src/google/oauth.rs`, change the `SCOPE` const to the three space-separated scopes:
```rust
pub const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events \
https://www.googleapis.com/auth/gmail.readonly \
https://www.googleapis.com/auth/gmail.compose";
```

- [ ] **Step 2: Update / add the consent-URL scope test**

There is an existing test asserting the consent URL contains the calendar scope (search `calendar.events` in `oauth.rs` tests). Extend it (or add a test) to also assert the Gmail scopes are present (URL-encoded `enc(SCOPE)` turns spaces into `%20` and `:`/`/` into `%3A`/`%2F`):
```rust
    #[test]
    fn consent_url_requests_all_scopes() {
        let url = consent_url("cid", "https://app/redir", "state123");
        assert!(url.contains("calendar.events"), "{url}");
        assert!(url.contains("gmail.readonly"), "{url}");
        assert!(url.contains("gmail.compose"), "{url}");
    }
```
(If an existing test hard-asserts the exact full encoded scope string and now fails, update it to match the new `SCOPE`.)

- [ ] **Step 3: Expose `current_access_token`**

In `src/google/engine.rs`, add a public wrapper that builds config + key like `run_cycle` and returns a fresh token:
```rust
/// A valid (refreshed-if-needed) Google access token for ad-hoc API calls
/// (e.g. the Gmail tools). Errors when Google isn't connected.
pub async fn current_access_token(db: &Db) -> anyhow::Result<String> {
    let cfg = OAuthConfig::from_env()?;
    let key = crate::google::crypto::key_from_env()?;
    ensure_access_token(db, &cfg, &key).await
}
```
(`ensure_access_token`, `OAuthConfig`, `Db` are already in scope in this file.)

- [ ] **Step 4: Build + test**

Run: `cargo test oauth::` and `cargo build`. Expected: PASS / clean (`current_access_token` is dead until Task 3 — warning OK).

- [ ] **Step 5: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/google/oauth.rs backend/src/google/engine.rs
git commit -m "feat(google): add Gmail scopes + current_access_token helper"
```

---

## Task 2: Gmail client (`src/google/gmail.rs`)

**Files:**
- Create: `src/google/gmail.rs`
- Modify: `src/google/mod.rs` (`pub mod gmail;`)
- Verify: `base64` is a dependency (it is — used in `telegram/mod.rs`).

- [ ] **Step 1: Register the module**

In `src/google/mod.rs`, add `pub mod gmail;` (alphabetical, after `pub mod engine;` → actually after `crypto`/before `oauth`; keep the list sorted: calendar, crypto, engine, gmail, oauth, sync).

- [ ] **Step 2: Write `gmail.rs` with pure helpers + tests first**

Create `src/google/gmail.rs`:
```rust
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
        // text/plain body "halo dunia" base64url-encoded (no pad).
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
```

- [ ] **Step 3: Build + test**

Run: `cargo test gmail::` and `cargo build`. Expected: 4 tests PASS; build clean (HTTP methods dead until Task 3 — warnings OK).

- [ ] **Step 4: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/google/gmail.rs backend/src/google/mod.rs
git commit -m "feat(google): Gmail client (list/get/draft) behind a trait seam"
```

---

## Task 3: Gmail agent tools

**Files:**
- Modify: `src/assistant/tools.rs` (3 schemas + registration test)
- Modify: `src/assistant/dispatcher.rs` (3 arms, 3 handlers, FakeGmail + tests)

- [ ] **Step 1: Write failing tests (with a `FakeGmail`)**

Add to `src/assistant/dispatcher.rs` tests:
```rust
    use crate::google::gmail::{EmailDetail, EmailSummary, GmailApi, GmailError};
    use std::sync::Mutex as StdMutex;

    #[derive(Default)]
    struct FakeGmail {
        messages: Vec<EmailSummary>,
        drafts: StdMutex<Vec<(String, String)>>, // (thread_id, body)
    }
    #[async_trait::async_trait]
    impl GmailApi for FakeGmail {
        async fn list_important_unread(&self, _max: u32) -> Result<Vec<EmailSummary>, GmailError> {
            Ok(self.messages.clone())
        }
        async fn get_message(&self, id: &str) -> Result<EmailDetail, GmailError> {
            let m = self.messages.iter().find(|m| m.id == id)
                .ok_or(GmailError::Api { status: 404, body: "not found".into() })?;
            Ok(EmailDetail { id: m.id.clone(), thread_id: m.thread_id.clone(), from: m.from.clone(),
                subject: m.subject.clone(), body: "isi email".into() })
        }
        async fn create_draft(&self, thread_id: &str, _to: &str, _subject: &str, body: &str)
            -> Result<String, GmailError> {
            self.drafts.lock().unwrap().push((thread_id.to_string(), body.to_string()));
            Ok("draft_1".into())
        }
    }

    fn email(id: &str, from: &str, subject: &str) -> EmailSummary {
        EmailSummary { id: id.into(), thread_id: format!("t_{id}"), from: from.into(),
            subject: subject.into(), snippet: "snippet".into() }
    }

    #[tokio::test]
    async fn list_emails_formats_or_empty() {
        let mut fake = FakeGmail::default();
        assert!(gmail_list_emails(&fake, &serde_json::json!({})).await.unwrap().to_lowercase().contains("nggak ada"));
        fake.messages = vec![email("m1", "Budi", "Meeting")];
        let out = gmail_list_emails(&fake, &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Budi") && out.contains("Meeting") && out.contains("m1"), "{out}");
    }

    #[tokio::test]
    async fn read_email_returns_body() {
        let fake = FakeGmail { messages: vec![email("m1", "Budi", "Meeting")], ..Default::default() };
        let out = gmail_read_email(&fake, &serde_json::json!({ "id": "m1" })).await.unwrap();
        assert!(out.contains("isi email"), "{out}");
    }

    #[tokio::test]
    async fn draft_reply_creates_draft() {
        let fake = FakeGmail { messages: vec![email("m1", "Budi", "Meeting")], ..Default::default() };
        let out = gmail_draft_reply(&fake, &serde_json::json!({ "id": "m1", "body": "ok meeting jam 3" })).await.unwrap();
        assert!(out.to_lowercase().contains("draft"), "{out}");
        assert_eq!(fake.drafts.lock().unwrap()[0], ("t_m1".to_string(), "ok meeting jam 3".to_string()));
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test list_emails_formats_or_empty`
Expected: FAIL (`gmail_list_emails` not defined).

- [ ] **Step 3: Add handlers**

Add to `src/assistant/dispatcher.rs` (near the other integration handlers):
```rust
async fn gmail_list_emails(api: &dyn crate::google::gmail::GmailApi, input: &serde_json::Value) -> Result<String, String> {
    let max = input.get("max").and_then(|v| v.as_u64()).unwrap_or(10) as u32;
    let emails = api.list_important_unread(max).await.map_err(|e| format!("{e}"))?;
    if emails.is_empty() {
        return Ok("nggak ada email penting yang belum dibaca".into());
    }
    let mut out = String::new();
    for e in emails {
        out.push_str(&format!("[{}] {} — {} — {}\n", e.id, e.from, e.subject, e.snippet));
    }
    Ok(out)
}

async fn gmail_read_email(api: &dyn crate::google::gmail::GmailApi, input: &serde_json::Value) -> Result<String, String> {
    let id = str_arg(input, "id").ok_or("missing required argument 'id'")?;
    let m = api.get_message(id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("Dari: {}\nSubjek: {}\n\n{}", m.from, m.subject, m.body))
}

async fn gmail_draft_reply(api: &dyn crate::google::gmail::GmailApi, input: &serde_json::Value) -> Result<String, String> {
    let id = str_arg(input, "id").ok_or("missing required argument 'id'")?;
    let body = str_arg(input, "body").ok_or("missing required argument 'body'")?;
    let m = api.get_message(id).await.map_err(|e| format!("{e}"))?;
    api.create_draft(&m.thread_id, &m.from, &m.subject, body).await.map_err(|e| format!("{e}"))?;
    Ok(format!("draft balasan ke {} disimpan di Gmail — cek & kirim dari sana", m.from))
}
```

- [ ] **Step 4: Add dispatch arms (gated on a live token)**

In the `match name` block, add:
```rust
        "list_emails" => match crate::google::engine::current_access_token(db).await {
            Ok(token) => gmail_list_emails(&crate::google::gmail::HttpGmail::new(token), input).await,
            Err(_) => Err("Gmail belum tersambung — sambungin Google dulu di web UI".into()),
        },
        "read_email" => match crate::google::engine::current_access_token(db).await {
            Ok(token) => gmail_read_email(&crate::google::gmail::HttpGmail::new(token), input).await,
            Err(_) => Err("Gmail belum tersambung — sambungin Google dulu di web UI".into()),
        },
        "draft_reply" => match crate::google::engine::current_access_token(db).await {
            Ok(token) => gmail_draft_reply(&crate::google::gmail::HttpGmail::new(token), input).await,
            Err(_) => Err("Gmail belum tersambung — sambungin Google dulu di web UI".into()),
        },
```

- [ ] **Step 5: Register the three tool schemas**

In `src/assistant/tools.rs`, append at the END of `definitions()` (comma after current last object):
```rust
        {
            "name": "list_emails",
            "description": "List important unread Gmail (sender, subject, snippet, id). Use for 'ada email penting?'.",
            "input_schema": { "type": "object", "properties": { "max": { "type": "integer", "description": "Max emails, default 10" } } }
        },
        {
            "name": "read_email",
            "description": "Read the full body of one email by id (from list_emails) to summarize it or to draft a reply.",
            "input_schema": { "type": "object", "properties": { "id": { "type": "string", "description": "Gmail message id" } }, "required": ["id"] }
        },
        {
            "name": "draft_reply",
            "description": "Create a Gmail DRAFT reply to an email (by id). You compose the body; the draft is saved in Gmail for the owner to review and send — it is NOT sent automatically.",
            "input_schema": { "type": "object", "properties": { "id": { "type": "string", "description": "Gmail message id to reply to" }, "body": { "type": "string", "description": "Reply text you composed" } }, "required": ["id", "body"] }
        }
```
Update `defines_all_tools_with_schemas` expected vector: append `"list_emails", "read_email", "draft_reply"` after the actual current last name (read the file to find it).

- [ ] **Step 6: Run tests**

Run: `cargo test gmail` (handlers) and `cargo test tools::tests`. Expected PASS. `cargo build` clean.

- [ ] **Step 7: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/tools.rs backend/src/assistant/dispatcher.rs
git commit -m "feat(assistant): Gmail list/read/draft-reply tools"
```

---

## Task 4: Important email in the morning briefing

**Files:**
- Modify: `src/assistant/proactive/briefing.rs` (`BriefingData` + gather + render + tests)

- [ ] **Step 1: Add the field + gather + render**

In `BriefingData` add:
```rust
    /// Important unread emails. `None` when Gmail isn't reachable (section omitted).
    pub gmail_important: Option<Vec<crate::google::gmail::EmailSummary>>,
```
In `gather`, after the clickup section, add a best-effort fetch:
```rust
    let gmail_important = match crate::google::engine::current_access_token(db).await {
        Ok(token) => {
            let gmail = crate::google::gmail::HttpGmail::new(token);
            match gmail.list_important_unread(5).await {
                Ok(list) => Some(list),
                Err(e) => {
                    tracing::warn!("briefing: gmail unavailable: {e}");
                    None
                }
            }
        }
        Err(_) => None, // not connected → section omitted
    };
```
Add `gmail_important` to the `BriefingData { ... }` constructor.

In `render_data_block`, after the clickup section, add:
```rust
    if let Some(emails) = &d.gmail_important {
        out.push_str("Email penting:\n");
        if emails.is_empty() {
            out.push_str("(tidak ada)\n");
        } else {
            for e in emails {
                out.push_str(&format!("- {} — {}\n", e.from, e.subject));
            }
        }
    }
```

- [ ] **Step 2: Fix the test `data()` helper + add a render test**

The `data()` test helper builds `BriefingData { ... }` literally — add `gmail_important: None,`. Add a render test:
```rust
    #[test]
    fn gmail_section_renders_when_present_and_omitted_when_none() {
        let mut d = data();
        assert!(!render_data_block(&d).contains("Email penting"));
        d.gmail_important = Some(vec![crate::google::gmail::EmailSummary {
            id: "m1".into(), thread_id: "t1".into(), from: "Budi".into(),
            subject: "Invoice".into(), snippet: "..".into(),
        }]);
        let block = render_data_block(&d);
        assert!(block.contains("Email penting:"), "{block}");
        assert!(block.contains("Budi — Invoice"), "{block}");
    }
```

- [ ] **Step 3: Build + test**

Run: `cargo test briefing::tests` and `cargo build`. Expected PASS / clean.

- [ ] **Step 4: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/proactive/briefing.rs
git commit -m "feat(proactive): important email section in the morning briefing"
```

---

## Task 5: Prompt guidance

**Files:**
- Modify: `src/assistant/agent.rs` (`SYSTEM`)

- [ ] **Step 1: Append guidance**

Append to the END of the `SYSTEM` string (before the closing `";`, continuing the `\`-joined style; ensure the prior fragment ends with ` \`):
```rust
 You can also handle Gmail: 'ada email penting?' → list_emails; to read or summarize \
a specific one, call read_email with its id; to reply, call read_email for context then draft_reply \
with the id and the reply text you compose — the draft is saved to Gmail for the owner to review and \
send, it is NOT sent automatically, so tell them to check Gmail. If a Gmail tool says it's not \
connected, tell the owner to connect Google in the web UI.
```

- [ ] **Step 2: Build + test**

Run: `cargo build` (clean) and `cargo test agent`.

- [ ] **Step 3: Commit**

```bash
cd /Users/bimapangestu/Desktop/Works/personal/portfolio-tracker
git add backend/src/assistant/agent.rs
git commit -m "feat(assistant): Gmail prompt guidance"
```

---

## Final verification
- [ ] `cargo test` (all pass) and `cargo build` (no new warnings).

## Spec coverage check
- Read/summarize important mail → Task 2 (client), Task 3 (`list_emails`/`read_email`).
- Draft replies (compose, never send) → Task 2 (`create_draft`), Task 3 (`draft_reply`).
- Important email in briefing → Task 4.
- Gmail scopes + re-consent + token reuse → Task 1; 403→ScopeMissing→re-consent hint → Task 2/3.
- Degradation when not connected → Task 3 (gating), Task 4 (omit section).
- No migration; "important" = Gmail markers; drafts direct → throughout.
- Prompt guidance → Task 5.
