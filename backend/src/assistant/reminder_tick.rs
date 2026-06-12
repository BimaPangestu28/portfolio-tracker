//! 60-second tick loop: deliver due reminders to the linked Telegram chat.

use crate::db::Db;
use crate::repo::reminders::ReminderRow;
use crate::telegram::client::TelegramClient;

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn one_shot_is_marked_sent() {
        let db = mem_db().await;
        let r = crate::repo::reminders::create(&db, None, "x", "2026-06-10T00:00:00Z", "none")
            .await.unwrap();
        finalize_delivered(&db, &r).await.unwrap();
        let row = crate::repo::reminders::get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "sent");
        assert!(row.sent_at.is_some());
    }

    #[tokio::test]
    async fn recurring_is_rescheduled_into_the_future() {
        let db = mem_db().await;
        // remind_at far in the past: next occurrence must land after now,
        // not at the next slot after the stale remind_at.
        let r = crate::repo::reminders::create(&db, None, "x", "2026-01-01T02:00:00Z", "daily")
            .await.unwrap();
        finalize_delivered(&db, &r).await.unwrap();
        let row = crate::repo::reminders::get(&db, r.id).await.unwrap();
        assert_eq!(row.status, "pending");
        assert!(row.sent_at.is_some());
        let next = chrono::DateTime::parse_from_rfc3339(&row.remind_at).unwrap();
        assert!(next.with_timezone(&chrono::Utc) > chrono::Utc::now(), "{}", row.remind_at);
    }

    #[test]
    fn reminder_text_is_prefixed() {
        let row = ReminderRow {
            id: 1, todo_id: None, message: "bayar listrik".into(),
            remind_at: "2026-06-10T00:00:00Z".into(), recurrence: "none".into(),
            status: "pending".into(), sent_at: None, event_id: None,
        };
        assert_eq!(reminder_text(&row), "⏰ bayar listrik");
    }
}

const TICK: std::time::Duration = std::time::Duration::from_secs(60);

/// Spawn the delivery loop when TELEGRAM_BOT_TOKEN is configured. Without the
/// token reminders still accumulate; they deliver once a token is set and the
/// backend restarts.
pub fn spawn(db: Db) {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else {
        tracing::info!("TELEGRAM_BOT_TOKEN not set; reminder delivery disabled");
        return;
    };
    tokio::spawn(async move {
        let client = TelegramClient::new(token);
        loop {
            if let Err(e) = tick(&db, &client).await {
                tracing::warn!("reminder tick failed: {e:#}");
            }
            tokio::time::sleep(TICK).await;
        }
    });
}

/// The user-facing notification line for one reminder.
fn reminder_text(reminder: &ReminderRow) -> String {
    format!("⏰ {}", reminder.message)
}

/// Deliver every due reminder. A failed send leaves the reminder 'pending'
/// so the next tick retries — slightly late beats lost. Delivery is
/// at-least-once: a finalize failure (or a crash between send and finalize)
/// re-sends the same reminder on the next tick.
async fn tick(db: &Db, client: &TelegramClient) -> anyhow::Result<()> {
    let now = super::time::to_db_utc(chrono::Utc::now());
    let due = crate::repo::reminders::due(db, &now).await?;
    if due.is_empty() {
        return Ok(());
    }
    let Some(link) = crate::repo::telegram_link::get(db).await? else {
        tracing::warn!("{} reminder(s) due but no Telegram chat is linked", due.len());
        return Ok(());
    };
    for reminder in due {
        let text = reminder_text(&reminder);
        let send_result = match reminder.todo_id {
            Some(todo_id) => {
                let callback = format!("tododone:{todo_id}");
                client
                    .send_message_with_buttons(
                        link.chat_id,
                        &text,
                        &[("✅ Selesai", callback.as_str())],
                    )
                    .await
            }
            None => client.send_message(link.chat_id, &text).await,
        };
        if let Err(e) = send_result {
            tracing::warn!("reminder #{} send failed (will retry): {e:#}", reminder.id);
            continue;
        }
        // Mirror the send path: one bad row must not stall the rest of the
        // batch. The row stays pending and re-sends next tick (at-least-once).
        if let Err(e) = finalize_delivered(db, &reminder).await {
            tracing::warn!("reminder #{} finalize failed (may re-send): {e:#}", reminder.id);
        }
    }
    Ok(())
}

/// After a successful send: one-shots become 'sent'; recurring reminders are
/// rescheduled strictly past now so a late delivery doesn't refire at once.
/// Callers must pass rows fresh from `due()` — `mark_sent`/`reschedule` are
/// unconditional updates that assume a pending row.
async fn finalize_delivered(db: &Db, reminder: &ReminderRow) -> anyhow::Result<()> {
    let now = chrono::Utc::now();
    let sent_at = super::time::to_db_utc(now);
    let current = chrono::DateTime::parse_from_rfc3339(&reminder.remind_at)
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or(now);
    match super::recurrence::next_after(current, &reminder.recurrence, now) {
        Some(next) => {
            crate::repo::reminders::reschedule(
                db,
                reminder.id,
                &super::time::to_db_utc(next),
                &sent_at,
            )
            .await
        }
        None => crate::repo::reminders::mark_sent(db, reminder.id, &sent_at).await,
    }
}
