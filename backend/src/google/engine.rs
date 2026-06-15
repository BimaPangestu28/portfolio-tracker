//! Orchestrates one sync pass: execute outbound ops, then apply inbound ops,
//! persisting Google ids/etags and the sync token. Network access goes through
//! the `CalendarApi` trait so this is testable with a fake.

use crate::db::Db;
use crate::google::calendar::{CalendarApi, CalendarError};
use crate::google::sync::{plan_inbound_one, plan_outbound, InboundOp, OutboundOp};
use crate::repo::events;

/// Window for the initial (token-less) inbound list: now .. now + 30 days.
const INBOUND_WINDOW_DAYS: i64 = 30;

/// What one sync pass changed — surfaced to the manual-sync API and logs.
#[derive(Debug, Default, Clone, Copy, serde::Serialize)]
pub struct SyncSummary {
    /// app events pushed to Google (created/patched/deleted).
    pub pushed: usize,
    /// Google events applied locally (imported/updated/removed).
    pub imported: usize,
}

/// Execute one outbound+inbound pass with a given client. Pure DB + the trait;
/// no env reads, so tests inject a fake. Returns a count of what changed.
pub async fn run_pass<C: CalendarApi>(db: &Db, cal: &C) -> anyhow::Result<SyncSummary> {
    let mut pushed = 0usize;
    let mut imported = 0usize;

    // --- Outbound ---
    for op in plan_outbound(&events::pending_push(db).await?) {
        match op {
            OutboundOp::Create { event_id, write } => match cal.insert(&write).await {
                Ok(ev) => { events::mark_synced(db, event_id, &ev.id, &ev.etag).await?; pushed += 1; }
                Err(e) => tracing::warn!("google insert {event_id} skipped: {e}"),
            },
            OutboundOp::Patch { event_id, google_event_id, etag, write } => {
                match cal.patch(&google_event_id, &etag, &write).await {
                    Ok(ev) => { events::mark_synced(db, event_id, &ev.id, &ev.etag).await?; pushed += 1; }
                    Err(CalendarError::PreconditionFailed) => {
                        tracing::info!("google patch {event_id} lost race; inbound will reconcile");
                    }
                    Err(e) => tracing::warn!("google patch {event_id} skipped: {e}"),
                }
            }
            OutboundOp::Delete { event_id, google_event_id } => match cal.delete(&google_event_id).await {
                Ok(()) => { events::mark_synced(db, event_id, &google_event_id, "").await?; pushed += 1; }
                Err(e) => tracing::warn!("google delete {event_id} skipped: {e}"),
            },
        }
    }

    // --- Inbound ---
    let sync_token = crate::repo::google_integration::get(db).await?
        .and_then(|r| r.calendar_sync_token);
    let now = chrono::Utc::now();
    let time_min = now.to_rfc3339();
    let time_max = (now + chrono::Duration::days(INBOUND_WINDOW_DAYS)).to_rfc3339();

    let page = match cal.list(sync_token.as_deref(), &time_min, &time_max).await {
        Ok(page) => page,
        Err(CalendarError::SyncTokenGone) => {
            crate::repo::google_integration::clear_sync_token(db).await?;
            tracing::info!("google sync token expired; will full-resync next pass");
            return Ok(SyncSummary { pushed, imported });
        }
        Err(e) => { tracing::warn!("google list skipped: {e}"); return Ok(SyncSummary { pushed, imported }); }
    };

    for r in &page.events {
        let existing = events::get_by_google_id(db, &r.id).await?;
        match plan_inbound_one(r, existing.as_ref()) {
            Some(InboundOp::UpsertForeign { google_event_id, etag, summary, location, notes, start_at }) => {
                events::upsert_foreign(db, &google_event_id, &summary, location.as_deref(), notes.as_deref(), &start_at, &etag).await?;
                imported += 1;
            }
            Some(InboundOp::RemoveForeign { event_id }) => { events::cancel_by_sync(db, event_id).await?; imported += 1; }
            Some(InboundOp::UpdateLocal { event_id, etag, summary, location, notes, start_at }) => {
                events::update_from_google(db, event_id, &summary, location.as_deref(), notes.as_deref(), &start_at, &etag).await?;
                imported += 1;
            }
            Some(InboundOp::CancelLocal { event_id }) => { events::cancel_by_sync(db, event_id).await?; imported += 1; }
            None => {}
        }
    }

    if let Some(tok) = page.next_sync_token {
        crate::repo::google_integration::set_sync_token(db, &tok).await?;
    }
    Ok(SyncSummary { pushed, imported })
}

