//! Async human escalation: record it, flip the conversation to needs_human, and
//! best-effort notify the owner (in-app inbox now; Telegram if configured).

use crate::db::Db;

/// Escalate a conversation to the human owner. The escalation row + status flip +
/// inbox entry are the durable record (must succeed). The Telegram ping is
/// best-effort — a notify failure is logged but never fails the escalation, so
/// the customer still gets their reply.
pub async fn escalate(
    db: &Db,
    conversation_id: i64,
    reason: &str,
    summary: &str,
) -> anyhow::Result<()> {
    crate::repo::cs::escalation_create(db, conversation_id, reason, summary).await?;
    crate::repo::cs::conversation_set_status(db, conversation_id, "needs_human").await?;

    // Build an inbox line with the visitor's identity for context.
    let label = inbox_label(db, conversation_id).await;
    if let Err(e) = crate::repo::inbox::create(db, &format!("[CS] {label}: {summary}")).await {
        tracing::warn!("cs escalation: inbox create failed: {e}");
    }

    notify_owner_telegram(db, &format!("🆘 CS butuh kamu — {label}\n{summary}")).await;
    Ok(())
}

/// "Budi (b@x.com)" style label from the conversation row; falls back to the id.
async fn inbox_label(db: &Db, conversation_id: i64) -> String {
    // Direct by-id lookup added to repo::cs (cleaner than scanning the recent list).
    match crate::repo::cs::conversation_get(db, conversation_id).await {
        Ok(Some(c)) => {
            let name = c.visitor_name.unwrap_or_else(|| format!("conv#{conversation_id}"));
            match c.visitor_email.or(c.visitor_phone) {
                Some(contact) => format!("{name} ({contact})"),
                None          => name,
            }
        }
        _ => format!("conv#{conversation_id}"),
    }
}

/// Best-effort Telegram ping to the linked owner. Never returns an error.
async fn notify_owner_telegram(db: &Db, text: &str) {
    let token = match std::env::var("TELEGRAM_BOT_TOKEN") {
        Ok(t) if !t.is_empty() => t,
        _ => return,
    };
    let link = match crate::repo::telegram_link::get(db).await {
        Ok(Some(row)) => row,
        _ => return,
    };
    let client = crate::telegram::client::TelegramClient::new(token);
    if let Err(e) = client.send_message(link.chat_id, text).await {
        tracing::warn!("cs escalation: telegram notify failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn escalate_records_row_flips_status_and_inboxes() {
        let db   = mem_db().await;
        let conv = crate::repo::cs::conversation_create(
            &db, "web", Some("Budi"), Some("b@x.com"), None, "tok-esc",
        )
        .await
        .unwrap();

        escalate(&db, conv.id, "cannot_answer", "Customer asks about custom integration")
            .await
            .unwrap();

        // escalation row created and open
        let open = crate::repo::cs::escalation_list_open(&db).await.unwrap();
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].conversation_id, conv.id);

        // conversation flipped to needs_human
        let after = crate::repo::cs::conversation_by_token(&db, "tok-esc").await.unwrap().unwrap();
        assert_eq!(after.status, "needs_human");

        // a pending inbox item exists (owner sees it in-app)
        let inbox = crate::repo::inbox::list_pending(&db).await.unwrap();
        assert_eq!(inbox.len(), 1);
        assert!(inbox[0].content.contains("Budi"));
    }
}
