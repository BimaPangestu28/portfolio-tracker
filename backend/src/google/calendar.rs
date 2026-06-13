//! Thin Google Calendar API v3 client behind a trait so the sync reconciler can
//! be tested with a fake. Only the fields Phase 1 maps are modeled: summary,
//! location, description, start time, plus id/etag/updated and the ownership tag.

use async_trait::async_trait;

pub const APP_TAG: &str = "portfolio";

/// Fields we write to a Google event.
#[derive(Debug, Clone)]
pub struct EventWrite {
    pub summary: String,
    pub location: Option<String>,
    pub description: Option<String>,
    pub start_rfc3339_z: String,
}

/// A Google event as we read it.
#[derive(Debug, Clone)]
pub struct GCalEvent {
    pub id: String,
    pub etag: String,
    pub summary: Option<String>,
    pub location: Option<String>,
    pub description: Option<String>,
    pub start_rfc3339: Option<String>,
    pub updated: Option<String>,
    pub cancelled: bool,
    pub app_owned: bool,
}

/// Result of a list call: the page of events plus the next sync token (if final).
pub struct EventPage {
    pub events: Vec<GCalEvent>,
    pub next_sync_token: Option<String>,
}

/// Default duration for agenda events. The app's `events` table only stores a
/// start time, but Google rejects a timed event whose end is not after its
/// start, so a fixed 1-hour block is used.
const DEFAULT_EVENT_DURATION_HOURS: i64 = 1;

/// Derive the event end as start + default duration. Falls back to the start
/// value verbatim if it can't be parsed (Google then surfaces the error).
fn end_after(start_rfc3339: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(start_rfc3339) {
        Ok(dt) => (dt + chrono::Duration::hours(DEFAULT_EVENT_DURATION_HOURS))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        Err(_) => start_rfc3339.to_string(),
    }
}

pub fn to_request_body(w: &EventWrite) -> serde_json::Value {
    serde_json::json!({
        "summary": w.summary,
        "location": w.location,
        "description": w.description,
        "start": { "dateTime": w.start_rfc3339_z },
        "end": { "dateTime": end_after(&w.start_rfc3339_z) },
        "extendedProperties": { "private": { "app": APP_TAG } },
    })
}

