//! Daily evening review: what got done, what's left, and an offer to roll the
//! leftovers to tomorrow. Deterministic gather → compose-and-send.

use crate::db::Db;
use crate::repo::todos::TodoRow;
use chrono::{DateTime, Datelike, Utc};

pub struct ReviewData {
    pub date_wib: String,
    pub weekday: String,
    pub done_today: Vec<TodoRow>,
    pub unfinished: Vec<TodoRow>,
}

/// Open todos whose due date (WIB) is today or earlier — the rollover candidates.
fn unfinished_through_today(open: Vec<TodoRow>, today_wib: &str) -> Vec<TodoRow> {
    open.into_iter()
        .filter(|t| {
            t.due_at
                .as_deref()
                .and_then(|d| chrono::DateTime::parse_from_rfc3339(d).ok())
                .map(|dt| {
                    dt.with_timezone(&crate::assistant::time::wib())
                        .format("%Y-%m-%d")
                        .to_string()
                        .as_str()
                        <= today_wib
                })
                .unwrap_or(false)
        })
        .collect()
}

pub async fn gather(db: &Db, now_utc: DateTime<Utc>) -> anyhow::Result<ReviewData> {
    let now_wib = now_utc.with_timezone(&crate::assistant::time::wib());
    let today_wib = now_wib.format("%Y-%m-%d").to_string();

    // Start of today in WIB, expressed as a +00:00 RFC3339 string to match the
    // format `todos::complete` writes into completed_at.
    let day_start_utc = crate::assistant::time::start_of_today_wib(now_utc).to_rfc3339();

    let done_today = crate::repo::todos::completed_since(db, &day_start_utc).await?;
    let unfinished = super::plan::order_todos(unfinished_through_today(
        crate::repo::todos::list_open(db).await?,
        &today_wib,
    ));

    Ok(ReviewData {
        date_wib: today_wib,
        weekday: crate::assistant::time::weekday_id(now_wib.weekday()).to_string(),
        done_today,
        unfinished,
    })
}

pub fn render_data_block(d: &ReviewData) -> String {
    let mut out = format!("Review sore: {}, {} (WIB)\n", d.weekday, d.date_wib);

    out.push_str("Selesai hari ini:\n");
    if d.done_today.is_empty() {
        out.push_str("(belum ada)\n");
    } else {
        for t in &d.done_today {
            out.push_str(&format!("- #{} {}\n", t.id, t.title));
        }
    }

    out.push_str("Belum kelar:\n");
    if d.unfinished.is_empty() {
        out.push_str("(semua kelar)\n");
    } else {
        for t in &d.unfinished {
            out.push_str(&format!("- #{} {}", t.id, t.title));
            if let Some(due) = &t.due_at {
                out.push_str(&format!(" (due {})", crate::assistant::time::to_wib_display(due)));
            }
            out.push('\n');
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
    let data = gather(db, chrono::Utc::now()).await?;
    let block = render_data_block(&data);
    let text =
        super::compose::compose(super::compose::REVIEW_SYSTEM, &block, "🌙 Review sore (mode ringkas)").await;
    client
        .send_message(chat_id, &text)
        .await
        .map_err(|e| anyhow::anyhow!("evening review send failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn todo(id: i64, due_at: Option<&str>) -> TodoRow {
        TodoRow {
            id,
            title: format!("t{id}"),
            notes: None,
            due_at: due_at.map(|s| s.into()),
            status: "open".into(),
            created_at: String::new(),
            completed_at: None,
            priority: None,
            estimate_minutes: None,
        }
    }

    #[test]
    fn unfinished_keeps_overdue_and_today_drops_future_and_undated() {
        let kept = unfinished_through_today(
            vec![
                todo(1, Some("2026-06-10T02:00:00Z")), // overdue
                todo(2, Some("2026-06-12T02:00:00Z")), // today
                todo(3, Some("2026-06-20T02:00:00Z")), // future
                todo(4, None),                         // undated
            ],
            "2026-06-12",
        );
        let ids: Vec<i64> = kept.iter().map(|t| t.id).collect();
        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn render_block_shows_done_and_unfinished_sections() {
        let d = ReviewData {
            date_wib: "2026-06-12".into(),
            weekday: "Jumat".into(),
            done_today: vec![todo(5, None)],
            unfinished: vec![todo(6, Some("2026-06-12T02:00:00Z"))],
        };
        let block = render_data_block(&d);
        assert!(block.contains("Selesai hari ini:"), "{block}");
        assert!(block.contains("#5 t5"), "{block}");
        assert!(block.contains("Belum kelar:"), "{block}");
        assert!(block.contains("#6 t6"), "{block}");
    }

    #[tokio::test]
    async fn gather_works_on_an_empty_db() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let d = gather(&db, chrono::Utc::now()).await.unwrap();
        assert!(d.done_today.is_empty());
        assert!(d.unfinished.is_empty());
    }
}
