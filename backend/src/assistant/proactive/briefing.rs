//! Morning briefing: deterministic gathering, then compose-and-send.

use crate::db::Db;
use crate::repo::reminders::ReminderRow;
use crate::repo::todos::TodoRow;
use crate::service::chat::group_id;
use crate::service::movers::Mover;
use chrono::Datelike;
use rust_decimal::Decimal;

pub struct BriefingData {
    pub date_wib: String,
    pub weekday: String,
    pub todos_due_today: Vec<TodoRow>,
    pub todos_overdue: Vec<TodoRow>,
    pub reminders_today: Vec<ReminderRow>,
    pub events_today: Vec<crate::repo::events::EventRow>,
    pub net_worth_idr: Decimal,
    pub delta_vs_yesterday_idr: Option<Decimal>,
    pub movers: Vec<Mover>,
    pub pending_reviews: usize,
    pub memory_facts: Vec<crate::assistant::memory::MemoryFact>,
    /// Overdue/due-today ClickUp tasks grouped by project. `None` when ClickUp
    /// isn't configured (section omitted); `Some(empty)` when nothing is due.
    pub clickup_due: Option<Vec<(String, Vec<String>)>>,
}

/// One briefing line for a task that is overdue or due today; None otherwise
/// (future due date, or no due date — those don't belong in a "due" view).
fn clickup_due_line(task: &crate::clickup::client::Task, now_ms: i64, end_today_ms: i64) -> Option<String> {
    let due = task.due_date_ms?;
    if due < now_ms {
        Some(format!("{} (overdue)", task.name))
    } else if due <= end_today_ms {
        Some(format!("{} (hari ini)", task.name))
    } else {
        None
    }
}

/// Collect overdue/due-today tasks grouped by project name.
async fn gather_clickup_due(
    api: &dyn crate::clickup::ClickUpApi,
) -> anyhow::Result<Vec<(String, Vec<String>)>> {
    let now = chrono::Utc::now();
    let now_ms = now.timestamp_millis();
    let end_today = crate::assistant::time::end_of_today_wib_ms(now);
    let projects = api.list_projects().await?;
    let mut grouped = Vec::new();
    for project in projects {
        let tasks = api.list_tasks(&project.id).await?;
        let lines: Vec<String> = tasks
            .iter()
            .filter_map(|t| clickup_due_line(t, now_ms, end_today))
            .collect();
        if !lines.is_empty() {
            grouped.push((project.name, lines));
        }
    }
    Ok(grouped)
}

/// Split open todos into due-today and overdue by their WIB calendar date.
fn classify_todos(open_todos: Vec<TodoRow>, today_wib: &str) -> (Vec<TodoRow>, Vec<TodoRow>) {
    let (mut due_today, mut overdue) = (Vec::new(), Vec::new());
    for todo in open_todos {
        let Some(due_at) = &todo.due_at else { continue };
        let due_date_wib = match chrono::DateTime::parse_from_rfc3339(due_at) {
            Ok(dt) => dt
                .with_timezone(&crate::assistant::time::wib())
                .format("%Y-%m-%d")
                .to_string(),
            Err(_) => continue,
        };
        if due_date_wib == today_wib {
            due_today.push(todo);
        } else if due_date_wib.as_str() < today_wib {
            overdue.push(todo);
        }
    }
    (due_today, overdue)
}

