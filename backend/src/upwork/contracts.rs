//! Upwork contracts → ClickUp project (List) sync. Pure helpers here; the
//! `run_pass` orchestration and polling loop are added in later tasks. One-way,
//! create-only; idempotent via the `upwork_project_link` mapping.

use crate::upwork::client::Contract;

/// The ClickUp List name for a contract: "{client} — {title}", or just the
/// title when the client name is empty.
pub fn list_name(contract: &Contract) -> String {
    if contract.client_name.trim().is_empty() {
        contract.title.clone()
    } else {
        format!("{} — {}", contract.client_name, contract.title)
    }
}

/// Plain-text Telegram alert announcing a synced contract (no Markdown).
pub fn format_created_alert(name: &str) -> String {
    format!("🗂 New Upwork contract synced to ClickUp: {name}")
}

use crate::clickup::client::ClickUpApi;
use crate::db::Db;
use crate::repo::upwork_project_link;
use crate::upwork::client::UpworkClient;
use crate::upwork::jobs::Notifier;

/// One sync pass against injected seams. For each active contract not yet in the
/// mapping, create a ClickUp List, record the mapping AFTER success, and (when an
/// owner chat is known) send a Telegram heads-up. Returns the number created.
pub async fn run_pass<U: UpworkClient, C: ClickUpApi, N: Notifier>(
    db: &Db,
    upwork: &U,
    clickup: &C,
    notifier: &N,
    owner_chat: Option<i64>,
) -> anyhow::Result<usize> {
    let contracts = match upwork.fetch_contracts().await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("fetch contracts failed: {e}");
            return Ok(0);
        }
    };
    let mut created = 0usize;
    for c in &contracts {
        if upwork_project_link::get(db, &c.id).await?.is_some() {
            continue;
        }
        let name = list_name(c);
        match clickup.create_project(&name).await {
            Ok(project) => {
                upwork_project_link::link(db, &c.id, &project.id, &name).await?;
                if let Some(chat) = owner_chat {
                    if let Err(e) = notifier.send(chat, &format_created_alert(&name)).await {
                        tracing::warn!("contract sync notify failed: {e}");
                    }
                }
                created += 1;
            }
            Err(e) => tracing::warn!("create_project for contract {} failed: {e}", c.id),
        }
    }
    Ok(created)
}

use crate::repo::{telegram_link, upwork_integration};
use crate::upwork::client::HttpUpwork;
use crate::upwork::jobs::TelegramNotifier;
use crate::upwork::oauth::OAuthConfig;

const DEFAULT_POLL_SECS: u64 = 3600;

/// One full cycle: ensure token, build the ClickUp + Telegram clients, run a pass.
pub async fn sync_cycle(db: &Db) -> anyhow::Result<usize> {
    let cfg = OAuthConfig::from_env()?;
    let key = crate::upwork::crypto::key_from_env()?;
    let token = match crate::upwork::engine::ensure_access_token(db, &cfg, &key).await {
        Ok(t) => t,
        Err(e) => {
            upwork_integration::set_status(db, "error", Some(&e.to_string())).await?;
            return Ok(0);
        }
    };
    let clickup = match crate::clickup::client::ClickUpClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!("clickup not configured; contract sync skipped: {e}");
            return Ok(0);
        }
    };
    let tg_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
    let owner_chat = match (telegram_link::get(db).await?, &tg_token) {
        (Some(link), Some(_)) => Some(link.chat_id),
        _ => None,
    };
    let notifier = TelegramNotifier {
        client: crate::telegram::client::TelegramClient::new(tg_token.unwrap_or_default()),
    };
    let upwork = HttpUpwork::new(token);
    run_pass(db, &upwork, &clickup, &notifier, owner_chat).await
}

