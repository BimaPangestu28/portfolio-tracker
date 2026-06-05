//! Telegram bot channel: linking state, Bot API client, and the polling loop.
//!
//! The poller is spawned from main() only when TELEGRAM_BOT_TOKEN is set. It
//! long-polls getUpdates and answers messages from the linked owner chat via
//! the shared chat service. Messages from unlinked chats are only ever used
//! for the one-time link-code handshake.

pub mod client;
pub mod state;

/// What to do with an inbound text message, decided from the link state.
#[derive(Debug, PartialEq, Eq)]
pub enum Action {
    /// Linked owner chat: answer via the chat service.
    Answer,
    /// No link exists yet: try the message as a link code.
    TryLink,
    /// A link exists and this is some other chat: ignore silently.
    Ignore,
}

/// Pure dispatch decision: who may talk to the bot.
pub fn plan_action(linked_chat_id: Option<i64>, from_chat_id: i64) -> Action {
    match linked_chat_id {
        Some(id) if id == from_chat_id => Action::Answer,
        Some(_) => Action::Ignore,
        None => Action::TryLink,
    }
}

use crate::db::Db;
use client::{TelegramClient, TgError, TgUpdate};
use state::SharedTgState;
use std::time::Instant;

const LINK_OK_REPLY: &str =
    "✅ Telegram tertaut. Silakan tanya apa saja tentang portofoliomu.";
const LINK_HINT_REPLY: &str =
    "Kode tidak valid atau kedaluwarsa. Buka halaman Telegram di web UI untuk membuat kode tautan.";
const ANSWER_FAILED_REPLY: &str =
    "Maaf, lagi ada gangguan saat menjawab. Coba lagi sebentar lagi ya.";

/// Spawn the background poller when TELEGRAM_BOT_TOKEN is configured.
/// Without the token the Telegram channel is simply off.
pub fn spawn(db: Db, tg: SharedTgState) {
    let Ok(token) = std::env::var("TELEGRAM_BOT_TOKEN") else {
        tracing::info!("TELEGRAM_BOT_TOKEN not set; telegram channel disabled");
        return;
    };
    tokio::spawn(async move {
        poll_loop(TelegramClient::new(token), db, tg).await;
    });
}

/// Long-poll getUpdates forever. Network errors back off and retry; a 401
/// (bad token) flags the state for the UI and waits longer between retries.
async fn poll_loop(client: TelegramClient, db: Db, tg: SharedTgState) {
    tracing::info!("telegram poller started");
    let mut offset = 0i64;
    loop {
        match client.get_updates(offset).await {
            Ok(updates) => {
                for update in updates {
                    offset = offset.max(update.update_id + 1);
                    handle_update(&client, &db, &tg, update).await;
                }
            }
            Err(TgError::Unauthorized) => {
                tracing::error!("telegram rejected the bot token; check TELEGRAM_BOT_TOKEN");
                if let Ok(mut guard) = tg.lock() {
                    guard.set_auth_failed();
                }
                tokio::time::sleep(std::time::Duration::from_secs(60)).await;
            }
            Err(e) => {
                tracing::warn!("telegram getUpdates failed: {e}; retrying");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    }
}

/// Process one update end-to-end. All failures are logged, never propagated —
/// one bad message must not kill the poller.
async fn handle_update(client: &TelegramClient, db: &Db, tg: &SharedTgState, update: TgUpdate) {
    // Ignore non-message updates and non-text messages.
    let Some(message) = update.message else { return };
    let Some(text) = message.text.as_deref().filter(|t| !t.trim().is_empty()) else { return };
    let chat_id = message.chat.id;

    let linked = match crate::repo::telegram_link::get(db).await {
        Ok(row) => row.map(|r| r.chat_id),
        Err(e) => {
            tracing::error!("telegram: failed to read link row: {e:#}");
            return;
        }
    };

    match plan_action(linked, chat_id) {
        Action::Answer => {
            let reply = answer(db, text).await.unwrap_or_else(|e| {
                tracing::error!("telegram: answer failed: {e:#}");
                ANSWER_FAILED_REPLY.to_string()
            });
            send_or_log(client, chat_id, &reply).await;
        }
        Action::TryLink => {
            let code_ok = match tg.lock() {
                Ok(mut guard) => guard.verify_code(text, Instant::now()),
                Err(_) => false,
            };
            if code_ok {
                let username = message.from.as_ref().and_then(|u| u.username.as_deref());
                match crate::repo::telegram_link::set(db, chat_id, username).await {
                    Ok(()) => {
                        tracing::info!("telegram: linked chat {chat_id}");
                        send_or_log(client, chat_id, LINK_OK_REPLY).await;
                    }
                    Err(e) => tracing::error!("telegram: failed to persist link: {e:#}"),
                }
            } else {
                send_or_log(client, chat_id, LINK_HINT_REPLY).await;
            }
        }
        Action::Ignore => {}
    }
}

/// Answer a linked owner message via the shared chat service.
async fn answer(db: &Db, text: &str) -> anyhow::Result<String> {
    let llm = crate::llm::claude::ClaudeClient::from_env()
        .map_err(|e| anyhow::anyhow!("chat unavailable: {e}"))?;
    crate::service::chat::answer(db, &llm, "telegram", text).await
}

async fn send_or_log(client: &TelegramClient, chat_id: i64, text: &str) {
    if let Err(e) = client.send_message(chat_id, text).await {
        tracing::error!("telegram: sendMessage to {chat_id} failed: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_chat_gets_answered() {
        assert_eq!(plan_action(Some(42), 42), Action::Answer);
    }

    #[test]
    fn other_chats_are_ignored_once_linked() {
        assert_eq!(plan_action(Some(42), 99), Action::Ignore);
    }

    #[test]
    fn unlinked_messages_attempt_the_link_code() {
        assert_eq!(plan_action(None, 99), Action::TryLink);
    }
}
