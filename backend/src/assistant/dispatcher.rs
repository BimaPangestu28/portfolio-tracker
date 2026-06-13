//! Execute one tool call against the database. Ok(text) feeds back to the
//! model as a tool_result; Err(text) becomes an is_error tool_result so the
//! model can self-correct or ask the user.

use crate::db::Db;
use super::time::{parse_tool_datetime, to_db_utc, to_wib_display};

/// Route one tool call by name.
pub async fn dispatch(db: &Db, name: &str, input: &serde_json::Value) -> Result<String, String> {
    match name {
        "create_todo" => create_todo(db, input).await,
        "list_todos" => list_todos(db).await,
        "complete_todo" => complete_todo(db, input).await,
        "create_reminder" => create_reminder(db, input).await,
        "list_reminders" => list_reminders(db).await,
        "cancel_reminder" => cancel_reminder(db, input).await,
        "get_portfolio_summary" => portfolio_summary(db).await,
        "search_memory" => search_memory(input).await,
        "remember" => remember(input).await,
        "create_event" => create_event(db, input).await,
        "list_events" => list_events(db, input).await,
        "cancel_event" => cancel_event(db, input).await,
        "reject_review" => reject_review(db, input).await,
        "list_accounts" => list_accounts(db).await,
        "create_account" => create_account(db, input).await,
        "list_pending_reviews" => list_pending_reviews(db).await,
        "confirm_review" => confirm_review(db, input).await,
        "list_instruments" => list_instruments(db).await,
        "list_projects" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_list_projects(&api).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "create_project" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_create_project(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "create_task" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_create_task(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "list_tasks" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_list_tasks(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        _ => Err(format!("unknown tool: {name}")),
    }
}

fn str_arg<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty())
}

fn id_arg(input: &serde_json::Value, key: &str) -> Result<i64, String> {
    input
        .get(key)
        .and_then(|v| v.as_i64())
        .ok_or_else(|| format!("missing integer argument '{key}'"))
}

/// Optional integer argument: absent/null → None; present-but-not-integer is
/// an error so the model self-corrects instead of assuming a silent default.
fn optional_id(input: &serde_json::Value, key: &str) -> Result<Option<i64>, String> {
    match input.get(key) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(v) => Ok(Some(
            v.as_i64()
                .ok_or_else(|| format!("{key} must be an integer, got {v}"))?,
        )),
    }
}

async fn create_todo(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let title = str_arg(input, "title").ok_or("missing required argument 'title'")?;
    let due_at = match str_arg(input, "due_at") {
        Some(raw) => {
            let dt = parse_tool_datetime(raw)
                .ok_or_else(|| format!("unparseable due_at '{raw}' — use RFC3339 with +07:00"))?;
            Some(to_db_utc(dt))
        }
        None => None,
    };
    let todo = crate::repo::todos::create(db, title, str_arg(input, "notes"), due_at.as_deref())
        .await
        .map_err(|e| format!("db error: {e}"))?;
    Ok(format!("created todo #{} '{}'", todo.id, todo.title))
}