use crate::google::oauth::{self, OAuthConfig};

/// Ensure a non-expired access token, refreshing if needed. Returns the
/// plaintext access token, or Err with a reason to record as last_error.
async fn ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8; 32]) -> anyhow::Result<String> {
    let row = crate::repo::google_integration::get(db).await?
        .ok_or_else(|| anyhow::anyhow!("not connected"))?;
    let expired = chrono::DateTime::parse_from_rfc3339(&row.expiry)
        .map(|exp| chrono::Utc::now() >= exp.with_timezone(&chrono::Utc))
        .unwrap_or(true);
    if !expired {
        return crate::google::crypto::decrypt(&row.access_token, key);
    }
    let refresh = crate::google::crypto::decrypt(&row.refresh_token, key)?;
    let tokens = oauth::refresh_access(cfg, &refresh).await?;
    let enc = crate::google::crypto::encrypt(&tokens.access_token, key)?;
    let expiry = oauth::expiry_from_now(tokens.expires_in);
    crate::repo::google_integration::update_access(db, &enc, &expiry).await?;
    Ok(tokens.access_token)
}

/// A valid (refreshed-if-needed) Google access token for ad-hoc API calls
/// (e.g. the Gmail tools). Errors when Google isn't connected.
pub async fn current_access_token(db: &Db) -> anyhow::Result<String> {
    let cfg = OAuthConfig::from_env()?;
    let key = crate::google::crypto::key_from_env()?;
    ensure_access_token(db, &cfg, &key).await
}

/// One full cycle including auth. Sets integration status + last_synced_at, and
/// returns what changed. A not-connected/disconnected/errored account yields an
/// empty summary (the caller reads the row's status separately).
pub async fn run_cycle(db: &Db) -> anyhow::Result<SyncSummary> {
    let Some(row) = crate::repo::google_integration::get(db).await? else { return Ok(SyncSummary::default()) };
    if row.status == "disconnected" { return Ok(SyncSummary::default()); }
    let cfg = OAuthConfig::from_env()?;
    let key = crate::google::crypto::key_from_env()?;
    let token = match ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("google sync: auth failed: {e}");
            crate::repo::google_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(SyncSummary::default());
        }
    };
    if row.status == "error" {
        crate::repo::google_integration::set_status(db, "connected", None).await?;
    }
    let cal = crate::google::calendar::HttpCalendar::new(token);
    let summary = run_pass(db, &cal).await?;
    crate::repo::google_integration::set_synced_at(db, &chrono::Utc::now().to_rfc3339()).await?;
    Ok(summary)
}

const TICK: std::time::Duration = std::time::Duration::from_secs(300);

