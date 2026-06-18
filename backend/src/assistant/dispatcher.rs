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
        "plan_day" => plan_day(db).await,
        "rollover_todos" => rollover_todos(db, input).await,
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
        "create_transaction" => create_transaction(db, input).await,
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
        "complete_task" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_complete_task(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "start_timer" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_start_timer(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "stop_timer" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_stop_timer(&api).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "current_timer" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_current_timer(&api).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "time_report" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_time_report(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "add_time_entry" => match crate::clickup::ClickUpClient::from_env() {
            Ok(api) => clickup_add_time_entry(&api, input).await,
            Err(e) => Err(format!("clickup belum dikonfigurasi: {e}")),
        },
        "draft_proposal" => draft_proposal(input).await,
        "list_clients" => invoice_list_clients(db).await,
        "create_invoice" => invoice_create(db, input).await,
        "capture_to_inbox" => capture_to_inbox(db, input).await,
        "list_inbox" => list_inbox(db).await,
        "resolve_inbox" => resolve_inbox(db, input).await,
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
        "cashflow_summary" => cashflow_summary(db, input).await,
        "portfolio_insights" => portfolio_insights(db).await,
        "set_price_alert" => set_price_alert(db, input).await,
        "list_price_alerts" => list_price_alerts(db).await,
        "cancel_price_alert" => cancel_price_alert(db, input).await,
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
    let priority = match str_arg(input, "priority") {
        Some(p) if matches!(p, "high" | "normal" | "low") => Some(p),
        Some(p) => return Err(format!("invalid priority '{p}' — use high/normal/low")),
        None => None,
    };
    let estimate_minutes = match input.get("estimate_minutes") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let minutes = v
                .as_i64()
                .ok_or_else(|| format!("estimate_minutes must be an integer, got {v}"))?;
            if !(1..=MAX_ESTIMATE_MINUTES).contains(&minutes) {
                return Err(format!(
                    "estimate_minutes must be between 1 and {MAX_ESTIMATE_MINUTES}, got {minutes}"
                ));
            }
            Some(minutes)
        }
    };
    let todo = crate::repo::todos::create(
        db,
        title,
        str_arg(input, "notes"),
        due_at.as_deref(),
        priority,
        estimate_minutes,
    )
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
        if let Some(p) = &t.priority {
            if p != "normal" {
                out.push_str(&format!(" [{p}]"));
            }
        }
        if let Some(est) = t.estimate_minutes {
            out.push_str(&format!(" ~{est}m"));
        }
        if let Some(notes) = &t.notes {
            out.push_str(&format!(" — {notes}"));
        }
        out.push('\n');
    }
    Ok(out)
}

async fn plan_day(db: &Db) -> Result<String, String> {
    let plan = crate::assistant::proactive::plan::gather(db, chrono::Utc::now())
        .await
        .map_err(|e| format!("db error: {e}"))?;
    Ok(crate::assistant::proactive::plan::render_plan_block(&plan))
}