async fn list_todos(db: &Db) -> Result<String, String> {
    let todos = crate::repo::todos::list_open(db).await.map_err(|e| format!("db error: {e}"))?;
    if todos.is_empty() {
        return Ok("no open todos".into());
    }
    let mut out = String::new();
    for t in todos {
        out.push_str(&format!("#{} {}", t.id, t.title));
        if let Some(due) = &t.due_at {
            out.push_str(&format!(" (due {})", to_wib_display(due)));
        }
        if let Some(notes) = &t.notes {
            out.push_str(&format!(" — {notes}"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn complete_todo(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    let done = crate::repo::todos::complete(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if done {
        Ok(format!("todo #{id} marked done"))
    } else {
        Err(format!("todo #{id} not found or already done"))
    }
}

async fn create_reminder(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let message = str_arg(input, "message").ok_or("missing required argument 'message'")?;
    let raw = str_arg(input, "remind_at").ok_or("missing required argument 'remind_at'")?;
    let remind_at = parse_tool_datetime(raw)
        .ok_or_else(|| format!("unparseable remind_at '{raw}' — use RFC3339 with +07:00"))?;
    if remind_at <= chrono::Utc::now() {
        return Err(format!("remind_at '{raw}' is in the past — ask the user for a future time"));
    }
    let recurrence = str_arg(input, "recurrence").unwrap_or("none");
    if !matches!(recurrence, "none" | "daily" | "weekly" | "monthly") {
        return Err(format!("invalid recurrence '{recurrence}' — use none/daily/weekly/monthly"));
    }
    // A present-but-non-integer todo_id must error (not silently unlink) so
    // the model can self-correct instead of believing the link was made.
    let todo_id = match input.get("todo_id") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => Some(v.as_i64().ok_or_else(|| format!("todo_id must be an integer, got {v}"))?),
    };
    if let Some(tid) = todo_id {
        crate::repo::todos::get(db, tid).await.map_err(|_| format!("todo #{tid} not found"))?;
    }
    let reminder =
        crate::repo::reminders::create(db, todo_id, message, &to_db_utc(remind_at), recurrence)
            .await
            .map_err(|e| format!("db error: {e}"))?;
    Ok(format!(
        "created reminder #{} '{}' at {}{}",
        reminder.id,
        reminder.message,
        to_wib_display(&reminder.remind_at),
        if reminder.recurrence == "none" { String::new() } else { format!(" (repeats {})", reminder.recurrence) },
    ))
}

async fn list_reminders(db: &Db) -> Result<String, String> {
    let reminders =
        crate::repo::reminders::list_pending(db).await.map_err(|e| format!("db error: {e}"))?;
    if reminders.is_empty() {
        return Ok("no pending reminders".into());
    }
    let mut out = String::new();
    for r in reminders {
        out.push_str(&format!("#{} '{}' at {}", r.id, r.message, to_wib_display(&r.remind_at)));
        if r.recurrence != "none" {
            out.push_str(&format!(" (repeats {})", r.recurrence));
        }
        if let Some(todo_id) = r.todo_id {
            out.push_str(&format!(" [todo #{todo_id}]"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn cancel_reminder(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    let cancelled =
        crate::repo::reminders::cancel(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if cancelled {
        Ok(format!("reminder #{id} cancelled"))
    } else {
        Err(format!("reminder #{id} not found or not pending"))
    }
}

async fn portfolio_summary(db: &Db) -> Result<String, String> {
    let summary = crate::service::portfolio::build_summary(db)
        .await
        .map_err(|e| format!("summary error: {e}"))?;
    let instruments =
        crate::repo::instruments::list(db).await.map_err(|e| format!("db error: {e}"))?;
    Ok(crate::service::chat::build_context(&summary, &instruments))
}

/// How many facts an explicit memory search returns to the model — larger
/// than the auto-inject limit because the user asked for recall.
const TOOL_SEARCH_LIMIT: u32 = 15;

async fn search_memory(input: &serde_json::Value) -> Result<String, String> {
    let query = str_arg(input, "query").ok_or("missing required argument 'query'")?;
    let Some(memory) = super::memory::MemoryClient::from_env() else {
        return Err("long-term memory is not configured".into());
    };
    let facts = memory
        .try_search(query, TOOL_SEARCH_LIMIT)
        .await
        .map_err(|e| format!("long-term memory is temporarily unreachable: {e}"))?;
    if facts.is_empty() {
        return Ok("no memories found for that query".into());
    }
    let mut out = String::new();
    for f in facts {
        out.push_str(&format!("- {}", f.fact));
        if let Some(valid_at) = &f.valid_at {
            out.push_str(&format!(" (as of {valid_at})"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn remember(input: &serde_json::Value) -> Result<String, String> {
    let note = str_arg(input, "note").ok_or("missing required argument 'note'")?;
    let Some(memory) = super::memory::MemoryClient::from_env() else {
        return Err("long-term memory is not configured".into());
    };
    memory
        .add_episode(note, "manual")
        .await
        .map_err(|e| format!("could not save the note: {e}"))?;
    Ok("noted — saved to long-term memory".into())
}

/// Default lead time for the automatic pre-event reminder.
const DEFAULT_EVENT_REMIND_MINUTES: i64 = 30;
/// Ceiling for the pre-event reminder offset (one week) — also guards the
/// chrono Duration arithmetic against overflow panics from absurd values.
const MAX_EVENT_REMIND_MINUTES: i64 = 7 * 24 * 60;
/// Default lookahead for list_events.
const DEFAULT_EVENT_RANGE_DAYS: i64 = 7;

async fn create_event(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let title = str_arg(input, "title").ok_or("missing required argument 'title'")?;
    let raw = str_arg(input, "start_at").ok_or("missing required argument 'start_at'")?;
    let start = parse_tool_datetime(raw)
        .ok_or_else(|| format!("unparseable start_at '{raw}' — use RFC3339 with +07:00"))?;
    if start <= chrono::Utc::now() {
        return Err(format!("start_at '{raw}' is in the past — ask the user for a future time"));
    }
    let remind_minutes = match input.get("remind_minutes_before") {
        None | Some(serde_json::Value::Null) => DEFAULT_EVENT_REMIND_MINUTES,
        Some(v) => v
            .as_i64()
            .filter(|m| (0..=MAX_EVENT_REMIND_MINUTES).contains(m))
            .ok_or_else(|| {
                format!(
                    "remind_minutes_before must be an integer between 0 and {MAX_EVENT_REMIND_MINUTES}, got {v}"
                )
            })?,
    };
    let event = crate::repo::events::create(
        db,
        title,
        str_arg(input, "location"),
        str_arg(input, "notes"),
        &to_db_utc(start),
    )
    .await
    .map_err(|e| format!("db error: {e}"))?;

    let reminder_note = if remind_minutes == 0 {
        String::new()
    } else {
        let remind_at = start - chrono::Duration::minutes(remind_minutes);
        if remind_at <= chrono::Utc::now() {
            " (terlalu dekat untuk reminder otomatis)".to_string()
        } else {
            let location_part = event
                .location
                .as_deref()
                .map(|l| format!(" di {l}"))
                .unwrap_or_default();
            let message =
                format!("📅 {}{} — {} menit lagi", event.title, location_part, remind_minutes);
            crate::repo::reminders::create_for_event(db, event.id, &message, &to_db_utc(remind_at))
                .await
                .map_err(|e| format!("event created but reminder failed: {e}"))?;
            format!(" (reminder {remind_minutes} menit sebelumnya dibuat)")
        }
    };
    Ok(format!(
        "created event #{} '{}' at {}{}",
        event.id,
        event.title,
        to_wib_display(&event.start_at),
        reminder_note,
    ))
}

async fn list_events(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let now = chrono::Utc::now();
    let from = match str_arg(input, "from") {
        Some(raw) => parse_tool_datetime(raw)
            .ok_or_else(|| format!("unparseable from '{raw}' — use RFC3339 with +07:00"))?,
        None => now,
    };
    let to = match str_arg(input, "to") {
        Some(raw) => parse_tool_datetime(raw)
            .ok_or_else(|| format!("unparseable to '{raw}' — use RFC3339 with +07:00"))?,
        None => from + chrono::Duration::days(DEFAULT_EVENT_RANGE_DAYS),
    };
    if to <= from {
        return Err("'to' must be after 'from'".into());
    }
    let events = crate::repo::events::list_between(db, &to_db_utc(from), &to_db_utc(to))
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if events.is_empty() {
        return Ok("no events in that range".into());
    }
    let mut out = String::new();
    for e in events {
        out.push_str(&format!("- #{} {}: {}", e.id, to_wib_display(&e.start_at), e.title));
        if let Some(location) = &e.location {
            out.push_str(&format!(" ({location})"));
        }
        if let Some(notes) = &e.notes {
            out.push_str(&format!(" — {notes}"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn cancel_event(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let id = id_arg(input, "id")?;
    // Verify the event exists and is scheduled BEFORE touching its reminder.
    let event = crate::repo::events::get(db, id)
        .await
        .map_err(|_| format!("event #{id} not found or already cancelled"))?;
    if event.status != "scheduled" {
        return Err(format!("event #{id} not found or already cancelled"));
    }
    // Reminder first: a failure here leaves a still-scheduled event with no
    // reminder — the recoverable failure mode.
    let reminder_cancelled = crate::repo::reminders::cancel_by_event(db, id)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    let cancelled =
        crate::repo::events::cancel(db, id).await.map_err(|e| format!("db error: {e}"))?;
    if !cancelled {
        return Err(format!("event #{id} not found or already cancelled"));
    }
    Ok(format!(
        "event #{id} cancelled{}",
        if reminder_cancelled { " (its reminder too)" } else { "" }
    ))
}

async fn reject_review(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let review_id = id_arg(input, "review_id")?;
    crate::ingestion::review::reject(db, review_id)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(format!("review #{review_id} ditolak"))
}

async fn list_accounts(db: &Db) -> Result<String, String> {
    let accounts = crate::repo::accounts::list(db).await.map_err(|e| format!("db error: {e}"))?;
    if accounts.is_empty() {
        return Ok("no accounts yet".into());
    }
    let mut out = String::new();
    for a in accounts {
        out.push_str(&format!("#{} {} ({})\n", a.id, a.name, a.account_type));
    }
    Ok(out)
}

async fn clickup_create_project(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let name = str_arg(input, "name").ok_or("missing required argument 'name'")?;
    let project = api.create_project(name).await.map_err(|e| format!("{e}"))?;
    Ok(format!("project '{}' dibuat di ClickUp", project.name))
}

async fn clickup_create_task(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let project = str_arg(input, "project").ok_or("missing required argument 'project'")?;
    let title = str_arg(input, "title").ok_or("missing required argument 'title'")?;
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    let matched = projects
        .iter()
        .find(|p| p.name.eq_ignore_ascii_case(project))
        .ok_or_else(|| format!("project '{project}' belum ada — tawarkan buat project baru dulu"))?;
    let due_date_ms = match str_arg(input, "due") {
        Some(raw) => {
            let dt = parse_tool_datetime(raw)
                .ok_or_else(|| format!("due '{raw}' tidak terbaca — pakai RFC3339 +07:00"))?;
            Some(dt.timestamp_millis())
        }
        None => None,
    };
    let task = crate::clickup::NewTask { name: title.to_string(), due_date_ms };
    api.create_task(&matched.id, &task).await.map_err(|e| format!("{e}"))?;
    Ok(format!("task '{title}' ditambahkan ke project '{}'", matched.name))
}

async fn clickup_list_tasks(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let scope = str_arg(input, "scope").unwrap_or("open");
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    let targets: Vec<&crate::clickup::Project> = match str_arg(input, "project") {
        Some(name) => {
            let p = projects.iter().find(|p| p.name.eq_ignore_ascii_case(name))
                .ok_or_else(|| format!("project '{name}' belum ada"))?;
            vec![p]
        }
        None => projects.iter().collect(),
    };
    let now_ms = chrono::Utc::now().timestamp_millis();
    let end_today = crate::assistant::time::end_of_today_wib_ms(chrono::Utc::now());
    let mut out = String::new();
    for project in targets {
        let tasks = api.list_tasks(&project.id).await.map_err(|e| format!("{e}"))?;
        let mut lines = String::new();
        for t in &tasks {
            let keep = match scope {
                "overdue" => t.due_date_ms.is_some_and(|d| d < now_ms),
                "today" => t.due_date_ms.is_some_and(|d| d >= now_ms && d <= end_today),
                _ => true,
            };
            if !keep { continue; }
            lines.push_str(&format!("  [{}] {}\n", t.id, t.name));
        }
        if !lines.is_empty() {
            out.push_str(&format!("{}:\n{lines}", project.name));
        }
    }
    if out.is_empty() {
        return Ok("tidak ada task".into());
    }
    Ok(out)
}

async fn clickup_list_projects(api: &dyn crate::clickup::ClickUpApi) -> Result<String, String> {
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    if projects.is_empty() {
        return Ok("belum ada project di ClickUp".into());
    }
    let mut out = String::new();
    for p in projects {
        out.push_str(&format!("#{} {}\n", p.id, p.name));
    }
    Ok(out)
}

async fn list_instruments(db: &Db) -> Result<String, String> {
    let instruments = crate::repo::instruments::list(db).await.map_err(|e| format!("db error: {e}"))?;
    if instruments.is_empty() {
        return Ok("no instruments yet".into());
    }
    let mut out = String::new();
    for i in instruments {
        out.push_str(&format!("#{} {} — {} ({})\n", i.id, i.symbol, i.name, i.instrument_type));
    }
    Ok(out)
}

async fn create_account(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let name = str_arg(input, "name").ok_or("missing required argument 'name'")?;
    let account_type =
        str_arg(input, "account_type").ok_or("missing required argument 'account_type'")?;
    let native_currency =
        str_arg(input, "native_currency").ok_or("missing required argument 'native_currency'")?;
    let account = crate::repo::accounts::create(db, &crate::repo::accounts::NewAccount {
        name: name.to_string(),
        account_type: account_type.to_string(),
        institution: str_arg(input, "institution").map(str::to_string),
        native_currency: native_currency.to_string(),
        note: str_arg(input, "note").map(str::to_string),
    })
    .await
    .map_err(|e| format!("db error: {e}"))?;
    Ok(format!("created account #{} '{}'", account.id, account.name))
}

async fn list_pending_reviews(db: &Db) -> Result<String, String> {
    let items = crate::repo::review_items::list_by_status(db, "pending")
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if items.is_empty() {
        return Ok("no pending review items".into());
    }
    let mut out = String::new();
    for item in items {
        let entry: Option<crate::ingestion::extract::ExtractedEntry> =
            serde_json::from_str(&item.payload_json).ok();
        let entry_type = entry.as_ref().map(|e| e.entry_type.as_str()).unwrap_or("?");
        let instrument = match item.suggested_instrument_id {
            Some(instrument_id) => crate::repo::instruments::get(db, instrument_id)
                .await
                .ok()
                .map(|i| format!("{} ({})", i.symbol, i.name))
                .unwrap_or_else(|| "❓ belum dikenali".into()),
            None => "❓ belum dikenali".into(),
        };
        let account = match item.suggested_account_id {
            Some(account_id) => crate::repo::accounts::get(db, account_id)
                .await
                .ok()
                .map(|a| a.name)
                .unwrap_or_else(|| "❓ belum dikenali".into()),
            None => "❓ belum dikenali".into(),
        };
        out.push_str(&format!(
            "#{} {} — instrumen: {instrument} — akun: {account}",
            item.id, entry_type
        ));
        if let Some(e) = &entry {
            if let (Some(quantity), Some(price)) = (&e.quantity, &e.price_native) {
                out.push_str(&format!(" — {quantity} @ {price}"));
            } else if let Some(amount) = &e.amount_native {
                out.push_str(&format!(" — nominal {amount}"));
            }
            if let Some(date) = &e.executed_at {
                out.push_str(&format!(" — {date}"));
            }
        }
        if item.suggested_account_id.is_none() || item.suggested_instrument_id.is_none() {
            out.push_str(" [perlu dilengkapi sebelum konfirmasi]");
        }
        out.push('\n');
    }
    Ok(out)
}

async fn confirm_review(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let review_id = id_arg(input, "review_id")?;
    let account_id = optional_id(input, "account_id")?;
    let instrument_id = optional_id(input, "instrument_id")?;
    if account_id.is_some() || instrument_id.is_some() {
        crate::repo::review_items::set_suggestions(db, review_id, account_id, instrument_id)
            .await
            .map_err(|e| format!("db error: {e}"))?;
    }
    let item = crate::repo::review_items::get(db, review_id)
        .await
        .map_err(|_| format!("review #{review_id} not found"))?;
    let payload = crate::ingestion::review::build_confirm_payload(&item)?;
    let txn_id = crate::ingestion::review::confirm(db, review_id, &payload)
        .await
        .map_err(|e| format!("{e}"))?;
    Ok(format!("transaksi #{txn_id} dibuat dari review #{review_id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    /// A future remind_at the model would plausibly emit (WIB offset).
    fn future_wib() -> String {
        (chrono::Utc::now() + chrono::Duration::days(1))
            .with_timezone(&super::super::time::wib())
            .to_rfc3339()
    }

    #[tokio::test]
    async fn create_todo_inserts_and_reports() {
        let db = mem_db().await;
        let out = dispatch(&db, "create_todo", &serde_json::json!({
            "title": "bayar listrik", "due_at": "2026-06-12T09:00:00+07:00"
        })).await.unwrap();
        assert!(out.contains("bayar listrik"), "{out}");
        let todos = crate::repo::todos::list_open(&db).await.unwrap();
        assert_eq!(todos.len(), 1);
        // 09:00 WIB stored as 02:00 UTC.
        assert_eq!(todos[0].due_at.as_deref(), Some("2026-06-12T02:00:00Z"));
    }

    #[tokio::test]
    async fn create_todo_requires_title() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_todo", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("title"), "{err}");
    }

    #[tokio::test]
    async fn create_todo_rejects_bad_due_at() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_todo", &serde_json::json!({
            "title": "x", "due_at": "besok"
        })).await.unwrap_err();
        assert!(err.contains("besok"), "{err}");
    }

    #[tokio::test]
    async fn list_todos_renders_rows_or_empty_note() {
        let db = mem_db().await;
        assert_eq!(dispatch(&db, "list_todos", &serde_json::json!({})).await.unwrap(), "no open todos");
        crate::repo::todos::create(&db, "beli kado", None, None).await.unwrap();
        let out = dispatch(&db, "list_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("beli kado"), "{out}");
    }

    #[tokio::test]
    async fn complete_todo_round_trips_and_errors_when_done() {
        let db = mem_db().await;
        let todo = crate::repo::todos::create(&db, "x", None, None).await.unwrap();
        let out = dispatch(&db, "complete_todo", &serde_json::json!({ "id": todo.id })).await.unwrap();
        assert!(out.contains("done"), "{out}");
        let err = dispatch(&db, "complete_todo", &serde_json::json!({ "id": todo.id })).await.unwrap_err();
        assert!(err.contains("already done") || err.contains("not found"), "{err}");
    }

    #[tokio::test]
    async fn create_reminder_validates_and_inserts() {
        let db = mem_db().await;
        let out = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "bayar listrik", "remind_at": future_wib(), "recurrence": "daily"
        })).await.unwrap();
        assert!(out.contains("bayar listrik"), "{out}");
        let pending = crate::repo::reminders::list_pending(&db).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].recurrence, "daily");
    }

    #[tokio::test]
    async fn create_reminder_rejects_past_times() {
        let db = mem_db().await;
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": past
        })).await.unwrap_err();
        assert!(err.contains("past"), "{err}");
    }

    #[tokio::test]
    async fn create_reminder_rejects_unknown_recurrence_and_todo() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": future_wib(), "recurrence": "hourly"
        })).await.unwrap_err();
        assert!(err.contains("hourly"), "{err}");
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": future_wib(), "todo_id": 999
        })).await.unwrap_err();
        assert!(err.contains("999"), "{err}");
        // Wrong type must error, not silently create an unlinked reminder.
        let err = dispatch(&db, "create_reminder", &serde_json::json!({
            "message": "x", "remind_at": future_wib(), "todo_id": "5"
        })).await.unwrap_err();
        assert!(err.contains("integer"), "{err}");
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn cancel_reminder_round_trips() {
        let db = mem_db().await;
        let r = crate::repo::reminders::create(&db, None, "x", "2099-01-01T00:00:00Z", "none")
            .await.unwrap();
        let out = dispatch(&db, "cancel_reminder", &serde_json::json!({ "id": r.id })).await.unwrap();
        assert!(out.contains("cancelled"), "{out}");
        let err = dispatch(&db, "cancel_reminder", &serde_json::json!({ "id": r.id })).await.unwrap_err();
        assert!(err.contains("not"), "{err}");
    }

    #[tokio::test]
    async fn unknown_tool_is_an_error() {
        let db = mem_db().await;
        let err = dispatch(&db, "fly_to_moon", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("fly_to_moon"), "{err}");
    }

    #[tokio::test]
    async fn search_memory_requires_query_and_errors_when_unconfigured() {
        let db = mem_db().await;
        let err = dispatch(&db, "search_memory", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("query"), "{err}");
        // Tests never set MEMORY_SERVICE_URL, so memory is unconfigured here.
        let err = dispatch(&db, "search_memory", &serde_json::json!({ "query": "anak" }))
            .await
            .unwrap_err();
        assert!(err.contains("not configured"), "{err}");
    }

    #[tokio::test]
    async fn remember_requires_note_and_errors_when_unconfigured() {
        let db = mem_db().await;
        let err = dispatch(&db, "remember", &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("note"), "{err}");
        let err = dispatch(&db, "remember", &serde_json::json!({ "note": "paspor di laci" }))
            .await
            .unwrap_err();
        assert!(err.contains("not configured"), "{err}");
    }

    #[tokio::test]
    async fn portfolio_summary_renders_for_an_empty_db() {
        let db = mem_db().await;
        let out = dispatch(&db, "get_portfolio_summary", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Net worth"), "{out}");
    }

    #[tokio::test]
    async fn create_event_makes_event_and_default_linked_reminder() {
        let db = mem_db().await;
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        let out = dispatch(&db, "create_event", &serde_json::json!({
            "title": "meeting vendor", "start_at": start, "location": "kantor"
        })).await.unwrap();
        assert!(out.contains("meeting vendor"), "{out}");
        let events = crate::repo::events::list_between(&db, "2000-01-01T00:00:00Z", "2099-01-01T00:00:00Z")
            .await.unwrap();
        assert_eq!(events.len(), 1);
        let reminders = crate::repo::reminders::list_pending(&db).await.unwrap();
        assert_eq!(reminders.len(), 1);
        assert_eq!(reminders[0].event_id, Some(events[0].id));
        assert!(reminders[0].message.contains("meeting vendor"), "{}", reminders[0].message);
        assert!(reminders[0].message.contains("kantor"), "{}", reminders[0].message);
        assert!(reminders[0].message.contains("30 menit"), "{}", reminders[0].message);
        // remind_at = start - 30 minutes (Z format, second precision).
        let start_dt = chrono::DateTime::parse_from_rfc3339(&start).unwrap();
        let expected = crate::assistant::time::to_db_utc(
            (start_dt - chrono::Duration::minutes(30)).with_timezone(&chrono::Utc),
        );
        assert_eq!(reminders[0].remind_at, expected);
    }

    #[tokio::test]
    async fn create_event_zero_minutes_skips_the_reminder() {
        let db = mem_db().await;
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start, "remind_minutes_before": 0
        })).await.unwrap();
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_event_too_soon_for_the_offset_skips_the_reminder() {
        let db = mem_db().await;
        // Starts in 10 minutes; the default 30-minute reminder would be in the past.
        let start = (chrono::Utc::now() + chrono::Duration::minutes(10)).to_rfc3339();
        let out = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start
        })).await.unwrap();
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
        assert!(out.contains("terlalu dekat"), "{out}");
    }

    #[tokio::test]
    async fn create_event_rejects_past_and_bad_input() {
        let db = mem_db().await;
        let past = (chrono::Utc::now() - chrono::Duration::hours(1)).to_rfc3339();
        let err = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": past
        })).await.unwrap_err();
        assert!(err.contains("past"), "{err}");
        let err = dispatch(&db, "create_event", &serde_json::json!({ "title": "x" }))
            .await.unwrap_err();
        assert!(err.contains("start_at"), "{err}");
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        let err = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start, "remind_minutes_before": -5
        })).await.unwrap_err();
        assert!(err.contains("between 0 and"), "{err}");
        let err = dispatch(&db, "create_event", &serde_json::json!({
            "title": "x", "start_at": start, "remind_minutes_before": 9_000_000_000_000i64
        })).await.unwrap_err();
        assert!(err.contains("between 0 and"), "{err}");
    }

    #[tokio::test]
    async fn list_events_defaults_to_a_week_and_renders_wib() {
        let db = mem_db().await;
        assert_eq!(
            dispatch(&db, "list_events", &serde_json::json!({})).await.unwrap(),
            "no events in that range"
        );
        crate::repo::events::create(
            &db, "meeting", Some("kantor"), None,
            &crate::assistant::time::to_db_utc(chrono::Utc::now() + chrono::Duration::days(2)),
        ).await.unwrap();
        // 9 days out: outside the default window.
        crate::repo::events::create(
            &db, "far away", None, None,
            &crate::assistant::time::to_db_utc(chrono::Utc::now() + chrono::Duration::days(9)),
        ).await.unwrap();
        let out = dispatch(&db, "list_events", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("meeting"), "{out}");
        assert!(out.contains("kantor"), "{out}");
        assert!(out.contains("WIB"), "{out}");
        assert!(!out.contains("far away"), "{out}");
    }

    #[tokio::test]
    async fn list_events_rejects_inverted_ranges() {
        let db = mem_db().await;
        let err = dispatch(&db, "list_events", &serde_json::json!({
            "from": "2026-06-20T00:00:00+07:00", "to": "2026-06-13T00:00:00+07:00"
        })).await.unwrap_err();
        assert!(err.contains("after"), "{err}");
    }

    #[tokio::test]
    async fn cancel_event_cascades_to_its_reminder() {
        let db = mem_db().await;
        let start = (chrono::Utc::now() + chrono::Duration::hours(3)).to_rfc3339();
        dispatch(&db, "create_event", &serde_json::json!({ "title": "m", "start_at": start }))
            .await.unwrap();
        let event_id = crate::repo::events::list_between(&db, "2000-01-01T00:00:00Z", "2099-01-01T00:00:00Z")
            .await.unwrap()[0].id;
        let out = dispatch(&db, "cancel_event", &serde_json::json!({ "id": event_id }))
            .await.unwrap();
        assert!(out.contains("cancelled"), "{out}");
        assert!(crate::repo::reminders::list_pending(&db).await.unwrap().is_empty());
        let err = dispatch(&db, "cancel_event", &serde_json::json!({ "id": event_id }))
            .await.unwrap_err();
        assert!(err.contains("not found or already cancelled"), "{err}");
    }

    /// Insert a bare `pending` review_item into `db`. When `account_id` or
    /// `instrument_id` are `None` the FK columns are left NULL (safe because
    /// `PRAGMA foreign_keys = ON` only rejects *non-NULL* values that don't
    /// reference an existing row). Callers that need those FKs must create the
    /// parent rows first and pass the resulting ids.
    pub(super) async fn seed_pending_item(
        db: &Db,
        account_id: Option<i64>,
        instrument_id: Option<i64>,
    ) -> i64 {
        crate::repo::review_items::create(db, &crate::repo::review_items::NewReviewItem {
            batch_id: "b1",
            source_kind: "image",
            source_filename: "f.jpg",
            source_path: "",
            doc_type: "txn_history",
            needs_attention: false,
            payload_json: r#"{ "entry_type": "buy", "symbol": "BTC", "quantity": "1",
                "price_native": "100", "fee_native": "0", "currency": "IDR",
                "executed_at": "2026-06-04", "confidence": 0.95 }"#,
            raw_llm_json: "{}",
            suggested_instrument_id: instrument_id,
            suggested_account_id: account_id,
        })
        .await
        .unwrap()
        .id
    }

    #[tokio::test]
    async fn confirm_review_with_account_override_creates_txn() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        let account = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Nanovest".into(), account_type: "broker".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        // Account unknown at ingest time; instrument matched.
        let id = seed_pending_item(&db, None, Some(instrument.id)).await;

        let out = dispatch(&db, "confirm_review", &serde_json::json!({
            "review_id": id, "account_id": account.id
        })).await.unwrap();
        assert!(out.contains("dibuat"), "{out}");

        let item = crate::repo::review_items::get(&db, id).await.unwrap();
        assert_eq!(item.status, "confirmed");
        assert!(item.created_txn_id.is_some());
    }

    #[tokio::test]
    async fn confirm_review_still_incomplete_returns_reason() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        // No account supplied and none seeded → not confirmable.
        let id = seed_pending_item(&db, None, Some(instrument.id)).await;
        let err = dispatch(&db, "confirm_review", &serde_json::json!({ "review_id": id })).await.unwrap_err();
        assert!(err.contains("akun belum dikenali"), "{err}");
        let item = crate::repo::review_items::get(&db, id).await.unwrap();
        assert_eq!(item.status, "pending", "must not confirm");
    }

    #[tokio::test]
    async fn reject_review_marks_item_rejected() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let id = seed_pending_item(&db, None, None).await;
        let out = dispatch(&db, "reject_review", &serde_json::json!({ "review_id": id }))
            .await
            .unwrap();
        assert!(out.contains("ditolak"), "{out}");
        let item = crate::repo::review_items::get(&db, id).await.unwrap();
        assert_eq!(item.status, "rejected");
    }

    #[tokio::test]
    async fn cancel_event_after_reminder_sent_omits_the_reminder_note() {
        let db = mem_db().await;
        let event = crate::repo::events::create(&db, "m", None, None, "2099-01-01T00:00:00Z")
            .await.unwrap();
        let r = crate::repo::reminders::create_for_event(&db, event.id, "x", "2099-01-01T00:00:00Z")
            .await.unwrap();
        crate::repo::reminders::mark_sent(&db, r.id, "2026-06-12T00:00:00Z").await.unwrap();
        let out = dispatch(&db, "cancel_event", &serde_json::json!({ "id": event.id }))
            .await.unwrap();
        assert!(out.contains("cancelled"), "{out}");
        assert!(!out.contains("reminder"), "{out}");
    }

    #[tokio::test]
    async fn create_account_then_list_shows_it() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let out = dispatch(&db, "create_account", &serde_json::json!({
            "name": "Nanovest", "account_type": "broker", "native_currency": "IDR"
        })).await.unwrap();
        assert!(out.contains("Nanovest"), "{out}");

        let listed = dispatch(&db, "list_accounts", &serde_json::json!({})).await.unwrap();
        assert!(listed.contains("Nanovest"), "{listed}");
        assert!(listed.contains("broker"), "{listed}");
    }

    #[tokio::test]
    async fn create_account_requires_name() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let err = dispatch(&db, "create_account", &serde_json::json!({
            "account_type": "broker", "native_currency": "IDR"
        })).await.unwrap_err();
        assert!(err.contains("name"), "{err}");
    }

    #[tokio::test]
    async fn list_pending_reviews_flags_unknown_account() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        let id = seed_pending_item(&db, None, Some(instrument.id)).await;

        let out = dispatch(&db, "list_pending_reviews", &serde_json::json!({})).await.unwrap();
        assert!(out.contains(&format!("#{id}")), "{out}");
        assert!(out.contains("BTC"), "instrument shown: {out}");
        assert!(out.contains("belum dikenali"), "unknown account flagged: {out}");
        assert!(out.contains("perlu dilengkapi"), "blocker noted: {out}");
    }

    #[tokio::test]
    async fn list_pending_reviews_empty_is_explicit() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let out = dispatch(&db, "list_pending_reviews", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("no pending"), "{out}");
    }

    #[tokio::test]
    async fn list_instruments_shows_existing() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        let out = dispatch(&db, "list_instruments", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("BTC"), "{out}");
        assert!(out.contains("Bitcoin"), "{out}");
    }

    #[tokio::test]
    async fn list_instruments_empty_is_explicit() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let out = dispatch(&db, "list_instruments", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("no instruments"), "{out}");
    }

    #[tokio::test]
    async fn confirm_review_with_instrument_override_creates_txn() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "BTC".into(), name: "Bitcoin".into(), instrument_type: "crypto".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        let account = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Nanovest".into(), account_type: "broker".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        // Instrument unknown at ingest; account matched.
        let id = seed_pending_item(&db, Some(account.id), None).await;
        let out = dispatch(&db, "confirm_review", &serde_json::json!({
            "review_id": id, "instrument_id": instrument.id
        })).await.unwrap();
        assert!(out.contains("dibuat"), "{out}");
        let item = crate::repo::review_items::get(&db, id).await.unwrap();
        assert_eq!(item.status, "confirmed");
        assert!(item.created_txn_id.is_some());
    }

    // ── ClickUp fake ─────────────────────────────────────────────────────────

    use crate::clickup::client::{ClickUpApi, ClickUpError, NewTask, Project, Task};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeClickUp {
        projects: Mutex<Vec<Project>>,
        created_tasks: Mutex<Vec<(String, String)>>, // (list_id, title)
        created_dues: Mutex<Vec<Option<i64>>>,       // due_date_ms per created task
        tasks: Mutex<std::collections::HashMap<String, Vec<crate::clickup::client::Task>>>,
        completed: Mutex<Vec<String>>,
    }

    #[async_trait::async_trait]
    impl ClickUpApi for FakeClickUp {
        async fn list_projects(&self) -> Result<Vec<Project>, ClickUpError> {
            Ok(self.projects.lock().unwrap().clone())
        }
        async fn create_project(&self, name: &str) -> Result<Project, ClickUpError> {
            let project = Project { id: format!("list_{name}"), name: name.to_string() };
            self.projects.lock().unwrap().push(project.clone());
            Ok(project)
        }
        async fn create_task(&self, list_id: &str, task: &NewTask) -> Result<String, ClickUpError> {
            self.created_tasks.lock().unwrap().push((list_id.to_string(), task.name.clone()));
            self.created_dues.lock().unwrap().push(task.due_date_ms);
            Ok(format!("task_{}", task.name))
        }
        async fn list_tasks(&self, list_id: &str) -> Result<Vec<crate::clickup::client::Task>, ClickUpError> {
            Ok(self.tasks.lock().unwrap().get(list_id).cloned().unwrap_or_default())
        }
        async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError> {
            self.completed.lock().unwrap().push(task_id.to_string());
            Ok(())
        }
    }

    #[tokio::test]
    async fn list_projects_formats_known_projects() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        let out = clickup_list_projects(&fake).await.unwrap();
        assert!(out.contains("PT AIS"), "{out}");
    }

    #[tokio::test]
    async fn list_projects_empty_is_explicit() {
        let fake = FakeClickUp::default();
        let out = clickup_list_projects(&fake).await.unwrap();
        assert!(out.contains("belum ada project"), "{out}");
    }

    #[tokio::test]
    async fn create_project_creates_and_reports() {
        let fake = FakeClickUp::default();
        let out = clickup_create_project(&fake, &serde_json::json!({ "name": "Klien Baru" })).await.unwrap();
        assert!(out.contains("Klien Baru"), "{out}");
        assert!(fake.projects.lock().unwrap().iter().any(|p| p.name == "Klien Baru"));
    }

    #[tokio::test]
    async fn create_task_parses_due_into_ms() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        clickup_create_task(&fake, &serde_json::json!({
            "project": "PT AIS", "title": "kirim invoice", "due": "2026-06-14T17:00:00+07:00"
        })).await.unwrap();
        // The same instant in epoch ms; proves parse_tool_datetime -> timestamp_millis ran.
        let expected = chrono::DateTime::parse_from_rfc3339("2026-06-14T17:00:00+07:00")
            .unwrap()
            .timestamp_millis();
        assert_eq!(fake.created_dues.lock().unwrap()[0], Some(expected));
    }

    #[tokio::test]
    async fn create_task_without_due_records_none() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        clickup_create_task(&fake, &serde_json::json!({ "project": "PT AIS", "title": "x" }))
            .await.unwrap();
        assert_eq!(fake.created_dues.lock().unwrap()[0], None);
    }

    #[tokio::test]
    async fn create_task_bad_due_errors_and_creates_nothing() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        let err = clickup_create_task(&fake, &serde_json::json!({
            "project": "PT AIS", "title": "x", "due": "besok pagi"
        })).await.unwrap_err();
        assert!(err.contains("tidak terbaca"), "{err}");
        assert!(fake.created_tasks.lock().unwrap().is_empty(), "no task on bad due");
    }

    #[tokio::test]
    async fn create_project_then_create_task_completes_the_recovery_loop() {
        let fake = FakeClickUp::default();
        // Agent flow: project missing -> create it -> retry create_task against it.
        clickup_create_project(&fake, &serde_json::json!({ "name": "Klien Baru" })).await.unwrap();
        let out = clickup_create_task(&fake, &serde_json::json!({
            "project": "klien baru", "title": "bikin kontrak"
        })).await.unwrap();
        assert!(out.contains("bikin kontrak"), "{out}");
        let created = fake.created_tasks.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].0, "list_Klien Baru", "task routed to the freshly created list");
    }

    #[tokio::test]
    async fn create_project_requires_name() {
        let fake = FakeClickUp::default();
        let err = clickup_create_project(&fake, &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("name"), "{err}");
    }

    #[tokio::test]
    async fn create_task_adds_to_matching_project() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        let out = clickup_create_task(&fake, &serde_json::json!({
            "project": "pt ais", "title": "bikin kontrak"
        })).await.unwrap();
        assert!(out.contains("bikin kontrak"), "{out}");
        let created = fake.created_tasks.lock().unwrap();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].0, "l1", "task went to the matched list");
    }

    #[tokio::test]
    async fn create_task_unknown_project_reports_for_offer() {
        let fake = FakeClickUp::default();
        let err = clickup_create_task(&fake, &serde_json::json!({
            "project": "Klien Baru", "title": "x"
        })).await.unwrap_err();
        assert!(err.contains("Klien Baru"), "{err}");
        assert!(err.contains("belum ada"), "{err}");
        assert!(fake.created_tasks.lock().unwrap().is_empty(), "no task created");
    }

    #[tokio::test]
    async fn list_tasks_for_a_project_shows_open_tasks() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![
            Task { id: "t1".into(), name: "bikin kontrak".into(), status: "to do".into(), due_date_ms: None },
        ]);
        let out = clickup_list_tasks(&fake, &serde_json::json!({ "project": "PT AIS" })).await.unwrap();
        assert!(out.contains("bikin kontrak"), "{out}");
        assert!(out.contains("t1"), "task id shown: {out}");
    }

    #[tokio::test]
    async fn list_tasks_overdue_filters_across_projects() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![
            Task { id: "t1".into(), name: "lewat deadline".into(), status: "to do".into(), due_date_ms: Some(1_000) },
            Task { id: "t2".into(), name: "tanpa due".into(), status: "to do".into(), due_date_ms: None },
        ]);
        let out = clickup_list_tasks(&fake, &serde_json::json!({ "scope": "overdue" })).await.unwrap();
        assert!(out.contains("lewat deadline"), "{out}");
        assert!(!out.contains("tanpa due"), "no-due task must not be overdue: {out}");
    }

    #[tokio::test]
    async fn list_tasks_empty_is_explicit() {
        let fake = FakeClickUp::default();
        let out = clickup_list_tasks(&fake, &serde_json::json!({ "scope": "today" })).await.unwrap();
        assert!(out.contains("tidak ada task"), "{out}");
    }

    #[tokio::test]
    async fn list_pending_reviews_shows_amount_only_nominal() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "SBF".into(), name: "Sucorinvest Bond Fund".into(), instrument_type: "mutual_fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(4), note: None,
        }).await.unwrap();
        let account = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit".into(), account_type: "broker".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        crate::repo::review_items::create(&db, &crate::repo::review_items::NewReviewItem {
            batch_id: "b9", source_kind: "image", source_filename: "f.jpg",
            source_path: "", doc_type: "txn_history", needs_attention: false,
            payload_json: r#"{ "entry_type": "buy", "instrument_name": "Sucorinvest Bond Fund",
                "amount_native": "13000000", "currency": "IDR", "confidence": 0.72 }"#,
            raw_llm_json: "{}",
            suggested_instrument_id: Some(instrument.id), suggested_account_id: Some(account.id),
        }).await.unwrap();
        let out = dispatch(&db, "list_pending_reviews", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("nominal 13000000"), "{out}");
    }
}
