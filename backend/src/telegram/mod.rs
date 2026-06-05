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
use client::{TelegramClient, TgError, TgMessage, TgUpdate};
use state::SharedTgState;
use std::time::Instant;

/// A downloadable attachment extracted from an inbound message.
#[derive(Debug, PartialEq, Eq)]
pub struct Attachment {
    pub file_id: String,
    pub filename: String,
    pub media_type: String,
}

/// What an inbound message carries for the ingest path.
#[derive(Debug, PartialEq, Eq)]
pub enum AttachmentPick {
    /// An ingestable image or PDF.
    Some(Attachment),
    /// A document we can't extract from (spreadsheets, archives, ...).
    Unsupported,
    /// No attachment — treat as a text message.
    None,
}

/// Pick the ingestable attachment from a message, if any. Photos use the
/// largest available resolution (Telegram photos are always JPEG); documents
/// must be images or PDFs.
pub fn pick_attachment(message: &TgMessage) -> AttachmentPick {
    if let Some(photos) = &message.photo {
        if let Some(best) = photos.iter().max_by_key(|p| p.width * p.height) {
            return AttachmentPick::Some(Attachment {
                file_id: best.file_id.clone(),
                filename: "telegram-photo.jpg".into(),
                media_type: "image/jpeg".into(),
            });
        }
    }
    if let Some(doc) = &message.document {
        let mime = doc.mime_type.as_deref().unwrap_or("");
        if mime.starts_with("image/") || mime == "application/pdf" {
            return AttachmentPick::Some(Attachment {
                file_id: doc.file_id.clone(),
                filename: doc.file_name.clone().unwrap_or_else(|| "telegram-file".into()),
                media_type: mime.to_string(),
            });
        }
        return AttachmentPick::Unsupported;
    }
    AttachmentPick::None
}

const LINK_OK_REPLY: &str =
    "✅ Telegram tertaut. Silakan tanya apa saja tentang portofoliomu.";
const LINK_HINT_REPLY: &str =
    "Kode tidak valid atau kedaluwarsa. Buka halaman Telegram di web UI untuk membuat kode tautan.";
const ANSWER_FAILED_REPLY: &str =
    "Maaf, lagi ada gangguan saat menjawab. Coba lagi sebentar lagi ya.";
const INGEST_FAILED_REPLY: &str =
    "Maaf, gagal memproses file-nya. Coba kirim ulang sebentar lagi ya.";
const UNSUPPORTED_FILE_REPLY: &str =
    "Format file tidak didukung — kirim foto atau PDF ya.";

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
    // Starting at 0 deliberately replays Telegram's buffered backlog (up to
    // 24h) after a restart, so messages sent while the backend was down still
    // get answered.
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
    // Ignore non-message updates.
    let Some(message) = update.message else { return };
    let chat_id = message.chat.id;

    let linked = match crate::repo::telegram_link::get(db).await {
        Ok(row) => row.map(|r| r.chat_id),
        Err(e) => {
            tracing::error!("telegram: failed to read link row: {e:#}");
            return;
        }
    };

    match plan_action(linked, chat_id) {
        Action::Answer => match pick_attachment(&message) {
            AttachmentPick::Some(attachment) => {
                let reply = match ingest_attachment(client, db, &attachment).await {
                    Ok(count) => format!(
                        "📥 {count} item masuk antrian review. Buka web UI → Data untuk konfirmasi."
                    ),
                    Err(e) => {
                        tracing::error!("telegram: ingest failed: {e:#}");
                        INGEST_FAILED_REPLY.to_string()
                    }
                };
                send_or_log(client, chat_id, &reply).await;
            }
            AttachmentPick::Unsupported => {
                send_or_log(client, chat_id, UNSUPPORTED_FILE_REPLY).await;
            }
            AttachmentPick::None => {
                let Some(text) = message.text.as_deref().filter(|t| !t.trim().is_empty()) else {
                    return;
                };
                let reply = answer(db, text).await.unwrap_or_else(|e| {
                    tracing::error!("telegram: answer failed: {e:#}");
                    ANSWER_FAILED_REPLY.to_string()
                });
                send_or_log(client, chat_id, &reply).await;
            }
        },
        Action::TryLink => {
            // The link handshake is text-only; media from unlinked chats is ignored.
            let Some(text) = message.text.as_deref().filter(|t| !t.trim().is_empty()) else {
                return;
            };
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

/// Download an attachment and run it through the shared ingestion pipeline
/// (same path as a web upload). Returns the number of staged review items.
async fn ingest_attachment(
    client: &TelegramClient,
    db: &Db,
    attachment: &Attachment,
) -> anyhow::Result<usize> {
    use base64::Engine;
    let file_path = client.get_file_path(&attachment.file_id).await?;
    let bytes = client.download_file(&file_path).await?;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let llm = crate::llm::claude::ClaudeClient::from_env()
        .map_err(|e| anyhow::anyhow!("ingest unavailable: {e}"))?;
    let batch_id = format!("tg-{}", chrono::Utc::now().timestamp_millis());
    let upload = crate::ingestion::ingest::UploadFile {
        filename: attachment.filename.clone(),
        media_type: attachment.media_type.clone(),
        data_base64,
    };
    let result = crate::ingestion::ingest::ingest_batch(db, &llm, &batch_id, &[upload]).await?;
    Ok(result.items.len())
}

async fn send_or_log(client: &TelegramClient, chat_id: i64, text: &str) {
    if let Err(e) = client.send_message(chat_id, text).await {
        tracing::error!("telegram: sendMessage to {chat_id} failed: {e:#}");
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

    use client::{TgChat, TgDocument, TgMessage, TgPhotoSize};

    fn bare_message() -> TgMessage {
        TgMessage { chat: TgChat { id: 1 }, from: None, text: None, photo: None, document: None }
    }

    #[test]
    fn picks_the_largest_photo() {
        let mut msg = bare_message();
        msg.photo = Some(vec![
            TgPhotoSize { file_id: "small".into(), width: 90, height: 160 },
            TgPhotoSize { file_id: "big".into(), width: 720, height: 1280 },
            TgPhotoSize { file_id: "mid".into(), width: 320, height: 568 },
        ]);
        let AttachmentPick::Some(att) = pick_attachment(&msg) else { panic!("expected attachment") };
        assert_eq!(att.file_id, "big");
        assert_eq!(att.media_type, "image/jpeg");
    }

    #[test]
    fn accepts_image_and_pdf_documents() {
        for (mime, name) in [("application/pdf", "statement.pdf"), ("image/png", "shot.png")] {
            let mut msg = bare_message();
            msg.document = Some(TgDocument {
                file_id: "doc".into(),
                file_name: Some(name.into()),
                mime_type: Some(mime.into()),
            });
            let AttachmentPick::Some(att) = pick_attachment(&msg) else { panic!("{mime} must be accepted") };
            assert_eq!(att.filename, name);
            assert_eq!(att.media_type, mime);
        }
    }

    #[test]
    fn rejects_unsupported_documents() {
        let mut msg = bare_message();
        msg.document = Some(TgDocument {
            file_id: "doc".into(),
            file_name: Some("data.xlsx".into()),
            mime_type: Some("application/vnd.ms-excel".into()),
        });
        assert_eq!(pick_attachment(&msg), AttachmentPick::Unsupported);
    }

    #[test]
    fn text_messages_have_no_attachment() {
        let mut msg = bare_message();
        msg.text = Some("berapa net worth saya?".into());
        assert_eq!(pick_attachment(&msg), AttachmentPick::None);
    }
}
