//! Day-plan assembler: deterministic schedule shared by the morning briefing
//! (ordering), the on-demand plan_day tool, and the evening review (ordering).

#[allow(unused_imports)]
use crate::db::Db;
#[allow(unused_imports)]
use crate::repo::events::EventRow;
use crate::repo::todos::TodoRow;
#[allow(unused_imports)]
use chrono::{DateTime, Utc};

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
}