/// Gather everything deterministically. Each finance source degrades
/// independently; todo/reminder/review failures propagate (they're local DB).
pub async fn gather(db: &Db) -> anyhow::Result<BriefingData> {
    let now_wib = chrono::Utc::now().with_timezone(&crate::assistant::time::wib());
    let today = now_wib.format("%Y-%m-%d").to_string();

    let open_todos = crate::repo::todos::list_open(db).await?;
    let (todos_due_today, todos_overdue) = classify_todos(open_todos, &today);
    let todos_due_today = super::plan::order_todos(todos_due_today);
    let todos_overdue = super::plan::order_todos(todos_overdue);

    let reminders_today = crate::repo::reminders::list_pending(db)
        .await?
        .into_iter()
        .filter(|r| {
            chrono::DateTime::parse_from_rfc3339(&r.remind_at)
                .map(|dt| {
                    dt.with_timezone(&crate::assistant::time::wib())
                        .format("%Y-%m-%d")
                        .to_string()
                        == today
                })
                .unwrap_or(false)
        })
        .collect();

    // Today's WIB calendar day expressed as a UTC range.
    let day_start = now_wib
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_local_timezone(crate::assistant::time::wib())
        .single()
        .expect("WIB has no DST gaps")
        .with_timezone(&chrono::Utc);
    let events_today = crate::repo::events::list_between(
        db,
        &crate::assistant::time::to_db_utc(day_start),
        &crate::assistant::time::to_db_utc(day_start + chrono::Duration::days(1)),
    )
    .await?;

    let pending_reviews = crate::repo::review_items::list_by_status(db, "pending")
        .await
        .map(|items| items.len())
        .unwrap_or(0);

    let (net_worth_idr, delta_vs_yesterday_idr) =
        match crate::service::portfolio::build_summary(db).await {
            Ok(summary) => {
                // ~24h baseline: see snapshot_before's doc for why yesterday.
                let yesterday =
                    (now_wib - chrono::Duration::days(1)).format("%Y-%m-%d").to_string();
                let delta = match crate::repo::snapshots::history(db).await {
                    Ok(rows) => super::snapshot_before(&rows, &yesterday)
                        .map(|prev| summary.net_worth_idr - prev),
                    Err(_) => None,
                };
                (summary.net_worth_idr, delta)
            }
            Err(e) => {
                tracing::warn!("briefing: portfolio summary unavailable: {e:#}");
                (Decimal::ZERO, None)
            }
        };

    let movers = crate::service::movers::daily_movers(db, 3).await.unwrap_or_else(|e| {
        tracing::warn!("briefing: movers unavailable: {e:#}");
        Vec::new()
    });

    let memory_facts = match crate::assistant::memory::MemoryClient::from_env() {
        Some(client) => {
            client
                .search(
                    &format!(
                        "hal penting hari ini {} {today}",
                        crate::assistant::time::weekday_id(now_wib.weekday())
                    ),
                    5,
                )
                .await
        }
        None => Vec::new(),
    };

    let clickup_due = match crate::clickup::ClickUpClient::from_env() {
        Ok(api) => Some(gather_clickup_due(&api).await.unwrap_or_else(|e| {
            tracing::warn!("briefing: clickup due tasks unavailable: {e:#}");
            Vec::new()
        })),
        Err(_) => None, // not configured → section omitted
    };

    Ok(BriefingData {
        date_wib: today,
        weekday: crate::assistant::time::weekday_id(now_wib.weekday()).to_string(),
        todos_due_today,
        todos_overdue,
        reminders_today,
        events_today,
        net_worth_idr,
        delta_vs_yesterday_idr,
        movers,
        pending_reviews,
        memory_facts,
        clickup_due,
    })
}

fn push_todo_lines(out: &mut String, todos: &[TodoRow]) {
    if todos.is_empty() {
        out.push_str("(tidak ada)\n");
        return;
    }
    for todo in todos {
        out.push_str(&format!("- #{} {}", todo.id, todo.title));
        if let Some(due) = &todo.due_at {
            out.push_str(&format!(
                " (due {})",
                crate::assistant::time::to_wib_display(due)
            ));
        }
        out.push('\n');
    }
}