/// Normalize a Google `start` object to a canonical UTC `...:SSZ` string so that
/// every stored `start_at` is directly comparable (lexical order == chronological)
/// and the web UI buckets it on the correct WIB day. Google returns timed events
/// as `dateTime` (often with a local offset like `+07:00`) and all-day events as
/// a bare `date` ("YYYY-MM-DD"); the latter is anchored to 00:00 WIB.
fn normalize_start(start: &serde_json::Value) -> Option<String> {
    if let Some(dt) = start.get("dateTime").and_then(|x| x.as_str()) {
        return Some(
            chrono::DateTime::parse_from_rfc3339(dt)
                .map(|d| d.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
                .unwrap_or_else(|_| dt.to_string()),
        );
    }
    if let Some(date) = start.get("date").and_then(|x| x.as_str()) {
        let normalized = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
            .and_then(|naive| naive.and_local_timezone(crate::assistant::time::wib()).single())
            .map(|dt| dt.with_timezone(&chrono::Utc).to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
            .unwrap_or_else(|| date.to_string());
        return Some(normalized);
    }
    None
}

pub fn parse_event(v: &serde_json::Value) -> anyhow::Result<GCalEvent> {
    let id = v
        .get("id")
        .and_then(|x| x.as_str())
        .ok_or_else(|| anyhow::anyhow!("event without id"))?
        .to_string();
    let status = v
        .get("status")
        .and_then(|x| x.as_str())
        .unwrap_or("confirmed");
    let start = v.get("start").and_then(normalize_start);
    let app_owned = v
        .get("extendedProperties")
        .and_then(|e| e.get("private"))
        .and_then(|p| p.get("app"))
        .and_then(|x| x.as_str())
        == Some(APP_TAG);
    Ok(GCalEvent {
        id,
        etag: v
            .get("etag")
            .and_then(|x| x.as_str())
            .unwrap_or_default()
            .to_string(),
        summary: v.get("summary").and_then(|x| x.as_str()).map(String::from),
        location: v.get("location").and_then(|x| x.as_str()).map(String::from),
        description: v
            .get("description")
            .and_then(|x| x.as_str())
            .map(String::from),
        start_rfc3339: start,
        updated: v.get("updated").and_then(|x| x.as_str()).map(String::from),
        cancelled: status == "cancelled",
        app_owned,
    })
}

/// Error type that distinguishes the cases the sync loop reacts to.
#[derive(Debug, thiserror::Error)]
pub enum CalendarError {
    #[error("sync token expired (410)")]
    SyncTokenGone,
    #[error("precondition failed (412)")]
    PreconditionFailed,
    #[error("rate limited or server error: {0}")]
    Transient(String),
    #[error("api error {status}: {body}")]
    Api { status: u16, body: String },
    #[error("http error: {0}")]
    Http(String),
}

/// The surface the reconciler depends on. Implemented by `HttpCalendar` in
/// production and a fake in tests.
#[async_trait]
pub trait CalendarApi {
    async fn insert(&self, w: &EventWrite) -> Result<GCalEvent, CalendarError>;
    async fn patch(
        &self,
        google_event_id: &str,
        etag: &str,
        w: &EventWrite,
    ) -> Result<GCalEvent, CalendarError>;
    async fn delete(&self, google_event_id: &str) -> Result<(), CalendarError>;
    async fn list(
        &self,
        sync_token: Option<&str>,
        time_min: &str,
        time_max: &str,
    ) -> Result<EventPage, CalendarError>;
}

/// Production HTTP client bound to the user's primary calendar.
pub struct HttpCalendar {
    access_token: String,
    client: reqwest::Client,
}

const BASE: &str = "https://www.googleapis.com/calendar/v3/calendars/primary/events";

impl HttpCalendar {
    pub fn new(access_token: String) -> Self {
        Self {
            access_token,
            client: reqwest::Client::new(),
        }
    }

    fn classify(status: reqwest::StatusCode, body: String) -> CalendarError {
        match status.as_u16() {
            410 => CalendarError::SyncTokenGone,
            412 => CalendarError::PreconditionFailed,
            429 | 500..=599 => CalendarError::Transient(format!("{status}: {body}")),
            other => CalendarError::Api { status: other, body },
        }
    }
}

#[async_trait]
impl CalendarApi for HttpCalendar {
    async fn insert(&self, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
        let resp = self
            .client
            .post(BASE)
            .bearer_auth(&self.access_token)
            .json(&to_request_body(w))
            .send()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, v.to_string()));
        }
        parse_event(&v).map_err(|e| CalendarError::Api {
            status: 200,
            body: e.to_string(),
        })
    }

    async fn patch(
        &self,
        google_event_id: &str,
        etag: &str,
        w: &EventWrite,
    ) -> Result<GCalEvent, CalendarError> {
        let resp = self
            .client
            .patch(format!("{BASE}/{google_event_id}"))
            .bearer_auth(&self.access_token)
            .header(reqwest::header::IF_MATCH, etag)
            .json(&to_request_body(w))
            .send()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, v.to_string()));
        }
        parse_event(&v).map_err(|e| CalendarError::Api {
            status: 200,
            body: e.to_string(),
        })
    }

    async fn delete(&self, google_event_id: &str) -> Result<(), CalendarError> {
        let resp = self
            .client
            .delete(format!("{BASE}/{google_event_id}"))
            .bearer_auth(&self.access_token)
            .send()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        if status.is_success() || status.as_u16() == 404 || status.as_u16() == 410 {
            return Ok(());
        }
        Err(Self::classify(
            status,
            resp.text().await.unwrap_or_default(),
        ))
    }

    async fn list(
        &self,
        sync_token: Option<&str>,
        time_min: &str,
        time_max: &str,
    ) -> Result<EventPage, CalendarError> {
        let mut req = self
            .client
            .get(BASE)
            .bearer_auth(&self.access_token)
            .query(&[
                ("singleEvents", "true"),
                ("showDeleted", "true"),
                ("maxResults", "250"),
            ]);
        req = match sync_token {
            Some(tok) => req.query(&[("syncToken", tok)]),
            None => req.query(&[("timeMin", time_min), ("timeMax", time_max)]),
        };
        let resp = req
            .send()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        let status = resp.status();
        let v: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| CalendarError::Http(e.to_string()))?;
        if !status.is_success() {
            return Err(Self::classify(status, v.to_string()));
        }
        let events = v
            .get("items")
            .and_then(|i| i.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| parse_event(e).ok())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let next_sync_token = v
            .get("nextSyncToken")
            .and_then(|x| x.as_str())
            .map(String::from);
        Ok(EventPage {
            events,
            next_sync_token,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_request_body_tags_app_owned_and_sets_start() {
        let body = to_request_body(&EventWrite {
            summary: "rapat".into(),
            location: Some("kantor".into()),
            description: None,
            start_rfc3339_z: "2026-06-13T07:00:00Z".into(),
        });
        assert_eq!(body["summary"], "rapat");
        assert_eq!(body["location"], "kantor");
        assert_eq!(body["start"]["dateTime"], "2026-06-13T07:00:00Z");
        // End is derived as start + 1h (Google rejects zero-duration events).
        assert_eq!(body["end"]["dateTime"], "2026-06-13T08:00:00Z");
        assert_eq!(body["extendedProperties"]["private"]["app"], "portfolio");
    }

    #[test]
    fn parse_event_reads_fields_and_app_tag() {
        let json = serde_json::json!({
            "id": "gid-1", "etag": "\"etag-1\"", "status": "confirmed",
            "summary": "rapat", "location": "kantor", "updated": "2026-06-12T09:00:00Z",
            "start": {"dateTime": "2026-06-13T07:00:00+07:00"},
            "extendedProperties": {"private": {"app": "portfolio"}}
        });
        let ev = parse_event(&json).unwrap();
        assert_eq!(ev.id, "gid-1");
        assert_eq!(ev.summary.as_deref(), Some("rapat"));
        assert!(ev.app_owned);
        assert!(!ev.cancelled);
    }

    #[test]
    fn parse_event_flags_cancelled_status() {
        let json = serde_json::json!({ "id": "gid-2", "etag": "e", "status": "cancelled" });
        let ev = parse_event(&json).unwrap();
        assert!(ev.cancelled);
        assert!(!ev.app_owned);
    }

    #[test]
    fn parse_event_normalizes_start_to_utc_z() {
        // Offset dateTime -> canonical Z (07:00 +07:00 == 00:00 UTC).
        let offset = serde_json::json!({
            "id": "g1", "etag": "e", "status": "confirmed",
            "start": { "dateTime": "2026-06-13T07:00:00+07:00" }
        });
        assert_eq!(parse_event(&offset).unwrap().start_rfc3339.as_deref(), Some("2026-06-13T00:00:00Z"));

        // Already-Z dateTime stays put.
        let zulu = serde_json::json!({
            "id": "g2", "etag": "e", "status": "confirmed",
            "start": { "dateTime": "2026-06-13T07:00:00Z" }
        });
        assert_eq!(parse_event(&zulu).unwrap().start_rfc3339.as_deref(), Some("2026-06-13T07:00:00Z"));

        // All-day `date` anchored to 00:00 WIB (== prior day 17:00 UTC).
        let allday = serde_json::json!({
            "id": "g3", "etag": "e", "status": "confirmed",
            "start": { "date": "2026-06-13" }
        });
        assert_eq!(parse_event(&allday).unwrap().start_rfc3339.as_deref(), Some("2026-06-12T17:00:00Z"));
    }
}