/// Spawn the independent 5-minute Google sync loop (no Telegram dependency).
/// No-op when OAuth env is unconfigured.
pub fn spawn(db: Db) {
    if OAuthConfig::from_env().is_err() {
        tracing::info!("GOOGLE_CLIENT_* not set; calendar sync disabled");
        return;
    }
    tokio::spawn(async move {
        loop {
            match run_cycle(&db).await {
                Ok(s) if s.pushed > 0 || s.imported > 0 => {
                    tracing::info!("google sync: pushed {}, imported {}", s.pushed, s.imported);
                }
                Ok(_) => {} // connected but nothing changed, or idle — stay quiet
                Err(e) => tracing::warn!("google sync cycle failed: {e:#}"),
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::google::calendar::{EventPage, EventWrite, GCalEvent};
    use async_trait::async_trait;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeCalendar {
        inserted: Mutex<Vec<String>>,
        page: Mutex<Vec<GCalEvent>>,
    }
    #[async_trait]
    impl CalendarApi for FakeCalendar {
        async fn insert(&self, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
            self.inserted.lock().unwrap().push(w.summary.clone());
            Ok(GCalEvent {
                id: format!("gid-{}", w.summary), etag: "e1".into(), summary: Some(w.summary.clone()),
                location: None, description: None, start_rfc3339: Some(w.start_rfc3339_z.clone()),
                updated: Some("2026-06-12T00:00:00Z".into()), cancelled: false, app_owned: true,
            })
        }
        async fn patch(&self, _id: &str, _etag: &str, w: &EventWrite) -> Result<GCalEvent, CalendarError> {
            Ok(GCalEvent { id: "gid".into(), etag: "e2".into(), summary: Some(w.summary.clone()),
                location: None, description: None, start_rfc3339: Some(w.start_rfc3339_z.clone()),
                updated: Some("2026-06-12T00:00:00Z".into()), cancelled: false, app_owned: true })
        }
        async fn delete(&self, _id: &str) -> Result<(), CalendarError> { Ok(()) }
        async fn list(&self, _t: Option<&str>, _a: &str, _b: &str) -> Result<EventPage, CalendarError> {
            Ok(EventPage { events: self.page.lock().unwrap().clone(), next_sync_token: Some("tok-next".into()) })
        }
    }

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    #[tokio::test]
    async fn outbound_creates_then_marks_synced() {
        let db = mem_db().await;
        events::create(&db, "rapat", None, None, "2026-06-13T07:00:00Z").await.unwrap();
        let cal = FakeCalendar::default();
        let summary = run_pass(&db, &cal).await.unwrap();
        assert_eq!(summary.pushed, 1);
        assert_eq!(summary.imported, 0);
        assert_eq!(cal.inserted.lock().unwrap().len(), 1);
        assert!(events::pending_push(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn inbound_imports_foreign_event_readonly() {
        let db = mem_db().await;
        let cal = FakeCalendar::default();
        cal.page.lock().unwrap().push(GCalEvent {
            id: "gf-1".into(), etag: "e".into(), summary: Some("dokter".into()), location: None,
            description: None, start_rfc3339: Some("2026-06-13T03:00:00Z".into()),
            updated: Some("2026-06-12T09:00:00Z".into()), cancelled: false, app_owned: false,
        });
        let summary = run_pass(&db, &cal).await.unwrap();
        assert_eq!(summary.imported, 1);
        assert_eq!(summary.pushed, 0);
        let row = events::get_by_google_id(&db, "gf-1").await.unwrap().unwrap();
        assert_eq!(row.source, "google");
        assert_eq!(row.title, "dokter");
    }

    /// Live round-trip against a real Google account. Requires GOOGLE_CLIENT_ID,
    /// GOOGLE_CLIENT_SECRET, GOOGLE_REDIRECT_URI, GOOGLE_TOKEN_ENC_KEY, and an
    /// already-connected google_integration row in a file DB at GOOGLE_SMOKE_DB.
    /// Run: GOOGLE_SMOKE_DB=sqlite:///tmp/smoke.db cargo test google::engine::tests::live_cycle -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_cycle() {
        let url = match std::env::var("GOOGLE_SMOKE_DB") { Ok(u) => u, Err(_) => return };
        let db = crate::db::connect(&url).await.unwrap();
        run_cycle(&db).await.unwrap();
        let row = crate::repo::google_integration::get(&db).await.unwrap().unwrap();
        assert_eq!(row.status, "connected", "last_error={:?}", row.last_error);
    }
}