/// Deterministic data block: both the LLM input and the fallback body.
pub fn render_data_block(d: &BriefingData) -> String {
    let mut out = format!("Hari: {}, {} (WIB)\n", d.weekday, d.date_wib);

    out.push_str("Todo jatuh tempo hari ini:\n");
    push_todo_lines(&mut out, &d.todos_due_today);
    out.push_str("Todo terlambat:\n");
    push_todo_lines(&mut out, &d.todos_overdue);

    out.push_str("Reminder hari ini:\n");
    if d.reminders_today.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for r in &d.reminders_today {
            out.push_str(&format!(
                "- {}: {}\n",
                crate::assistant::time::to_wib_display(&r.remind_at),
                r.message
            ));
        }
    }

    out.push_str("Agenda hari ini:\n");
    if d.events_today.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for e in &d.events_today {
            out.push_str(&format!(
                "- {}: {}",
                crate::assistant::time::to_wib_display(&e.start_at),
                e.title
            ));
            if let Some(location) = &e.location {
                out.push_str(&format!(" ({location})"));
            }
            out.push('\n');
        }
    }

    out.push_str(&format!(
        "Net worth: Rp {}\n",
        group_id(&d.net_worth_idr.round_dp(0))
    ));
    if let Some(delta) = &d.delta_vs_yesterday_idr {
        let sign = if delta.is_sign_negative() { "-" } else { "+" };
        out.push_str(&format!(
            "Perubahan vs kemarin: {sign}Rp {}\n",
            group_id(&delta.abs().round_dp(0))
        ));
    }
    if !d.movers.is_empty() {
        out.push_str("Movers:\n");
        for m in &d.movers {
            let sign = if m.delta_idr.is_sign_negative() { "-" } else { "+" };
            out.push_str(&format!(
                "- {} {}: {sign}Rp {}\n",
                m.symbol,
                format!("{:+.1}%", m.delta_pct).replace('.', ","),
                group_id(&m.delta_idr.abs().round_dp(0)),
            ));
        }
    }
    out.push_str(&format!("Review pending: {}\n", d.pending_reviews));

    if let Some(due) = &d.clickup_due {
        out.push_str("Task ClickUp jatuh tempo:\n");
        if due.is_empty() {
            out.push_str("(tidak ada)\n");
        } else {
            for (project, lines) in due {
                out.push_str(&format!("- {project}:\n"));
                for line in lines {
                    out.push_str(&format!("  - {line}\n"));
                }
            }
        }
    }

    if !d.memory_facts.is_empty() {
        out.push_str("Fakta tersimpan yang mungkin relevan:\n");
        for f in &d.memory_facts {
            out.push_str(&format!("- {}\n", f.fact));
        }
    }
    out
}

