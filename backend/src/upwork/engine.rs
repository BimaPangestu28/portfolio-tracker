//! Orchestrates one earnings sync: ensure a fresh access token, fetch
//! transactions since the stored cursor, plan income rows, insert them
//! idempotently, and advance the cursor. Network access goes through the
//! `UpworkClient` trait so this is testable with a fake.

use crate::db::Db;
use crate::repo::{cashflow, cashflow_categories, upwork_integration};
use crate::upwork::client::UpworkClient;
use crate::upwork::oauth::{self, OAuthConfig};
use crate::upwork::sync::plan_earnings;

const CATEGORY_NAME: &str = "Upwork";

/// Execute one fetch→plan→insert pass with a given client. Pure DB + trait; no
/// env reads, so tests inject a fake. Returns the number of new rows inserted.
pub async fn run_pass<C: UpworkClient>(db: &Db, client: &C) -> anyhow::Result<usize> {
    let cursor = upwork_integration::get(db).await?.and_then(|r| r.earnings_cursor);
    let batch = client
        .fetch_transactions(cursor.as_deref())
        .await
        .map_err(|e| anyhow::anyhow!("upwork fetch failed: {e}"))?;

    let category = cashflow_categories::ensure_by_name(db, CATEGORY_NAME, "income").await?;
    let planned = plan_earnings(&batch.txns, category.id);

    let mut inserted = 0usize;
    for p in &planned {
        if cashflow::insert_sourced(db, &p.cashflow, "upwork", &p.external_ref).await? {
            inserted += 1;
        }
    }
    if let Some(cur) = batch.next_cursor {
        upwork_integration::set_cursor(db, &cur).await?;
    }
    Ok(inserted)
}

/// Ensure a non-expired access token, refreshing if needed. Returns plaintext.
pub(crate) async fn ensure_access_token(db: &Db, cfg: &OAuthConfig, key: &[u8; 32]) -> anyhow::Result<String> {
    let row = upwork_integration::get(db).await?
        .ok_or_else(|| anyhow::anyhow!("not connected"))?;
    let expired = chrono::DateTime::parse_from_rfc3339(&row.expiry)
        .map(|exp| chrono::Utc::now() >= exp.with_timezone(&chrono::Utc))
        .unwrap_or(true);
    if !expired {
        return crate::upwork::crypto::decrypt(&row.access_token, key);
    }
    let refresh = crate::upwork::crypto::decrypt(&row.refresh_token, key)?;
    let tokens = oauth::refresh_access(cfg, &refresh).await?;
    let enc = crate::upwork::crypto::encrypt(&tokens.access_token, key)?;
    let expiry = oauth::expiry_from_now(tokens.expires_in);
    upwork_integration::update_access(db, &enc, &expiry).await?;
    Ok(tokens.access_token)
}

/// One full cycle including auth. Records integration status on failure.
/// Invoked by the manual `POST /api/upwork/sync` route (no background loop in v1).
pub async fn run_cycle(db: &Db) -> anyhow::Result<usize> {
    let Some(row) = upwork_integration::get(db).await? else { return Ok(0) };
    if row.status == "disconnected" { return Ok(0); }
    let cfg = OAuthConfig::from_env()?;
    let key = crate::upwork::crypto::key_from_env()?;
    let token = match ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(0);
        }
    };
    let client = crate::upwork::client::HttpUpwork::new(token);
    match run_pass(db, &client).await {
        Ok(n) => {
            if row.status == "error" {
                upwork_integration::set_status(db, "connected", None).await?;
            }
            Ok(n)
        }
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            Ok(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upwork::client::testkit::FakeUpwork;
    use crate::upwork::UpworkTransaction;

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    fn txn(id: &str, kind: &str, amount: &str) -> UpworkTransaction {
        UpworkTransaction {
            external_id: id.into(), date: "2026-06-10".into(), kind: kind.into(),
            amount: amount.into(), currency: "USD".into(), contract: Some("Acme".into()),
        }
    }

    #[tokio::test]
    async fn run_pass_inserts_earnings_and_is_idempotent() {
        let db = mem_db().await;
        upwork_integration::upsert(&db, "a", "r", "2030-01-01T00:00:00+00:00", "s").await.unwrap();
        let fake = FakeUpwork::with(
            vec![txn("t1", "Hourly", "120.00"), txn("t2", "Service Fee", "-10.00")],
            Some("cur-2".into()),
        );

        let first = run_pass(&db, &fake).await.unwrap();
        assert_eq!(first, 1, "only the earning is inserted");
        let rows = cashflow::list_all(&db).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].direction, "in");
        assert_eq!(rows[0].source.as_deref(), Some("upwork"));

        let cur = upwork_integration::get(&db).await.unwrap().unwrap().earnings_cursor;
        assert_eq!(cur.as_deref(), Some("cur-2"));

        let second = run_pass(&db, &fake).await.unwrap();
        assert_eq!(second, 0);
        assert_eq!(cashflow::list_all(&db).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn run_pass_passes_stored_cursor_to_client() {
        let db = mem_db().await;
        upwork_integration::upsert(&db, "a", "r", "2030-01-01T00:00:00+00:00", "s").await.unwrap();
        upwork_integration::set_cursor(&db, "cur-1").await.unwrap();
        let fake = FakeUpwork::with(vec![], None);
        run_pass(&db, &fake).await.unwrap();
        assert_eq!(fake.seen_cursor.lock().unwrap().as_deref(), Some("cur-1"));
    }

    /// Live round-trip against a real Upwork account. Requires UPWORK_CLIENT_ID,
    /// UPWORK_CLIENT_SECRET, UPWORK_REDIRECT_URI, UPWORK_TOKEN_ENC_KEY, and an
    /// already-connected upwork_integration row in a file DB at UPWORK_SMOKE_DB.
    /// Run: UPWORK_SMOKE_DB=sqlite:///tmp/upwork.db cargo test upwork::engine::tests::live_cycle -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn live_cycle() {
        let url = match std::env::var("UPWORK_SMOKE_DB") { Ok(u) => u, Err(_) => return };
        let db = crate::db::connect(&url).await.unwrap();
        let n = run_cycle(&db).await.unwrap();
        let row = upwork_integration::get(&db).await.unwrap().unwrap();
        assert_eq!(row.status, "connected", "last_error={:?}", row.last_error);
        eprintln!("inserted {n} new earnings");
    }
}
