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
    async fn portfolio_summary_renders_for_an_empty_db() {
        let db = mem_db().await;
        let out = dispatch(&db, "get_portfolio_summary", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Net worth"), "{out}");
    }
}