/// Gather → compose → send. The caller has already claimed the dedup key.
pub async fn run(
    db: &Db,
    client: &crate::telegram::client::TelegramClient,
    chat_id: i64,
) -> anyhow::Result<()> {
    let data = gather(db).await?;
    let block = render_data_block(&data);
    let text = super::compose::compose(
        super::compose::BRIEFING_SYSTEM,
        &block,
        "📋 Briefing (mode ringkas)",
    )
    .await;
    client
        .send_message(chat_id, &text)
        .await
        .map_err(|e| anyhow::anyhow!("briefing send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn data() -> BriefingData {
        BriefingData {
            date_wib: "2026-06-12".into(),
            weekday: "Jumat".into(),
            todos_due_today: vec![],
            todos_overdue: vec![],
            reminders_today: vec![],
            events_today: vec![],
            net_worth_idr: dec!(91960083),
            delta_vs_yesterday_idr: Some(dec!(1200000)),
            movers: vec![],
            pending_reviews: 0,
            memory_facts: vec![],
            clickup_due: None,
        }
    }

    #[test]
    fn clickup_due_line_tags_overdue_and_today_only() {
        use crate::clickup::client::Task;
        let now = 100_000i64;
        let end = 200_000i64;
        let mk = |due: Option<i64>| Task {
            id: "t".into(),
            name: "kerjaan".into(),
            status: "to do".into(),
            due_date_ms: due,
        };
        assert_eq!(clickup_due_line(&mk(Some(50_000)), now, end).as_deref(), Some("kerjaan (overdue)"));
        assert_eq!(clickup_due_line(&mk(Some(150_000)), now, end).as_deref(), Some("kerjaan (hari ini)"));
        assert_eq!(clickup_due_line(&mk(Some(300_000)), now, end), None);
        assert_eq!(clickup_due_line(&mk(None), now, end), None);
    }

    #[test]
    fn clickup_section_renders_grouped_due_tasks() {
        let mut d = data();
        d.clickup_due = Some(vec![("PT AIS".into(), vec!["landing page (overdue)".into()])]);
        let block = render_data_block(&d);
        assert!(block.contains("Task ClickUp jatuh tempo:"), "{block}");
        assert!(block.contains("PT AIS"), "{block}");
        assert!(block.contains("landing page (overdue)"), "{block}");
    }

    #[test]
    fn clickup_section_omitted_when_unconfigured() {
        let d = data(); // clickup_due = None
        let block = render_data_block(&d);
        assert!(!block.contains("Task ClickUp"), "{block}");
    }

    #[test]
    fn block_contains_date_and_net_worth() {
        let block = render_data_block(&data());
        assert!(block.contains("Jumat, 2026-06-12"), "{block}");
        assert!(block.contains("Rp 91.960.083"), "{block}");
        assert!(block.contains("+Rp 1.200.000"), "{block}");
    }

    #[test]
    fn empty_sections_say_so_instead_of_vanishing() {
        let block = render_data_block(&data());
        // The LLM must be able to see "nothing today" explicitly.
        assert!(block.contains("(tidak ada)"), "{block}");
    }

    #[test]
    fn todos_and_facts_render_with_details() {
        let mut d = data();
        d.todos_due_today = vec![TodoRow {
            id: 3,
            title: "bayar listrik".into(),
            notes: None,
            due_at: Some("2026-06-12T02:00:00Z".into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: None,
            estimate_minutes: None,
        }];
        d.pending_reviews = 2;
        d.memory_facts = vec![crate::assistant::memory::MemoryFact {
            fact: "gajian tanggal 25".into(),
            valid_at: None,
            name: "R".into(),
        }];
        let block = render_data_block(&d);
        assert!(block.contains("#3 bayar listrik"), "{block}");
        assert!(block.contains("09:00 WIB"), "{block}"); // 02:00Z = 09:00 WIB
        assert!(block.contains("Review pending: 2"), "{block}");
        assert!(block.contains("gajian tanggal 25"), "{block}");
    }

    #[tokio::test]
    async fn gather_works_on_an_empty_db() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let d = gather(&db).await.unwrap();
        assert!(d.todos_due_today.is_empty());
        assert!(d.reminders_today.is_empty());
        assert_eq!(d.pending_reviews, 0);
    }

    #[test]
    fn agenda_section_renders_events_with_wib_time_and_location() {
        let mut d = data();
        d.events_today = vec![crate::repo::events::EventRow {
            id: 1,
            title: "meeting vendor".into(),
            location: Some("kantor".into()),
            notes: None,
            start_at: "2026-06-12T07:00:00Z".into(), // 14:00 WIB
            status: "scheduled".into(),
            created_at: String::new(),
            source: "local".into(),
            google_event_id: None,
            google_etag: None,
            synced_at: None,
            updated_at: None,
        }];
        let block = render_data_block(&d);
        assert!(block.contains("Agenda hari ini:"), "{block}");
        assert!(block.contains("meeting vendor (kantor)"), "{block}");
        assert!(block.contains("14:00 WIB"), "{block}");
    }

    fn todo_due(id: i64, due_at: &str) -> TodoRow {
        TodoRow {
            id,
            title: format!("t{id}"),
            notes: None,
            due_at: Some(due_at.into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: None,
            estimate_minutes: None,
        }
    }

    #[test]
    fn classification_respects_the_wib_date_boundary() {
        // 16:30Z = 23:30 WIB the SAME day; 17:30Z = 00:30 WIB the NEXT day.
        let (due, overdue) = classify_todos(
            vec![todo_due(1, "2026-06-12T16:30:00Z"), todo_due(2, "2026-06-12T17:30:00Z")],
            "2026-06-12",
        );
        assert_eq!(due.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1]);
        assert!(overdue.is_empty());
        // The next WIB day, the 17:30Z todo is due and the 16:30Z one overdue.
        let (due, overdue) = classify_todos(
            vec![todo_due(1, "2026-06-12T16:30:00Z"), todo_due(2, "2026-06-12T17:30:00Z")],
            "2026-06-13",
        );
        assert_eq!(due.iter().map(|t| t.id).collect::<Vec<_>>(), vec![2]);
        assert_eq!(overdue.iter().map(|t| t.id).collect::<Vec<_>>(), vec![1]);
    }
}