/// Independent polling loop. No-op when Upwork OAuth env is unset.
pub fn spawn(db: Db) {
    if OAuthConfig::from_env().is_err() {
        tracing::info!("UPWORK_CLIENT_* not set; contract sync disabled");
        return;
    }
    let secs = std::env::var("UPWORK_CONTRACTS_POLL_SECS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(DEFAULT_POLL_SECS);
    let period = std::time::Duration::from_secs(secs);
    tokio::spawn(async move {
        loop {
            match sync_cycle(&db).await {
                Ok(n) if n > 0 => tracing::info!("upwork contract sync: created {n} ClickUp lists"),
                Ok(_) => {}
                Err(e) => tracing::warn!("upwork contract sync cycle failed: {e:#}"),
            }
            tokio::time::sleep(period).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contract(client: &str, title: &str) -> Contract {
        Contract {
            id: "c1".into(), title: title.into(), client_name: client.into(), status: "active".into(),
        }
    }

    #[test]
    fn list_name_joins_client_and_title() {
        assert_eq!(list_name(&contract("Acme", "Build API")), "Acme — Build API");
    }

    #[test]
    fn list_name_falls_back_to_title_when_client_empty() {
        assert_eq!(list_name(&contract("   ", "Build API")), "Build API");
    }

    #[test]
    fn created_alert_has_name_no_markdown() {
        let msg = format_created_alert("Acme — Build API");
        assert!(msg.contains("Acme — Build API"));
        assert!(!msg.contains("**"));
    }

    use crate::clickup::client::{ClickUpApi, ClickUpError, NewTask, Project, RunningEntry, Task, TimeEntry};
    use crate::upwork::client::testkit::FakeUpwork;
    use crate::upwork::jobs::Notifier;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeClickUp { created: Mutex<Vec<String>>, next_id: Mutex<u32> }
    #[async_trait::async_trait]
    impl ClickUpApi for FakeClickUp {
        async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError> { Ok(vec![]) }
        async fn create_project(&self, name: &str) -> Result<Project, ClickUpError> {
            let mut id = self.next_id.lock().unwrap();
            *id += 1;
            self.created.lock().unwrap().push(name.to_string());
            Ok(Project { id: format!("list-{}", *id), name: name.to_string() })
        }
        async fn create_task(&self, _list_id: &str, _task: &NewTask) -> Result<String, ClickUpError> { Ok("t".into()) }
        async fn list_tasks(&self, _list_id: &str) -> Result<Vec<Task>, ClickUpError> { Ok(vec![]) }
        async fn complete_task(&self, _task_id: &str) -> Result<(), ClickUpError> { Ok(()) }
        async fn start_timer(&self, _task_id: &str) -> Result<(), ClickUpError> { Ok(()) }
        async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError> { Ok(None) }
        async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError> { Ok(None) }
        async fn time_entries(&self, _start_ms: i64, _end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError> { Ok(vec![]) }
        async fn add_time_entry(&self, _task_id: &str, _duration_ms: i64, _start_ms: i64) -> Result<(), ClickUpError> { Ok(()) }
    }

    #[derive(Default)]
    struct CapturingNotifier { sent: Mutex<Vec<String>> }
    #[async_trait::async_trait]
    impl Notifier for CapturingNotifier {
        async fn send(&self, _chat: i64, text: &str) -> Result<(), String> {
            self.sent.lock().unwrap().push(text.to_string());
            Ok(())
        }
    }

    async fn mem_db() -> Db { crate::db::connect("sqlite::memory:").await.unwrap() }

    fn active(id: &str, client: &str, title: &str) -> Contract {
        Contract { id: id.into(), title: title.into(), client_name: client.into(), status: "active".into() }
    }

    #[tokio::test]
    async fn creates_lists_for_new_contracts_then_dedupes() {
        let db = mem_db().await;
        let upwork = FakeUpwork::with_contracts(vec![active("c1", "Acme", "API"), active("c2", "Globex", "App")]);
        let clickup = FakeClickUp::default();
        let notifier = CapturingNotifier::default();

        let n = run_pass(&db, &upwork, &clickup, &notifier, Some(42)).await.unwrap();
        assert_eq!(n, 2);
        assert_eq!(clickup.created.lock().unwrap().len(), 2);
        assert_eq!(upwork_project_link::list_all(&db).await.unwrap().len(), 2);
        assert_eq!(notifier.sent.lock().unwrap().len(), 2);
        assert!(notifier.sent.lock().unwrap().iter().any(|m| m.contains("Acme — API")));

        let n2 = run_pass(&db, &upwork, &clickup, &notifier, Some(42)).await.unwrap();
        assert_eq!(n2, 0);
        assert_eq!(clickup.created.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn creates_lists_without_owner_chat_and_skips_notify() {
        let db = mem_db().await;
        let upwork = FakeUpwork::with_contracts(vec![active("c1", "Acme", "API")]);
        let clickup = FakeClickUp::default();
        let notifier = CapturingNotifier::default();

        let n = run_pass(&db, &upwork, &clickup, &notifier, None).await.unwrap();
        assert_eq!(n, 1);
        assert_eq!(upwork_project_link::list_all(&db).await.unwrap().len(), 1);
        assert!(notifier.sent.lock().unwrap().is_empty(), "no owner chat → no notify");
    }
}