async fn rollover_todos(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let ids: Option<Vec<i64>> = match input.get("ids") {
        None | Some(serde_json::Value::Null) => None,
        Some(serde_json::Value::Array(arr)) => Some(
            arr.iter()
                .map(|v| v.as_i64().ok_or_else(|| format!("ids must be integers, got {v}")))
                .collect::<Result<Vec<_>, _>>()?,
        ),
        Some(v) => return Err(format!("ids must be an array of integers, got {v}")),
    };
    let moved = crate::repo::todos::rollover(db, ids.as_deref(), chrono::Utc::now())
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if moved.is_empty() {
        return Ok("nggak ada todo yang perlu digeser".into());
    }
    let mut out = format!("{} todo digeser ke besok:\n", moved.len());
    for t in moved {
        out.push_str(&format!("- #{} {}\n", t.id, t.title));
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

/// Draft an Upwork proposal from a pasted job. Validation only here; the
/// memory + LLM work lives in `assistant::proposal`.
async fn draft_proposal(input: &serde_json::Value) -> Result<String, String> {
    let job_text = str_arg(input, "job_text")
        .ok_or("missing required argument 'job_text' — paste the job description")?;
    let notes = str_arg(input, "notes");
    Ok(super::proposal::draft(job_text, notes).await)
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

/// Upper bound for a todo effort estimate (one week of minutes), mirroring the
/// event reminder ceiling — guards against hallucinated absurd values.
const MAX_ESTIMATE_MINUTES: i64 = 7 * 24 * 60;

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
    let billable = input.get("billable").and_then(|v| v.as_bool());
    let amount = input.get("amount").and_then(|v| v.as_f64());
    let task = crate::clickup::NewTask { name: title.to_string(), due_date_ms, billable, amount };
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

async fn clickup_complete_task(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let task_id = str_arg(input, "task_id").ok_or("missing required argument 'task_id'")?;
    api.complete_task(task_id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("task {task_id} ditandai selesai"))
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

/// Resolve a task name to (task_id, task_name, project_name) by scanning open
/// tasks across all projects. Exact (case-insensitive) matches win; otherwise
/// fall back to substring matches. Errors on no match or ambiguity so the model
/// asks the user instead of guessing.
async fn resolve_clickup_task(
    api: &dyn crate::clickup::ClickUpApi,
    name: &str,
) -> Result<(String, String, String), String> {
    let needle = name.to_lowercase();
    let projects = api.list_projects().await.map_err(|e| format!("{e}"))?;
    let mut exact = Vec::new();
    let mut partial = Vec::new();
    for project in &projects {
        for task in api.list_tasks(&project.id).await.map_err(|e| format!("{e}"))? {
            let hay = task.name.to_lowercase();
            if hay == needle {
                exact.push((task.id.clone(), task.name.clone(), project.name.clone()));
            } else if hay.contains(&needle) {
                partial.push((task.id.clone(), task.name.clone(), project.name.clone()));
            }
        }
    }
    let mut hits = if !exact.is_empty() { exact } else { partial };
    match hits.len() {
        0 => Err(format!("task '{name}' nggak ketemu — sebutin nama task yang ada ya")),
        1 => Ok(hits.remove(0)),
        _ => Err(format!("ada beberapa task yang cocok '{name}' — sebutin lebih spesifik")),
    }
}

async fn clickup_start_timer(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let name = str_arg(input, "task").ok_or("missing required argument 'task'")?;
    let (task_id, task_name, project) = resolve_clickup_task(api, name).await?;
    api.start_timer(&task_id).await.map_err(|e| format!("{e}"))?;
    Ok(format!("timer jalan buat '{task_name}' ({project})"))
}

async fn clickup_stop_timer(api: &dyn crate::clickup::ClickUpApi) -> Result<String, String> {
    match api.stop_timer().await.map_err(|e| format!("{e}"))? {
        Some(entry) => Ok(format!(
            "timer '{}' distop — {}",
            entry.task_name,
            crate::clickup::report::format_duration(entry.duration_ms)
        )),
        None => Ok("nggak ada timer yang jalan".into()),
    }
}

async fn clickup_current_timer(api: &dyn crate::clickup::ClickUpApi) -> Result<String, String> {
    match api.current_timer().await.map_err(|e| format!("{e}"))? {
        Some(running) => Ok(format!("lagi ngerjain '{}'", running.task_name)),
        None => Ok("lagi nggak ada timer yang jalan".into()),
    }
}

async fn clickup_time_report(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let scope = match str_arg(input, "scope") {
        Some(s) if matches!(s, "today" | "week" | "month") => s,
        Some(s) => return Err(format!("scope '{s}' nggak dikenal — pakai today/week/month")),
        None => "week",
    };
    let (start_ms, end_ms) = crate::clickup::report::period_window(scope, chrono::Utc::now());
    let mut entries = api.time_entries(start_ms, end_ms).await.map_err(|e| format!("{e}"))?;
    let project_filter = str_arg(input, "project");
    if let Some(project) = project_filter {
        let needle = project.to_lowercase();
        entries.retain(|e| e.project_name.to_lowercase() == needle);
    }
    let (projects, grand_total) = crate::clickup::report::aggregate_hours(&entries);
    if projects.is_empty() {
        return Ok(match project_filter {
            Some(project) => format!("nggak ada jam tercatat di project '{project}' untuk periode itu"),
            None => "belum ada jam tercatat untuk periode itu".into(),
        });
    }
    let label = match scope { "today" => "Hari ini", "month" => "Bulan ini", _ => "Minggu ini" };
    let mut out = format!("{label}: {}\n", crate::clickup::report::format_duration(grand_total));
    for project in projects {
        out.push_str(&format!("- {}: {}\n", project.project, crate::clickup::report::format_duration(project.total_ms)));
        for (task, ms) in project.tasks {
            out.push_str(&format!("  - {task}: {}\n", crate::clickup::report::format_duration(ms)));
        }
    }
    Ok(out)
}

async fn clickup_add_time_entry(
    api: &dyn crate::clickup::ClickUpApi,
    input: &serde_json::Value,
) -> Result<String, String> {
    let name = str_arg(input, "task").ok_or("missing required argument 'task'")?;
    let raw_duration = str_arg(input, "duration").ok_or("missing required argument 'duration'")?;
    let duration_ms = crate::clickup::report::parse_duration(raw_duration)
        .ok_or_else(|| format!("durasi '{raw_duration}' nggak kebaca — coba '2 jam' atau '90 menit'"))?;
    let start_ms = match str_arg(input, "day") {
        Some(raw) => {
            let dt = parse_tool_datetime(raw)
                .ok_or_else(|| format!("day '{raw}' nggak terbaca — pakai RFC3339 +07:00"))?;
            dt.timestamp_millis()
        }
        None => {
            let (start, _) = crate::clickup::report::period_window("today", chrono::Utc::now());
            start
        }
    };
    let (task_id, task_name, _project) = resolve_clickup_task(api, name).await?;
    api.add_time_entry(&task_id, duration_ms, start_ms).await.map_err(|e| format!("{e}"))?;
    Ok(format!(
        "{} dicatat ke '{task_name}'",
        crate::clickup::report::format_duration(duration_ms)
    ))
}

async fn invoice_list_clients(db: &Db) -> Result<String, String> {
    let clients = crate::repo::clients::list(db).await.map_err(|e| format!("db error: {e}"))?;
    if clients.is_empty() {
        return Ok("belum ada klien tersimpan".into());
    }
    let mut out = String::new();
    for c in clients {
        out.push_str(&format!("#{} {}\n", c.id, c.name));
    }
    Ok(out)
}

fn parse_line_items(input: &serde_json::Value) -> Result<Vec<crate::invoice::assemble::ParsedItem>, String> {
    let arr = input.get("line_items").and_then(|v| v.as_array()).ok_or("line_items harus berupa array")?;
    if arr.is_empty() {
        return Err("line_items kosong".into());
    }
    let mut items = Vec::new();
    for it in arr {
        let title = it.get("title").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()).ok_or("setiap item butuh 'title'")?;
        let amount = it.get("amount").and_then(|v| v.as_i64()).ok_or("setiap item butuh 'amount' (angka IDR)")?;
        // Clamp once here so the rendered PDF and the persisted JSON agree.
        let qty = it.get("qty").and_then(|v| v.as_i64()).unwrap_or(1).max(1);
        let body = it.get("body").and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty()).map(|s| s.to_string());
        items.push(crate::invoice::assemble::ParsedItem { title: title.to_string(), body, qty, amount_idr: amount });
    }
    Ok(items)
}

async fn invoice_create(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let client_name = str_arg(input, "client_name").ok_or("missing required argument 'client_name'")?;
    let items = parse_line_items(input)?;

    let existing = crate::repo::clients::get_by_name(db, client_name)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    // A new client with no details: bail before reading config or writing
    // anything (keeps this path independent of the invoice env vars).
    if existing.is_none() && input.get("client_details").is_none() {
        return Err(format!("klien '{client_name}' belum ada — minta detail klien (sub_name/website) ke user dulu, lalu kirim lewat client_details"));
    }

    // We will create an invoice now, so config is required. Read it BEFORE
    // creating any client row so a config failure never leaves an orphan client.
    let mut config = crate::invoice::config::from_env()?;
    if let Some(days) = input.get("due_days").and_then(|v| v.as_i64()) {
        config.due_days = days;
    }

    let client = match existing {
        Some(c) => c,
        None => {
            let details = input.get("client_details");
            let sub = details.and_then(|d| d.get("sub_name")).and_then(|v| v.as_str());
            let web = details.and_then(|d| d.get("website")).and_then(|v| v.as_str());
            crate::repo::clients::create(db, &crate::repo::clients::NewClient { name: client_name, sub_name: sub, website: web })
                .await.map_err(|e| format!("db error: {e}"))?
        }
    };

    let now = chrono::Utc::now();
    let number = crate::invoice::number::next_number(db, now).await.map_err(|e| format!("db error: {e}"))?;
    let issue_date = now.with_timezone(&crate::assistant::time::wib()).date_naive();
    let due_date = issue_date + chrono::Duration::days(config.due_days);
    let data = crate::invoice::assemble::assemble_invoice_data(number.clone(), issue_date, &config, &client, &items);
    let pdf = crate::invoice::render::render_pdf(&data).map_err(|e| format!("gagal render invoice: {e}"))?;

    let issue_date_iso = issue_date.format("%Y-%m-%d").to_string();
    let due_date_iso = due_date.format("%Y-%m-%d").to_string();
    let line_items_json = serde_json::to_string(
        &items.iter().map(|it| serde_json::json!({ "title": it.title, "body": it.body, "qty": it.qty, "amount": it.amount_idr })).collect::<Vec<_>>()
    ).unwrap_or_else(|_| "[]".into());
    crate::repo::invoices::insert(db, &crate::repo::invoices::NewInvoice {
        number: &number, client_id: client.id,
        issue_date: &issue_date_iso, due_date: &due_date_iso,
        subtotal: &data.subtotal, total: &data.total, line_items_json: &line_items_json,
    }).await.map_err(|e| format!("db error: {e}"))?;

    let sent = send_invoice_pdf(db, &number, pdf).await;
    let suffix = match sent {
        Ok(true) => " dan dikirim ke Telegram".to_string(),
        Ok(false) => " (tersimpan; Telegram belum tertaut, jadi PDF tidak dikirim)".to_string(),
        Err(e) => format!(" (tersimpan, tapi gagal kirim PDF: {e})"),
    };
    Ok(format!("Invoice {number} dibuat — total {}{suffix}", data.total))
}

/// Send the rendered PDF to the linked owner chat. Ok(false) = no link/token.
async fn send_invoice_pdf(db: &Db, number: &str, pdf: Vec<u8>) -> Result<bool, String> {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else { return Ok(false); };
    if token.trim().is_empty() { return Ok(false); }
    let Some(link) = crate::repo::telegram_link::get(db).await.map_err(|e| format!("db error: {e}"))? else { return Ok(false); };
    let client = crate::telegram::client::TelegramClient::new(token);
    let filename = crate::telegram::client::document_filename(number);
    client.send_document(link.chat_id, &filename, pdf, &format!("Invoice {number}")).await.map_err(|e| format!("{e}"))?;
    Ok(true)
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
    // The instrument/account above are auto-detected from the photo, not
    // authoritative — signal that the assistant may correct a wrong one rather
    // than treat the detection as final.
    out.push_str(
        "(instrumen & akun di atas hasil deteksi otomatis dari foto, bukan final — \
         kalau ada yang salah, override dengan account_id/instrument_id yang benar saat confirm_review)\n",
    );
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

async fn create_transaction(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let entry_type = str_arg(input, "entry_type").ok_or("missing required argument 'entry_type'")?;

    // Resolve instrument: id wins, else match by name/symbol.
    let instrument_id = match optional_id(input, "instrument_id")? {
        Some(id) => id,
        None => {
            let name = str_arg(input, "instrument")
                .ok_or("butuh 'instrument' (nama/simbol) atau 'instrument_id'")?;
            crate::ingestion::matching::suggest_instrument_for_entry(db, Some(name), Some(name))
                .await
                .map_err(|e| format!("db error: {e}"))?
                .ok_or_else(|| format!("instrumen '{name}' belum terdaftar — tambah dulu di Web UI → Data"))?
        }
    };
    // Resolve account: id wins, else case-insensitive name match.
    let account_id = match optional_id(input, "account_id")? {
        Some(id) => id,
        None => {
            let name = str_arg(input, "account").ok_or("butuh 'account' (nama) atau 'account_id'")?;
            let accounts = crate::repo::accounts::list(db).await.map_err(|e| format!("db error: {e}"))?;
            accounts.iter().find(|a| a.name.eq_ignore_ascii_case(name)).map(|a| a.id)
                .ok_or_else(|| format!("akun '{name}' nggak ketemu — cek list_accounts"))?
        }
    };

    let ins = crate::repo::instruments::get(db, instrument_id).await
        .map_err(|_| format!("instrumen #{instrument_id} nggak ada"))?;
    crate::repo::accounts::get(db, account_id).await
        .map_err(|_| format!("akun #{account_id} nggak ada"))?;

    let executed_at = match str_arg(input, "executed_at") {
        Some(raw) => crate::ingestion::review::to_rfc3339(raw)
            .ok_or_else(|| format!("tanggal nggak terbaca: {raw}"))?,
        None => chrono::Utc::now().to_rfc3339(),
    };
    let currency = str_arg(input, "currency").unwrap_or("IDR").to_string();
    let mut note = str_arg(input, "note").map(str::to_string);

    let (quantity, price_native) = match crate::service::txn_entry::resolve_qty_price(
        db, &ins, entry_type,
        str_arg(input, "quantity"), str_arg(input, "price_native"), str_arg(input, "amount_native"),
        /* allow_price_one_fallback */ false, &mut note,
    ).await {
        Ok(pair) => pair,
        Err(crate::service::txn_entry::ResolveError::NeedNavOrUnits) =>
            return Err("aku butuh NAV atau jumlah unit-nya dulu buat reksadana ini — kasih salah satu ya".into()),
        Err(crate::service::txn_entry::ResolveError::Other(e)) => return Err(format!("{e}")),
    };

    let usd_idr = crate::repo::prices::latest_fx(db, "USD", "IDR").await
        .map_err(|e| format!("db error: {e}"))?
        .unwrap_or(rust_decimal::Decimal::ONE);
    let fx_to_idr = if currency == "IDR" { "1".to_string() } else { usd_idr.to_string() };

    let nt = crate::repo::transactions::NewTransaction {
        account_id, instrument_id, txn_type: entry_type.to_string(),
        executed_at: chrono::DateTime::parse_from_rfc3339(&executed_at)
            .map_err(|e| format!("tanggal: {e}"))?.with_timezone(&chrono::Utc),
        quantity, price_native, fee_native: str_arg(input, "fee_native").map(str::to_string),
        currency, fx_to_idr, fx_to_usd: "1".to_string(), note, source: Some("chat".into()), external_id: None,
    };
    let txn = crate::repo::transactions::create(db, &nt).await.map_err(|e| format!("{e}"))?;
    Ok(format!("transaksi #{} dicatat: {} {} @ {} di {}", txn.id, txn.txn_type.as_str(), txn.quantity.normalize(), txn.price_native.normalize(), account_id))
}

async fn capture_to_inbox(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let content = str_arg(input, "content").ok_or("missing required argument 'content'")?;
    let row = crate::repo::inbox::create(db, content).await.map_err(|e| format!("db error: {e}"))?;
    Ok(format!("dicatat ke inbox (#{})", row.id))
}

async fn list_inbox(db: &Db) -> Result<String, String> {
    let rows = crate::repo::inbox::list_pending(db).await.map_err(|e| format!("db error: {e}"))?;
    if rows.is_empty() {
        return Ok("inbox kosong".into());
    }
    let mut out = String::new();
    for row in rows {
        out.push_str(&format!("#{} {}\n", row.id, row.content));
    }
    Ok(out)
}

async fn gmail_list_emails(api: &dyn crate::google::gmail::GmailApi, input: &serde_json::Value) -> Result<String, String> {
    let max = input.get("max").and_then(|v| v.as_u64()).unwrap_or(10).min(25) as u32;
    let emails = api.list_important_unread(max).await.map_err(|e| format!("{e}"))?;
    if emails.is_empty() {
        return Ok("nggak ada email penting yang belum dibaca".into());
    }
    let mut out = String::new();
    for e in emails {
        let subject = e.subject.replace('\n', " ");
        let snippet = e.snippet.replace('\n', " ");
        out.push_str(&format!("[{}] {} — {} — {}\n", e.id, e.from, subject, snippet));
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

/// Returns `true` when `m` looks like a valid YYYY-MM month string.
///
/// Accepts exactly 7 characters: four ASCII digits, a hyphen, two ASCII
/// digits — e.g. "2026-06". Does not validate calendar range (that is
/// enforced downstream by the DB query or the service layer).
fn valid_month(m: &str) -> bool {
    m.len() == 7
        && m.as_bytes()[4] == b'-'
        && m[..4].bytes().all(|b| b.is_ascii_digit())
        && m[5..].bytes().all(|b| b.is_ascii_digit())
}

/// Summarise the current (or given) month's cashflow: money in, money out,
/// net, top 3 expense categories, and total freelance invoiced that month.
/// Invoiced amount is shown separately — not added to income — because it
/// represents amounts billed, not necessarily received.
async fn cashflow_summary(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    // Resolve the target month: explicit arg or current WIB month.
    let month: String = match str_arg(input, "month") {
        Some(m) => {
            if !valid_month(m) {
                return Err("format bulan harus YYYY-MM, mis. 2026-06".into());
            }
            m.to_string()
        }
        None => chrono::Utc::now()
            .with_timezone(&super::time::wib())
            .format("%Y-%m")
            .to_string(),
    };

    // Load cashflow rows for the month.
    let cashflow_rows = crate::repo::cashflow::list_for_month(db, &month)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    // Load cashflow categories (income/expense, with optional monthly budget).
    let category_rows = crate::repo::cashflow_categories::list(db)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    // Map CashflowRow → CfRow (the service layer struct).
    // Use unwrap_or(ZERO) so a malformed amount string never silently drops a
    // row from the totals — consistent with how portfolio_insights handles it.
    let cf_rows: Vec<crate::service::cashflow::CfRow> = cashflow_rows
        .iter()
        .map(|row| crate::service::cashflow::CfRow {
            direction: row.direction.clone(),
            amount: crate::repo::dec(&row.amount).unwrap_or(rust_decimal::Decimal::ZERO),
            category_id: row.category_id,
        })
        .collect();

    // Map CashflowCategoryRow → CatRow with the real kind and optional budget.
    let cat_rows: Vec<crate::service::cashflow::CatRow> = category_rows
        .iter()
        .map(|cat| crate::service::cashflow::CatRow {
            id: cat.id,
            name: cat.name.clone(),
            kind: cat.kind.clone(),
            budget: cat.monthly_budget.as_deref().and_then(|s| s.parse::<rust_decimal::Decimal>().ok()),
        })
        .collect();

    let summary = crate::service::cashflow::month_summary(&month, &cf_rows, &cat_rows);

    // Sum invoices issued in this month.
    let all_invoices = crate::repo::invoices::list_all(db)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    let invoiced_total: rust_decimal::Decimal = all_invoices
        .iter()
        .filter(|inv| inv.issue_date.starts_with(&month))
        .filter_map(|inv| crate::repo::dec(&inv.total).ok())
        .fold(rust_decimal::Decimal::ZERO, |acc, amount| acc + amount);

    // Render the summary using Indonesian number formatting.
    let format_amount = |d: &rust_decimal::Decimal| {
        crate::service::chat::group_id(&d.round_dp(0))
    };

    let mut output = format!(
        "Bulan {month}: masuk Rp {}, kepake Rp {}, net Rp {}\n",
        format_amount(&summary.total_in),
        format_amount(&summary.total_out),
        format_amount(&summary.net),
    );

    // Top 3 expense categories sorted by actual spending (descending).
    // Categories with zero actual spend are excluded so an empty category
    // never produces a misleading "- Health: Rp 0" line.
    let mut expense_categories: Vec<&crate::service::cashflow::CategoryLine> = summary
        .categories
        .iter()
        .filter(|cat| cat.kind == "expense" && cat.actual > rust_decimal::Decimal::ZERO)
        .collect();
    expense_categories.sort_by(|a, b| b.actual.cmp(&a.actual));
    for category in expense_categories.iter().take(3) {
        output.push_str(&format!(
            "- {}: Rp {}\n",
            category.name,
            format_amount(&category.actual),
        ));
    }

    // Show freelance invoiced total only when it is non-zero.
    if invoiced_total > rust_decimal::Decimal::ZERO {
        output.push_str(&format!(
            "Freelance diinvoice: Rp {}\n",
            format_amount(&invoiced_total),
        ));
    }

    Ok(output)
}

/// Compute a portfolio health snapshot: net worth in IDR, biggest position
/// concentration as a percentage of net worth, and the current month's savings
/// rate derived from cashflow rows.
///
/// Returns at minimum the net-worth line so an empty database still produces a
/// non-empty, human-readable result.
async fn portfolio_insights(db: &Db) -> Result<String, String> {
    let summary = crate::service::portfolio::build_summary(db)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    let net = summary.net_worth_idr;
    let mut output = format!(
        "Net worth: Rp {}\n",
        crate::service::chat::group_id(&net.round_dp(0))
    );

    // Build (symbol, market_value_idr) pairs using the instrument list for
    // symbol resolution — Position only carries instrument_id.
    let instruments = crate::repo::instruments::list(db)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    let instrument_symbol_map: std::collections::HashMap<i64, String> = instruments
        .iter()
        .map(|i| (i.id, i.symbol.clone()))
        .collect();

    let symbol_values: Vec<(String, rust_decimal::Decimal)> = summary
        .positions
        .iter()
        .filter_map(|p| {
            instrument_symbol_map
                .get(&p.instrument_id)
                .map(|symbol| (symbol.clone(), p.market_value_idr))
        })
        .collect();

    // Only show concentration when net worth is non-zero — concentration() can
    // return Some with pct=0 when net worth is zero, which would print a
    // misleading "X (0%)" line on an empty/zero portfolio.
    if !net.is_zero() {
        if let Some(concentration) =
            crate::service::insights::concentration(&symbol_values, net)
        {
            output.push_str(&format!(
                "Konsentrasi terbesar: {} ({}%)\n",
                concentration.symbol,
                concentration.pct.round_dp(0),
            ));
        }
    }

    // Savings rate from cashflow rows for the current WIB month.
    let current_month = chrono::Utc::now()
        .with_timezone(&crate::assistant::time::wib())
        .format("%Y-%m")
        .to_string();

    let cashflow_rows = crate::repo::cashflow::list_for_month(db, &current_month)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    let mut total_income = rust_decimal::Decimal::ZERO;
    let mut total_expense = rust_decimal::Decimal::ZERO;
    for row in &cashflow_rows {
        let amount = crate::repo::dec(&row.amount).unwrap_or(rust_decimal::Decimal::ZERO);
        if row.direction == "in" {
            total_income += amount;
        } else if row.direction == "out" {
            total_expense += amount;
        }
    }
    if total_income > rust_decimal::Decimal::ZERO {
        let savings_rate =
            crate::service::insights::savings_rate(total_income, total_expense);
        output.push_str(&format!(
            "Savings rate bulan ini: {}%\n",
            savings_rate.round_dp(0),
        ));
    }

    Ok(output)
}

/// Resolve an instrument by name/symbol (case-insensitive).
///
/// Returns `(id, symbol)` if found, or an error message in Indonesian.
async fn resolve_instrument(db: &Db, name: &str) -> Result<(i64, String), String> {
    let instruments =
        crate::repo::instruments::list(db).await.map_err(|e| format!("db error: {e}"))?;
    let matched = instruments.iter().find(|i| {
        i.symbol.eq_ignore_ascii_case(name) || i.name.eq_ignore_ascii_case(name)
    });
    match matched {
        Some(i) => Ok((i.id, i.symbol.clone())),
        None => Err(format!("instrumen '{name}' nggak ketemu")),
    }
}

/// Set a price alert on an instrument.
///
/// Accepts either an absolute `target` price or a `percent` offset from the
/// current price (requires a `direction` of "above" or "below").
async fn set_price_alert(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let instrument_name =
        str_arg(input, "instrument").ok_or("missing required argument 'instrument'")?;
    let (instrument_id, symbol) = resolve_instrument(db, instrument_name).await?;

    // Resolve direction: explicit arg wins; otherwise error — we never silently default.
    let direction = match str_arg(input, "direction") {
        Some(d) if d == "above" || d == "below" => d.to_string(),
        Some(other) => {
            return Err(format!(
                "direction '{other}' tidak valid — gunakan 'above' atau 'below'"
            ))
        }
        None => {
            return Err(
                "direction wajib diisi — gunakan 'above' (naik) atau 'below' (turun)".into(),
            )
        }
    };

    // Compute target price.
    let target: rust_decimal::Decimal = if let Some(target_val) = input.get("target").and_then(|v| v.as_f64()) {
        rust_decimal::Decimal::try_from(target_val)
            .map_err(|_| format!("target '{target_val}' tidak valid"))?
    } else if let Some(pct_val) = input.get("percent").and_then(|v| v.as_f64()) {
        let current_price = match crate::repo::prices::latest(db, instrument_id)
            .await
            .map_err(|e| format!("db error: {e}"))?
        {
            Some(p) => p.price,
            None => {
                return Err(format!(
                    "harga {symbol} belum ada, nggak bisa hitung dari persen"
                ))
            }
        };
        let pct = rust_decimal::Decimal::try_from(pct_val)
            .map_err(|_| format!("percent '{pct_val}' tidak valid"))?;
        let hundred = rust_decimal::Decimal::from(100u32);
        match direction.as_str() {
            "below" => current_price * (rust_decimal::Decimal::ONE - pct / hundred),
            "above" => current_price * (rust_decimal::Decimal::ONE + pct / hundred),
            _ => unreachable!(),
        }
    } else {
        return Err("butuh 'target' (harga absolut) atau 'percent' (persen dari harga sekarang)".into());
    };

    crate::repo::price_alerts::create(db, instrument_id, &target.to_string(), &direction)
        .await
        .map_err(|e| format!("db error: {e}"))?;

    Ok(format!(
        "alert dipasang: {symbol} {direction} Rp {}",
        crate::service::chat::group_id(&target.round_dp(0))
    ))
}

/// List all active price alerts, resolved to their instrument symbols.
async fn list_price_alerts(db: &Db) -> Result<String, String> {
    let alerts =
        crate::repo::price_alerts::list_active(db).await.map_err(|e| format!("db error: {e}"))?;
    if alerts.is_empty() {
        return Ok("belum ada price alert".into());
    }
    let mut out = String::new();
    for alert in alerts {
        let symbol = crate::repo::instruments::get(db, alert.instrument_id)
            .await
            .ok()
            .map(|i| i.symbol)
            .unwrap_or_else(|| format!("#{}", alert.instrument_id));
        let target_display = alert
            .target_price
            .parse::<rust_decimal::Decimal>()
            .map(|d| crate::service::chat::group_id(&d.round_dp(0)))
            .unwrap_or_else(|_| alert.target_price.clone());
        out.push_str(&format!(
            "[{}] {} {} Rp {}\n",
            alert.id, symbol, alert.direction, target_display
        ));
    }
    Ok(out)
}

/// Cancel an active price alert by id.
async fn cancel_price_alert(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let alert_id = id_arg(input, "id")?;
    let cancelled = crate::repo::price_alerts::cancel(db, alert_id)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if cancelled {
        Ok(format!("price alert #{alert_id} dibatalkan"))
    } else {
        Err(format!("price alert #{alert_id} nggak ada atau sudah tidak aktif"))
    }
}

async fn resolve_inbox(db: &Db, input: &serde_json::Value) -> Result<String, String> {
    let status = str_arg(input, "status").ok_or("missing required argument 'status'")?;
    if !matches!(status, "sorted" | "dropped") {
        return Err(format!("invalid status '{status}' — use sorted/dropped"));
    }
    let ids: Vec<i64> = match input.get("ids") {
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .map(|v| v.as_i64().ok_or_else(|| format!("ids must be integers, got {v}")))
            .collect::<Result<Vec<_>, _>>()?,
        _ => return Err("missing required argument 'ids' (array of integers)".into()),
    };
    let affected = crate::repo::inbox::resolve(db, &ids, status).await.map_err(|e| format!("db error: {e}"))?;
    Ok(format!("{affected} item inbox ditandai {status}"))
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
        crate::repo::todos::create(&db, "beli kado", None, None, None, None).await.unwrap();
        let out = dispatch(&db, "list_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("beli kado"), "{out}");
    }

    #[tokio::test]
    async fn list_todos_renders_priority_and_estimate() {
        let db = mem_db().await;
        crate::repo::todos::create(&db, "deck", None, None, Some("high"), Some(45)).await.unwrap();
        crate::repo::todos::create(&db, "santai", None, None, Some("normal"), None).await.unwrap();
        let out = dispatch(&db, "list_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("[high]"), "{out}");
        assert!(out.contains("~45m"), "{out}");
        assert!(!out.contains("[normal]"), "{out}");
    }

    #[tokio::test]
    async fn complete_todo_round_trips_and_errors_when_done() {
        let db = mem_db().await;
        let todo = crate::repo::todos::create(&db, "x", None, None, None, None).await.unwrap();
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

    /// The account was already auto-detected (wrongly) as Stockbit at ingest;
    /// passing the right account must override it, not be refused because the
    /// item isn't 'belum dikenali'. Locks the mechanism the affordance relies on.
    #[tokio::test]
    async fn confirm_review_overrides_an_already_detected_wrong_account() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "VXUS".into(), name: "Vanguard Total Intl".into(), instrument_type: "etf".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        let stockbit = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Stockbit".into(), account_type: "broker".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let nanovest = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Nanovest".into(), account_type: "broker".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        // OCR read "Stockbit" → account resolved to the wrong one at ingest.
        let id = seed_pending_item(&db, Some(stockbit.id), Some(instrument.id)).await;

        let out = dispatch(&db, "confirm_review", &serde_json::json!({
            "review_id": id, "account_id": nanovest.id
        })).await.unwrap();
        assert!(out.contains("dibuat"), "{out}");

        let txns = crate::repo::transactions::list_all(&db).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].account_id, nanovest.id, "override must switch Stockbit -> Nanovest");
        let item = crate::repo::review_items::get(&db, id).await.unwrap();
        assert_eq!(item.suggested_account_id, Some(nanovest.id));
    }

    /// list_pending_reviews must hint that the shown account/instrument is an OCR
    /// guess the assistant can correct — otherwise a confidently-wrong account
    /// looks authoritative and the assistant won't override it.
    #[tokio::test]
    async fn list_pending_reviews_notes_detected_values_are_correctable() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let instrument = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "VXUS".into(), name: "Vanguard Total Intl".into(), instrument_type: "etf".into(),
            native_currency: "USD".into(), category_id: None, price_source: "manual".into(),
            decimals: Some(8), note: None,
        }).await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Stockbit".into(), account_type: "broker".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        seed_pending_item(&db, Some(acc.id), Some(instrument.id)).await;

        let out = dispatch(&db, "list_pending_reviews", &serde_json::json!({}))
            .await
            .unwrap()
            .to_lowercase();
        assert!(
            out.contains("override"),
            "listing should hint detected account/instrument is correctable: {out}"
        );
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
    async fn create_todo_stores_priority_and_estimate() {
        let db = mem_db().await;
        dispatch(&db, "create_todo", &serde_json::json!({
            "title": "siapin deck",
            "priority": "high",
            "estimate_minutes": 45
        })).await.unwrap();
        let todos = crate::repo::todos::list_open(&db).await.unwrap();
        assert_eq!(todos[0].priority.as_deref(), Some("high"));
        assert_eq!(todos[0].estimate_minutes, Some(45));
    }

    #[tokio::test]
    async fn create_todo_rejects_bad_priority() {
        let db = mem_db().await;
        let err = dispatch(&db, "create_todo", &serde_json::json!({
            "title": "x", "priority": "urgent"
        })).await.unwrap_err();
        assert!(err.contains("priority"), "{err}");
        assert!(err.contains("high/normal/low"), "{err}");
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

    use crate::clickup::client::{ClickUpApi, ClickUpError, NewTask, Project, RunningEntry, Task, TimeEntry};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeClickUp {
        projects: Mutex<Vec<Project>>,
        created_tasks: Mutex<Vec<(String, String)>>, // (list_id, title)
        created_dues: Mutex<Vec<Option<i64>>>,       // due_date_ms per created task
        created_billables: Mutex<Vec<Option<bool>>>,
        created_amounts: Mutex<Vec<Option<f64>>>,
        tasks: Mutex<std::collections::HashMap<String, Vec<crate::clickup::client::Task>>>,
        completed: Mutex<Vec<String>>,
        running: Mutex<Option<RunningEntry>>,
        entries: Mutex<Vec<TimeEntry>>,
        started: Mutex<Vec<String>>,       // task_ids passed to start_timer
        stopped: Mutex<u32>,
        added: Mutex<Vec<(String, i64)>>,  // (task_id, duration_ms)
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
            self.created_billables.lock().unwrap().push(task.billable);
            self.created_amounts.lock().unwrap().push(task.amount);
            Ok(format!("task_{}", task.name))
        }
        async fn list_tasks(&self, list_id: &str) -> Result<Vec<crate::clickup::client::Task>, ClickUpError> {
            Ok(self.tasks.lock().unwrap().get(list_id).cloned().unwrap_or_default())
        }
        async fn complete_task(&self, task_id: &str) -> Result<(), ClickUpError> {
            self.completed.lock().unwrap().push(task_id.to_string());
            // Mirror ClickUp: a completed task drops out of the open-task views.
            for tasks in self.tasks.lock().unwrap().values_mut() {
                tasks.retain(|t| t.id != task_id);
            }
            Ok(())
        }
        async fn start_timer(&self, task_id: &str) -> Result<(), ClickUpError> {
            self.started.lock().unwrap().push(task_id.to_string());
            *self.running.lock().unwrap() = Some(RunningEntry {
                task_name: task_id.to_string(),
                started_ms: 0,
            });
            Ok(())
        }
        async fn stop_timer(&self) -> Result<Option<TimeEntry>, ClickUpError> {
            *self.stopped.lock().unwrap() += 1;
            let running = self.running.lock().unwrap().take();
            Ok(running.map(|r| TimeEntry {
                task_id: r.task_name.clone(),
                task_name: r.task_name,
                project_name: "(test)".into(),
                duration_ms: 3_600_000,
                start_ms: 0,
                billable: false,
            }))
        }
        async fn current_timer(&self) -> Result<Option<RunningEntry>, ClickUpError> {
            Ok(self.running.lock().unwrap().clone())
        }
        async fn time_entries(&self, _start_ms: i64, _end_ms: i64) -> Result<Vec<TimeEntry>, ClickUpError> {
            Ok(self.entries.lock().unwrap().clone())
        }
        async fn add_time_entry(&self, task_id: &str, duration_ms: i64, _start_ms: i64) -> Result<(), ClickUpError> {
            self.added.lock().unwrap().push((task_id.to_string(), duration_ms));
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
    async fn complete_task_marks_done() {
        let fake = FakeClickUp::default();
        let out = clickup_complete_task(&fake, &serde_json::json!({ "task_id": "t1" })).await.unwrap();
        assert!(out.contains("selesai"), "{out}");
        assert_eq!(fake.completed.lock().unwrap().as_slice(), &["t1".to_string()]);
    }

    #[tokio::test]
    async fn complete_task_requires_id() {
        let fake = FakeClickUp::default();
        let err = clickup_complete_task(&fake, &serde_json::json!({})).await.unwrap_err();
        assert!(err.contains("task_id"), "{err}");
    }

    #[tokio::test]
    async fn completing_a_task_removes_it_from_list_tasks() {
        // Headline cross-phase invariant: list → complete → no longer listed.
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![
            Task { id: "t1".into(), name: "bikin kontrak".into(), status: "to do".into(), due_date_ms: None },
        ]);
        let before = clickup_list_tasks(&fake, &serde_json::json!({ "project": "PT AIS" })).await.unwrap();
        assert!(before.contains("bikin kontrak"), "{before}");

        clickup_complete_task(&fake, &serde_json::json!({ "task_id": "t1" })).await.unwrap();

        let after = clickup_list_tasks(&fake, &serde_json::json!({ "project": "PT AIS" })).await.unwrap();
        assert!(!after.contains("bikin kontrak"), "completed task still listed: {after}");
    }

    #[tokio::test]
    async fn create_task_passes_billable_and_amount() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        clickup_create_task(&fake, &serde_json::json!({
            "project": "PT AIS", "title": "landing page", "billable": true, "amount": 10000000
        })).await.unwrap();
        assert_eq!(fake.created_billables.lock().unwrap()[0], Some(true));
        assert_eq!(fake.created_amounts.lock().unwrap()[0], Some(10_000_000.0));
    }

    #[tokio::test]
    async fn create_task_without_billable_is_none() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        clickup_create_task(&fake, &serde_json::json!({ "project": "PT AIS", "title": "x" })).await.unwrap();
        assert_eq!(fake.created_billables.lock().unwrap()[0], None);
        assert_eq!(fake.created_amounts.lock().unwrap()[0], None);
    }

    #[tokio::test]
    async fn rollover_todos_default_moves_overdue_and_reports() {
        let db = mem_db().await;
        let yesterday = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        crate::repo::todos::create(&db, "kelar besok", None, Some(&yesterday), None, None).await.unwrap();
        let out = dispatch(&db, "rollover_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("kelar besok"), "{out}");
        assert!(out.contains("digeser"), "{out}");
    }

    #[tokio::test]
    async fn rollover_todos_reports_when_nothing_to_move() {
        let db = mem_db().await;
        let out = dispatch(&db, "rollover_todos", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("nggak ada"), "{out}");
    }

    #[tokio::test]
    async fn rollover_todos_rejects_malformed_ids() {
        let db = mem_db().await;
        let scalar = dispatch(&db, "rollover_todos", &serde_json::json!({ "ids": 5 })).await.unwrap_err();
        assert!(scalar.contains("ids"), "{scalar}");
        let bad_elem = dispatch(&db, "rollover_todos", &serde_json::json!({ "ids": ["x"] })).await.unwrap_err();
        assert!(bad_elem.contains("ids"), "{bad_elem}");
    }

    #[tokio::test]
    async fn plan_day_returns_block_with_ordered_todos() {
        let db = mem_db().await;
        crate::repo::todos::create(&db, "kerja low", None, None, Some("low"), None).await.unwrap();
        crate::repo::todos::create(&db, "kerja high", None, None, Some("high"), None).await.unwrap();
        let out = dispatch(&db, "plan_day", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Rencana hari"), "{out}");
        let hi = out.find("kerja high").unwrap();
        let lo = out.find("kerja low").unwrap();
        assert!(hi < lo, "{out}");
    }

    #[tokio::test]
    async fn draft_proposal_requires_job_text() {
        let err = super::draft_proposal(&serde_json::json!({})).await;
        assert!(err.is_err(), "missing job_text must error");
    }

    #[tokio::test]
    async fn list_clients_lists_saved() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::repo::clients::create(&db, &crate::repo::clients::NewClient { name: "PT AIS", sub_name: None, website: None }).await.unwrap();
        let out = invoice_list_clients(&db).await.unwrap();
        assert!(out.contains("PT AIS"), "{out}");
    }

    #[tokio::test]
    async fn create_invoice_persists_and_reports_number() {
        std::env::set_var("INVOICE_ISSUER_NAME", "Bima");
        std::env::set_var("INVOICE_ACCOUNT_NO", "123");
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        crate::repo::clients::create(&db, &crate::repo::clients::NewClient { name: "PT AIS", sub_name: None, website: None }).await.unwrap();
        let out = invoice_create(&db, &serde_json::json!({
            "client_name": "PT AIS",
            "line_items": [{ "title": "Landing page", "amount": 10000000 }]
        })).await.unwrap();
        std::env::remove_var("INVOICE_ISSUER_NAME");
        std::env::remove_var("INVOICE_ACCOUNT_NO");
        assert!(out.contains("INV/"), "should report the invoice number: {out}");
        let seq = crate::repo::invoices::max_seq_for_prefix(&db, "INV/").await.unwrap();
        assert!(seq.is_some(), "invoice not persisted");
    }

    #[tokio::test]
    async fn create_invoice_unknown_client_asks_for_details() {
        // No env vars needed — the unknown-client check short-circuits before
        // config is read, so this test is immune to env-var race conditions.
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let err = invoice_create(&db, &serde_json::json!({
            "client_name": "Klien Baru",
            "line_items": [{ "title": "x", "amount": 1000 }]
        })).await.unwrap_err();
        assert!(err.contains("Klien Baru"), "{err}");
        assert!(err.contains("detail") || err.contains("belum ada"), "{err}");
    }

    #[tokio::test]
    async fn create_invoice_new_client_with_details_creates_both() {
        std::env::set_var("INVOICE_ISSUER_NAME", "Bima");
        std::env::set_var("INVOICE_ACCOUNT_NO", "123");
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let out = invoice_create(&db, &serde_json::json!({
            "client_name": "Klien Baru",
            "client_details": { "sub_name": "PT Baru", "website": "baru.id" },
            "line_items": [{ "title": "Konsultasi", "amount": 5_000_000 }],
            "due_days": 30
        })).await.unwrap();
        std::env::remove_var("INVOICE_ISSUER_NAME");
        std::env::remove_var("INVOICE_ACCOUNT_NO");
        assert!(out.contains("INV/"), "{out}");
        // Both the client and the invoice were persisted.
        let client = crate::repo::clients::get_by_name(&db, "Klien Baru").await.unwrap();
        assert!(client.is_some(), "client not created");
        assert!(crate::repo::invoices::max_seq_for_prefix(&db, "INV/").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn start_timer_resolves_task_and_starts() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![Task {
            id: "t9".into(), name: "landing page".into(), status: "open".into(), due_date_ms: None,
        }]);
        let out = clickup_start_timer(&fake, &serde_json::json!({ "task": "landing page" })).await.unwrap();
        assert!(out.to_lowercase().contains("landing page"), "{out}");
        assert_eq!(fake.started.lock().unwrap().as_slice(), &["t9".to_string()]);
    }

    #[tokio::test]
    async fn start_timer_unknown_task_errors() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        let err = clickup_start_timer(&fake, &serde_json::json!({ "task": "ghost" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("ghost") || err.to_lowercase().contains("ketemu"), "{err}");
    }

    #[tokio::test]
    async fn stop_timer_running_then_none() {
        let fake = FakeClickUp::default();
        *fake.running.lock().unwrap() = Some(RunningEntry { task_name: "landing".into(), started_ms: 0 });
        let out = clickup_stop_timer(&fake).await.unwrap();
        assert!(out.to_lowercase().contains("landing"), "{out}");
        let out2 = clickup_stop_timer(&fake).await.unwrap();
        assert!(out2.to_lowercase().contains("nggak ada"), "{out2}");
    }

    #[tokio::test]
    async fn current_timer_reports_running_or_idle() {
        let fake = FakeClickUp::default();
        assert!(clickup_current_timer(&fake).await.unwrap().to_lowercase().contains("nggak ada"));
        *fake.running.lock().unwrap() = Some(RunningEntry { task_name: "kontrak".into(), started_ms: 0 });
        assert!(clickup_current_timer(&fake).await.unwrap().to_lowercase().contains("kontrak"));
    }

    #[tokio::test]
    async fn time_report_aggregates_entries() {
        let fake = FakeClickUp::default();
        fake.entries.lock().unwrap().extend([
            TimeEntry { task_id: "t1".into(), task_name: "landing".into(), project_name: "PT AIS".into(), duration_ms: 4 * 3_600_000, start_ms: 0, billable: false },
            TimeEntry { task_id: "t2".into(), task_name: "kontrak".into(), project_name: "PT AIS".into(), duration_ms: 2 * 3_600_000, start_ms: 0, billable: false },
        ]);
        let out = clickup_time_report(&fake, &serde_json::json!({ "scope": "week" })).await.unwrap();
        assert!(out.contains("PT AIS"), "{out}");
        assert!(out.contains("landing"), "{out}");
        assert!(out.contains("6j"), "{out}"); // project total 6j
    }

    #[tokio::test]
    async fn time_report_empty_is_explicit() {
        let fake = FakeClickUp::default();
        let out = clickup_time_report(&fake, &serde_json::json!({ "scope": "week" })).await.unwrap();
        assert!(out.to_lowercase().contains("belum ada"), "{out}");
    }

    #[tokio::test]
    async fn start_timer_ambiguous_task_errors() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![
            Task { id: "t1".into(), name: "revisi desain".into(), status: "open".into(), due_date_ms: None },
            Task { id: "t2".into(), name: "revisi kontrak".into(), status: "open".into(), due_date_ms: None },
        ]);
        // "revisi" substring-matches both → ambiguous.
        let err = clickup_start_timer(&fake, &serde_json::json!({ "task": "revisi" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("beberapa") || err.to_lowercase().contains("spesifik"), "{err}");
    }

    #[tokio::test]
    async fn time_report_project_filter_miss_is_project_specific() {
        let fake = FakeClickUp::default();
        fake.entries.lock().unwrap().push(TimeEntry {
            task_id: "t1".into(), task_name: "landing".into(), project_name: "PT AIS".into(),
            duration_ms: 3_600_000, start_ms: 0, billable: false,
        });
        let out = clickup_time_report(&fake, &serde_json::json!({ "scope": "week", "project": "Klien B" })).await.unwrap();
        assert!(out.contains("Klien B"), "{out}");
    }

    #[tokio::test]
    async fn add_time_entry_parses_duration_and_records() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![Task {
            id: "t9".into(), name: "kontrak".into(), status: "open".into(), due_date_ms: None,
        }]);
        let out = clickup_add_time_entry(&fake, &serde_json::json!({ "task": "kontrak", "duration": "2 jam" })).await.unwrap();
        assert!(out.to_lowercase().contains("kontrak"), "{out}");
        let added = fake.added.lock().unwrap();
        assert_eq!(added.as_slice(), &[("t9".to_string(), 7_200_000i64)]);
    }

    #[tokio::test]
    async fn add_time_entry_bad_duration_errors() {
        let fake = FakeClickUp::default();
        fake.projects.lock().unwrap().push(Project { id: "l1".into(), name: "PT AIS".into() });
        fake.tasks.lock().unwrap().insert("l1".into(), vec![Task {
            id: "t9".into(), name: "kontrak".into(), status: "open".into(), due_date_ms: None,
        }]);
        let err = clickup_add_time_entry(&fake, &serde_json::json!({ "task": "kontrak", "duration": "kapan-kapan" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("durasi"), "{err}");
    }

    #[tokio::test]
    async fn capture_to_inbox_stores_and_lists() {
        let db = mem_db().await;
        let out = dispatch(&db, "capture_to_inbox", &serde_json::json!({ "content": "beli kado" })).await.unwrap();
        assert!(out.to_lowercase().contains("inbox"), "{out}");
        let listed = dispatch(&db, "list_inbox", &serde_json::json!({})).await.unwrap();
        assert!(listed.contains("beli kado"), "{listed}");
    }

    #[tokio::test]
    async fn list_inbox_empty_is_explicit() {
        let db = mem_db().await;
        let out = dispatch(&db, "list_inbox", &serde_json::json!({})).await.unwrap();
        assert!(out.to_lowercase().contains("kosong"), "{out}");
    }

    #[tokio::test]
    async fn resolve_inbox_marks_sorted_and_rejects_bad_status() {
        let db = mem_db().await;
        let row = crate::repo::inbox::create(&db, "x").await.unwrap();
        let out = dispatch(&db, "resolve_inbox", &serde_json::json!({ "ids": [row.id], "status": "sorted" })).await.unwrap();
        assert!(out.contains("1"), "{out}");
        assert!(crate::repo::inbox::list_pending(&db).await.unwrap().is_empty());
        let err = dispatch(&db, "resolve_inbox", &serde_json::json!({ "ids": [row.id], "status": "nonsense" })).await.unwrap_err();
        assert!(err.to_lowercase().contains("status"), "{err}");
    }

    // ── Gmail fake ────────────────────────────────────────────────────────────

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

    #[tokio::test]
    async fn portfolio_insights_runs_on_empty_db() {
        let db = mem_db().await;
        let out = dispatch(&db, "portfolio_insights", &serde_json::json!({})).await.unwrap();
        assert!(!out.is_empty(), "{out}");
        assert!(out.to_lowercase().contains("net worth"), "{out}");
    }

    #[tokio::test]
    async fn cashflow_summary_reports_in_out_net() {
        let db = mem_db().await;
        let month = chrono::Utc::now().with_timezone(&crate::assistant::time::wib()).format("%Y-%m").to_string();
        // Create a real cashflow expense category so the 'out' row can be tied to it.
        let cat = crate::repo::cashflow_categories::create(
            &db,
            &crate::repo::cashflow_categories::NewCashflowCategory {
                name: "Makan".into(),
                kind: "expense".into(),
                monthly_budget: None,
                color: None,
            },
        ).await.unwrap();
        // Insert one 'in' 1_000_000 cashflow row dated within `month`.
        crate::repo::cashflow::create(&db, &crate::repo::cashflow::NewCashflow {
            account_id: None,
            occurred_on: format!("{month}-15"),
            direction: "in".into(),
            amount: "1000000".into(),
            currency: "IDR".into(),
            category_id: None,
            note: None,
        }).await.unwrap();
        // Insert one 'out' 400_000 row tied to the "Makan" category.
        crate::repo::cashflow::create(&db, &crate::repo::cashflow::NewCashflow {
            account_id: None,
            occurred_on: format!("{month}-15"),
            direction: "out".into(),
            amount: "400000".into(),
            currency: "IDR".into(),
            category_id: Some(cat.id),
            note: None,
        }).await.unwrap();
        let out = dispatch(&db, "cashflow_summary", &serde_json::json!({})).await.unwrap();
        assert!(out.to_lowercase().contains("masuk"), "{out}");
        assert!(out.to_lowercase().contains("net"), "{out}");
        assert!(out.contains("1.000.000") || out.contains("600.000"), "{out}"); // in or net
        assert!(out.contains("Makan"), "category name must appear in expense breakdown: {out}");
    }

    /// cashflow_summary must reject a month arg that is not YYYY-MM format.
    #[tokio::test]
    async fn cashflow_summary_rejects_bad_month_format() {
        let db = mem_db().await;
        let err = dispatch(&db, "cashflow_summary", &serde_json::json!({ "month": "2026-1" }))
            .await
            .unwrap_err();
        assert!(
            err.to_lowercase().contains("yyyy-mm") || err.to_lowercase().contains("format bulan"),
            "error must mention expected format: {err}"
        );
    }

    /// cashflow_summary must not emit a "Rp 0" line for a category that had
    /// no activity this month.
    #[tokio::test]
    async fn cashflow_summary_omits_zero_spend_categories() {
        let db = mem_db().await;
        let month = chrono::Utc::now()
            .with_timezone(&crate::assistant::time::wib())
            .format("%Y-%m")
            .to_string();

        // "Makan" — has actual spend; "Kesehatan" — zero activity this month.
        let cat_makan = crate::repo::cashflow_categories::create(
            &db,
            &crate::repo::cashflow_categories::NewCashflowCategory {
                name: "Makan".into(),
                kind: "expense".into(),
                monthly_budget: None,
                color: None,
            },
        )
        .await
        .unwrap();
        crate::repo::cashflow_categories::create(
            &db,
            &crate::repo::cashflow_categories::NewCashflowCategory {
                name: "Kesehatan".into(),
                kind: "expense".into(),
                monthly_budget: None,
                color: None,
            },
        )
        .await
        .unwrap();

        crate::repo::cashflow::create(&db, &crate::repo::cashflow::NewCashflow {
            account_id: None,
            occurred_on: format!("{month}-10"),
            direction: "out".into(),
            amount: "200000".into(),
            currency: "IDR".into(),
            category_id: Some(cat_makan.id),
            note: None,
        })
        .await
        .unwrap();

        let out = dispatch(&db, "cashflow_summary", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Makan"), "active category must appear: {out}");
        assert!(
            !out.contains("Kesehatan"),
            "zero-spend category must not appear in top expenses: {out}"
        );
    }

    /// portfolio_insights on an empty DB must show net worth and must NOT show
    /// a concentration line (net worth is zero so concentration is suppressed).
    #[tokio::test]
    async fn portfolio_insights_empty_db_no_concentration_line() {
        let db = mem_db().await;
        let out = dispatch(&db, "portfolio_insights", &serde_json::json!({})).await.unwrap();
        assert!(out.to_lowercase().contains("net worth"), "net worth line required: {out}");
        assert!(
            !out.to_lowercase().contains("konsentrasi"),
            "concentration line must be absent when net worth is zero: {out}"
        );
        assert!(
            !out.to_lowercase().contains("savings rate"),
            "savings rate line must be absent when no cashflow rows: {out}"
        );
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

    // ── Price alert helpers ───────────────────────────────────────────────────

    /// Insert a minimal instrument and return it (id + symbol).
    async fn seed_instrument(db: &Db, symbol: &str) -> crate::repo::instruments::InstrumentRow {
        crate::repo::instruments::create(
            db,
            &crate::repo::instruments::NewInstrument {
                symbol: symbol.to_string(),
                name: format!("{symbol} Corp"),
                instrument_type: "stock".into(),
                native_currency: "IDR".into(),
                category_id: None,
                price_source: "manual".into(),
                decimals: Some(0),
                note: None,
            },
        )
        .await
        .unwrap()
    }

    // ── Price alert dispatcher tests ──────────────────────────────────────────

    #[tokio::test]
    async fn set_price_alert_with_absolute_target_stores_active_alert() {
        let db = mem_db().await;
        seed_instrument(&db, "BBCA").await;

        let out = dispatch(
            &db,
            "set_price_alert",
            &serde_json::json!({ "instrument": "BBCA", "target": 9000, "direction": "below" }),
        )
        .await
        .unwrap();
        assert!(out.contains("BBCA"), "{out}");
        assert!(out.contains("below"), "{out}");

        let active = crate::repo::price_alerts::list_active(&db).await.unwrap();
        assert_eq!(active.len(), 1, "should have one active alert");
        assert_eq!(active[0].direction, "below");
        // target is stored as a Decimal string; "9000" is the canonical form.
        let stored: rust_decimal::Decimal = active[0].target_price.parse().unwrap();
        assert_eq!(stored, rust_decimal::Decimal::from(9000u32));
    }

    #[tokio::test]
    async fn set_price_alert_with_percent_computes_target_from_current_price() {
        let db = mem_db().await;
        let ins = seed_instrument(&db, "BBCA").await;
        // Seed current price at 10000.
        crate::repo::prices::upsert_latest(
            &db, ins.id, rust_decimal::Decimal::from(10000u32), "IDR", "manual", "2026-06-15",
        )
        .await
        .unwrap();

        dispatch(
            &db,
            "set_price_alert",
            &serde_json::json!({ "instrument": "BBCA", "percent": 5, "direction": "below" }),
        )
        .await
        .unwrap();

        let active = crate::repo::price_alerts::list_active(&db).await.unwrap();
        assert_eq!(active.len(), 1);
        // 10000 * (1 - 5/100) = 9500
        let stored: rust_decimal::Decimal = active[0].target_price.parse().unwrap();
        assert_eq!(stored, rust_decimal::Decimal::from(9500u32));
    }

    #[tokio::test]
    async fn set_price_alert_unknown_instrument_returns_error() {
        let db = mem_db().await;
        let err = dispatch(
            &db,
            "set_price_alert",
            &serde_json::json!({ "instrument": "ZZZNOPE", "target": 1000, "direction": "below" }),
        )
        .await
        .unwrap_err();
        assert!(
            err.contains("ZZZNOPE") || err.contains("nggak ketemu"),
            "error should mention instrument name: {err}"
        );
    }

    #[tokio::test]
    async fn list_price_alerts_shows_symbol_and_target() {
        let db = mem_db().await;
        let ins = seed_instrument(&db, "TLKM").await;
        crate::repo::price_alerts::create(&db, ins.id, "4500", "above").await.unwrap();

        let out = dispatch(&db, "list_price_alerts", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("TLKM"), "{out}");
        assert!(out.contains("above"), "{out}");
        // Formatted IDR number.
        assert!(out.contains("4.500") || out.contains("4500"), "{out}");
    }

    #[tokio::test]
    async fn list_price_alerts_empty_shows_placeholder() {
        let db = mem_db().await;
        let out = dispatch(&db, "list_price_alerts", &serde_json::json!({})).await.unwrap();
        assert!(
            out.contains("belum ada") || out.contains("(belum ada alert)"),
            "should indicate no alerts: {out}"
        );
    }

    #[tokio::test]
    async fn cancel_price_alert_removes_from_active() {
        let db = mem_db().await;
        let ins = seed_instrument(&db, "ASII").await;
        let alert = crate::repo::price_alerts::create(&db, ins.id, "6000", "below").await.unwrap();

        let out = dispatch(
            &db,
            "cancel_price_alert",
            &serde_json::json!({ "id": alert.id }),
        )
        .await
        .unwrap();
        assert!(out.contains("dibatalkan") || out.contains("cancelled"), "{out}");
        assert!(crate::repo::price_alerts::list_active(&db).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn create_transaction_records_fund_buy_with_units_and_nav() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None,
            native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris Pasar Uang Indonesia".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(),
            decimals: Some(4), note: None,
        }).await.unwrap();
        let input = serde_json::json!({
            "account_id": acc.id, "instrument_id": ins.id, "entry_type": "buy",
            "executed_at": "2026-06-18", "quantity": "1236.7898", "price_native": "1617.0896",
        });
        let out = create_transaction(&db, &input).await.unwrap();
        assert!(out.contains("transaksi"));
        let txns = crate::repo::transactions::list_recent(&db, 10, Some(ins.id), None).await.unwrap();
        assert_eq!(txns.len(), 1);
        assert_eq!(txns[0].quantity.to_string(), "1236.7898");
        assert_eq!(txns[0].price_native.to_string(), "1617.0896");
    }

    #[tokio::test]
    async fn create_transaction_fund_amount_only_asks_for_nav() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let ins = crate::repo::instruments::create(&db, &crate::repo::instruments::NewInstrument {
            symbol: "MJR".into(), name: "Majoris".into(), instrument_type: "fund".into(),
            native_currency: "IDR".into(), category_id: None, price_source: "bibit:MJR02".into(),
            decimals: Some(4), note: None,
        }).await.unwrap();
        let acc = crate::repo::accounts::create(&db, &crate::repo::accounts::NewAccount {
            name: "Bibit #4".into(), account_type: "fund".into(), institution: None, native_currency: "IDR".into(), note: None,
        }).await.unwrap();
        let input = serde_json::json!({
            "account_id": acc.id, "instrument_id": ins.id, "entry_type": "buy",
            "amount_native": "2000000",
        });
        let err = create_transaction(&db, &input).await.unwrap_err();
        assert!(err.to_lowercase().contains("nav") || err.to_lowercase().contains("unit"));
    }
}
