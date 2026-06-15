//! Day-plan assembler: deterministic schedule shared by the morning briefing
//! (ordering), the on-demand plan_day tool, and the evening review (ordering).

use crate::db::Db;
use crate::repo::events::EventRow;
use crate::repo::todos::TodoRow;
use chrono::{DateTime, Datelike, Utc};

/// Sort rank for priority; NULL/unknown is treated as 'normal'.
fn priority_rank(priority: Option<&str>) -> u8 {
    match priority {
        Some("high") => 0,
        Some("low") => 2,
        _ => 1,
    }
}

/// Order open todos for planning: priority (high→low), then earliest due
/// (undated last), then shortest estimate (unknown last). Stable for ties.
pub fn order_todos(mut todos: Vec<TodoRow>) -> Vec<TodoRow> {
    todos.sort_by(|a, b| {
        priority_rank(a.priority.as_deref())
            .cmp(&priority_rank(b.priority.as_deref()))
            .then_with(|| match (&a.due_at, &b.due_at) {
                (Some(x), Some(y)) => x.cmp(y),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
            .then_with(|| {
                a.estimate_minutes
                    .unwrap_or(i64::MAX)
                    .cmp(&b.estimate_minutes.unwrap_or(i64::MAX))
            })
    });
    todos
}

pub struct DayPlan {
    pub date_wib: String,
    pub weekday: String,
    pub events: Vec<EventRow>,
    pub todos: Vec<TodoRow>,
}

/// Gather today's (WIB) events and open todos, todos ordered for planning.
pub async fn gather(db: &Db, now_utc: DateTime<Utc>) -> anyhow::Result<DayPlan> {
    let now_wib = now_utc.with_timezone(&crate::assistant::time::wib());
    let date_wib = now_wib.format("%Y-%m-%d").to_string();

    let day_start = crate::assistant::time::start_of_today_wib(now_utc);
    let events = crate::repo::events::list_between(
        db,
        &crate::assistant::time::to_db_utc(day_start),
        &crate::assistant::time::to_db_utc(day_start + chrono::Duration::days(1)),
    )
    .await?;

    let todos = order_todos(crate::repo::todos::list_open(db).await?);

    Ok(DayPlan {
        date_wib,
        weekday: crate::assistant::time::weekday_id(now_wib.weekday()).to_string(),
        events,
        todos,
    })
}

/// Deterministic plan block: LLM input and fallback body.
pub fn render_plan_block(plan: &DayPlan) -> String {
    let mut out = format!("Rencana hari: {}, {} (WIB)\n", plan.weekday, plan.date_wib);

    out.push_str("Agenda (jam pasti):\n");
    if plan.events.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for e in &plan.events {
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

    out.push_str("Todo (urut prioritas):\n");
    if plan.todos.is_empty() {
        out.push_str("(tidak ada)\n");
    } else {
        for t in &plan.todos {
            out.push_str(&format!("- #{} {}", t.id, t.title));
            let priority = t.priority.as_deref().unwrap_or("normal");
            out.push_str(&format!(" [{priority}]"));
            if let Some(est) = t.estimate_minutes {
                out.push_str(&format!(" ~{est}m"));
            }
            if let Some(due) = &t.due_at {
                out.push_str(&format!(" (due {})", crate::assistant::time::to_wib_display(due)));
            }
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: i64, priority: Option<&str>, due_at: Option<&str>, est: Option<i64>) -> TodoRow {
        TodoRow {
            id,
            title: format!("t{id}"),
            notes: None,
            due_at: due_at.map(|s| s.into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: priority.map(|s| s.into()),
            estimate_minutes: est,
        }
    }

    fn event(id: i64, start_at: &str, title: &str) -> EventRow {
        EventRow {
            id,
            title: title.into(),
            location: None,
            notes: None,
            start_at: start_at.into(),
            status: "scheduled".into(),
            created_at: String::new(),
            source: "local".into(),
            google_event_id: None,
            google_etag: None,
            synced_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn orders_by_priority_then_due_then_estimate() {
        let ordered = order_todos(vec![
            todo(1, Some("low"), Some("2026-06-12T00:00:00Z"), None),
            todo(2, Some("high"), None, Some(60)),
            todo(3, Some("high"), Some("2026-06-12T00:00:00Z"), Some(15)),
            todo(4, None, Some("2026-06-11T00:00:00Z"), None),
        ]);
        let ids: Vec<i64> = ordered.iter().map(|t| t.id).collect();
        // high+due(3) → high+undated(2) → normal(4) → low(1)
        assert_eq!(ids, vec![3, 2, 4, 1]);
    }

    #[test]
    fn render_block_lists_events_and_ordered_todos() {
        let plan = DayPlan {
            date_wib: "2026-06-12".into(),
            weekday: "Jumat".into(),
            events: vec![event(1, "2026-06-12T03:00:00Z", "meeting klien")], // 10:00 WIB
            todos: order_todos(vec![
                todo(7, Some("high"), None, Some(30)),
                todo(8, Some("low"), None, None),
            ]),
        };
        let block = render_plan_block(&plan);
        assert!(block.contains("Jumat, 2026-06-12"), "{block}");
        assert!(block.contains("10:00 WIB"), "{block}");
        assert!(block.contains("meeting klien"), "{block}");
        assert!(block.contains("#7 t7"), "{block}");
        let hi = block.find("#7").unwrap();
        let lo = block.find("#8").unwrap();
        assert!(hi < lo, "{block}");
    }
}
