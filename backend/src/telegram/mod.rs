//! Telegram bot channel: linking state, Bot API client, and the polling loop.
//!
//! The poller is spawned from main() only when TELEGRAM_BOT_TOKEN is set. It
//! long-polls getUpdates and answers messages from the linked owner chat via
//! the assistant agent (tool-use loop). Messages from unlinked chats are only
//! ever used for the one-time link-code handshake.

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
use crate::ingestion::extract::ExtractedEntry;
use crate::ingestion::review::ConfirmPayload;
use crate::repo::review_items::ReviewItemRow;
use client::{TelegramClient, TgCallbackQuery, TgError, TgMessage, TgUpdate};
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

/// A parsed inline-button press.
#[derive(Debug, PartialEq, Eq)]
pub enum CallbackAction {
    Confirm(i64),
    Reject(i64),
    /// "✅ Selesai" on a reminder notification: mark its todo done.
    TodoDone(i64),
}

/// Parse callback_data ("confirm:<review_id>" / "reject:<review_id>" /
/// "tododone:<todo_id>").
pub fn parse_callback(data: &str) -> Option<CallbackAction> {
    let (action, id) = data.split_once(':')?;
    let id: i64 = id.parse().ok()?;
    match action {
        "confirm" => Some(CallbackAction::Confirm(id)),
        "reject" => Some(CallbackAction::Reject(id)),
        "tododone" => Some(CallbackAction::TodoDone(id)),
        _ => None,
    }
}

/// Coerce a payload date into RFC3339: full RFC3339 passes through,
/// "YYYY-MM-DDTHH:MM" and date-only values are assumed UTC.
fn to_rfc3339(s: &str) -> Option<String> {
    if chrono::DateTime::parse_from_rfc3339(s).is_ok() {
        return Some(s.to_string());
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M") {
        return Some(format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S")));
    }
    if let Ok(d) = chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Some(format!("{d}T00:00:00Z"));
    }
    None
}

/// Build the ConfirmPayload for one-tap confirmation, or explain (in user
/// language) why the item must be completed in the web UI instead.
pub fn build_confirm_payload(item: &ReviewItemRow) -> Result<ConfirmPayload, String> {
    if item.needs_attention != 0 {
        return Err("item ini perlu dicek manual".into());
    }
    let account_id = item.suggested_account_id.ok_or("akun belum dikenali")?;
    let instrument_id = item.suggested_instrument_id.ok_or("instrumen belum dikenali")?;
    let entry: ExtractedEntry = serde_json::from_str(&item.payload_json)
        .map_err(|e| format!("payload tidak terbaca: {e}"))?;
    // Amount-only fund entries (an IDR amount, no units/NAV — e.g. Bibit) are
    // one-tap confirmable: confirm() derives NAV units when a stored bibit
    // quote exists, else records quantity = amount at price 1, for buy/sell.
    let amount_only = entry.quantity.is_none()
        && entry.price_native.is_none()
        && entry.amount_native.is_some()
        && matches!(entry.entry_type.as_str(), "buy" | "sell");
    let (quantity, price_native) = if amount_only {
        (String::new(), String::new())
    } else {
        (
            entry.quantity.ok_or("jumlah tidak ada")?,
            entry.price_native.ok_or("harga tidak ada")?,
        )
    };
    let currency = entry.currency.ok_or("mata uang tidak ada")?;
    let executed_at = match &entry.executed_at {
        Some(raw) => to_rfc3339(raw).ok_or_else(|| format!("tanggal tidak terbaca: {raw}"))?,
        // Mirrors the web UI, which defaults the executed-at field to now.
        None => chrono::Utc::now().to_rfc3339(),
    };
    Ok(ConfirmPayload {
        account_id,
        instrument_id,
        entry_type: entry.entry_type,
        executed_at,
        quantity,
        price_native,
        fee_native: entry.fee_native,
        currency,
        fx_to_idr: None,
        fx_to_usd: None,
        note: entry.note,
        amount_native: entry.amount_native,
    })
}

/// Format a payload number with Indonesian separators, falling back to the
/// raw string when it isn't a clean decimal.
fn fmt_payload_num(raw: &str) -> String {
    use std::str::FromStr;
    rust_decimal::Decimal::from_str(raw)
        .map(|d| crate::service::chat::group_id(&d))
        .unwrap_or_else(|_| raw.to_string())
}

/// One-message summary of a staged review item for the confirmation prompt.
fn item_summary(
    item: &ReviewItemRow,
    entry: Option<&ExtractedEntry>,
    account: Option<&str>,
    instrument: Option<&str>,
) -> String {
    let mut out = format!("🧾 Review #{}", item.id);
    if let Some(e) = entry {
        out.push_str(&format!(" — {}", e.entry_type));
        if let Some(symbol) = &e.symbol {
            out.push_str(&format!(" {symbol}"));
        }
        out.push('\n');
        if let Some(qty) = &e.quantity {
            out.push_str(&format!("Qty: {}\n", fmt_payload_num(qty)));
        }
        if let Some(price) = &e.price_native {
            let currency = e.currency.as_deref().unwrap_or("");
            out.push_str(&format!("Harga: {currency} {}\n", fmt_payload_num(price)));
        }
        // Amount-only fund entries have no qty/price; show the nominal instead.
        if e.quantity.is_none() && e.price_native.is_none() {
            if let Some(amount) = &e.amount_native {
                let currency = e.currency.as_deref().unwrap_or("");
                out.push_str(&format!("Nominal: {currency} {}\n", fmt_payload_num(amount)));
            }
        }
        out.push_str(&format!(
            "Tanggal: {}\n",
            e.executed_at.as_deref().unwrap_or("hari ini")
        ));
    } else {
        out.push('\n');
    }
    if let Some(name) = account {
        out.push_str(&format!("Akun: {name}\n"));
    }
    if let Some(label) = instrument {
        out.push_str(&format!("Instrumen: {label}\n"));
    }
    out.trim_end().to_string()
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
    "✅ Telegram tertaut. Aku bisa bantu catat todo, pasang pengingat, dan jawab pertanyaan soal portofoliomu.";
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
    if let Some(callback) = update.callback_query {
        handle_callback(client, db, callback).await;
        return;
    }
    // Ignore other non-message updates.
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
                match ingest_attachment(client, db, &attachment).await {
                    Ok(items) => send_review_prompts(client, db, chat_id, &items).await,
                    Err(e) => {
                        tracing::error!("telegram: ingest failed: {e:#}");
                        send_or_log(client, chat_id, INGEST_FAILED_REPLY).await;
                    }
                }
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

/// Answer a linked owner message via the assistant agent (tool-use loop).
async fn answer(db: &Db, text: &str) -> anyhow::Result<String> {
    let llm = crate::llm::claude::ClaudeClient::from_env()
        .map_err(|e| anyhow::anyhow!("chat unavailable: {e}"))?;
    crate::assistant::agent::handle_message(db, &llm, "telegram", text).await
}

/// Download an attachment and run it through the shared ingestion pipeline
/// (same path as a web upload). Returns the staged review items.
async fn ingest_attachment(
    client: &TelegramClient,
    db: &Db,
    attachment: &Attachment,
) -> anyhow::Result<Vec<ReviewItemRow>> {
    use base64::Engine;
    let file_path = client.get_file_path(&attachment.file_id).await?;
    let bytes = client.download_file(&file_path).await?;
    let data_base64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let llm = crate::llm::native::NativeLlmClient::from_env()
        .map_err(|e| anyhow::anyhow!("ingest unavailable: {e}"))?;
    let batch_id = format!("tg-{}", chrono::Utc::now().timestamp_millis());
    let upload = crate::ingestion::ingest::UploadFile {
        filename: attachment.filename.clone(),
        media_type: attachment.media_type.clone(),
        data_base64,
    };
    let result = crate::ingestion::ingest::ingest_batch(db, &llm, &batch_id, &[upload]).await?;
    Ok(result.items)
}

/// Send one confirmation prompt per staged item: confirmable items get
/// Konfirmasi/Tolak buttons, incomplete ones get Tolak plus a web-UI note.
async fn send_review_prompts(
    client: &TelegramClient,
    db: &Db,
    chat_id: i64,
    items: &[ReviewItemRow],
) {
    if items.is_empty() {
        send_or_log(client, chat_id, "Tidak ada transaksi yang terbaca dari file itu.").await;
        return;
    }
    for item in items {
        let entry: Option<ExtractedEntry> = serde_json::from_str(&item.payload_json).ok();
        let account_name = match item.suggested_account_id {
            Some(id) => crate::repo::accounts::get(db, id).await.ok().map(|a| a.name),
            None => None,
        };
        let instrument_label = match item.suggested_instrument_id {
            Some(id) => crate::repo::instruments::get(db, id)
                .await
                .ok()
                .map(|i| format!("{} ({})", i.symbol, i.name)),
            None => None,
        };
        let summary = item_summary(
            item,
            entry.as_ref(),
            account_name.as_deref(),
            instrument_label.as_deref(),
        );
        let confirm_data = format!("confirm:{}", item.id);
        let reject_data = format!("reject:{}", item.id);
        let send_result = match build_confirm_payload(item) {
            Ok(_) => {
                client
                    .send_message_with_buttons(
                        chat_id,
                        &summary,
                        &[("✅ Konfirmasi", &confirm_data), ("❌ Tolak", &reject_data)],
                    )
                    .await
            }
            Err(reason) => {
                let text = format!("{summary}\n\n⚠️ {reason} — lengkapi di web UI → Data.");
                client
                    .send_message_with_buttons(chat_id, &text, &[("❌ Tolak", &reject_data)])
                    .await
            }
        };
        if let Err(e) = send_result {
            tracing::error!("telegram: review prompt for #{} failed: {e:#}", item.id);
        }
    }
}

/// Handle an inline-button press: only the linked owner chat may act, the
/// existing review confirm/reject guards prevent double-processing, and the
/// prompt message is edited in place (which also retires its buttons).
async fn handle_callback(client: &TelegramClient, db: &Db, callback: TgCallbackQuery) {
    // Always acknowledge first so the client stops its loading spinner.
    if let Err(e) = client.answer_callback_query(&callback.id).await {
        tracing::warn!("telegram: answerCallbackQuery failed: {e:#}");
    }
    let Some(message) = callback.message else { return };
    let chat_id = message.chat.id;
    let linked = match crate::repo::telegram_link::get(db).await {
        Ok(row) => row.map(|r| r.chat_id),
        Err(e) => {
            tracing::error!("telegram: failed to read link row: {e:#}");
            return;
        }
    };
    if linked != Some(chat_id) {
        return;
    }
    let Some(action) = callback.data.as_deref().and_then(parse_callback) else { return };
    let text = match action {
        CallbackAction::Confirm(item_id) => {
            review_callback_text(item_id, confirm_item(db, item_id).await)
        }
        CallbackAction::Reject(item_id) => {
            review_callback_text(item_id, reject_item(db, item_id).await)
        }
        CallbackAction::TodoDone(todo_id) => todo_done_text(db, todo_id).await,
    };
    if let Err(e) = client.edit_message_text(chat_id, message.message_id, &text).await {
        tracing::error!("telegram: editMessageText failed: {e:#}");
    }
}

/// Confirm a staged item using its suggested account/instrument.
async fn confirm_item(db: &Db, item_id: i64) -> anyhow::Result<String> {
    let item = crate::repo::review_items::get(db, item_id).await?;
    let payload = build_confirm_payload(&item)
        .map_err(|reason| anyhow::anyhow!("{reason} — lengkapi di web UI → Data"))?;
    let txn_id = crate::ingestion::review::confirm(db, item_id, &payload).await?;
    Ok(format!("✅ Transaksi #{txn_id} dibuat."))
}

async fn reject_item(db: &Db, item_id: i64) -> anyhow::Result<String> {
    crate::ingestion::review::reject(db, item_id).await?;
    Ok("❌ Ditolak.".to_string())
}

/// Result line for review confirm/reject button presses.
fn review_callback_text(item_id: i64, outcome: anyhow::Result<String>) -> String {
    let status = outcome.unwrap_or_else(|e| format!("⚠️ {e:#}"));
    format!("🧾 Review #{item_id} — {status}")
}

/// Result line for the "✅ Selesai" button on a reminder notification.
async fn todo_done_text(db: &Db, todo_id: i64) -> String {
    match crate::repo::todos::complete(db, todo_id).await {
        Ok(true) => format!("✅ Todo #{todo_id} selesai."),
        Ok(false) => format!("Todo #{todo_id} sudah selesai atau tidak ditemukan."),
        Err(e) => format!("⚠️ {e:#}"),
    }
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

    // ── Inline confirmation ────────────────────────────────────────────────

    #[test]
    fn parses_confirm_and_reject_callbacks() {
        assert_eq!(parse_callback("confirm:42"), Some(CallbackAction::Confirm(42)));
        assert_eq!(parse_callback("reject:7"), Some(CallbackAction::Reject(7)));
        assert_eq!(parse_callback("tododone:9"), Some(CallbackAction::TodoDone(9)));
        assert_eq!(parse_callback("nope:1"), None);
        assert_eq!(parse_callback("confirm:abc"), None);
        assert_eq!(parse_callback("confirm"), None);
    }

    #[tokio::test]
    async fn todo_done_text_completes_open_todos_once() {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        let todo = crate::repo::todos::create(&db, "bayar listrik", None, None).await.unwrap();
        let first = todo_done_text(&db, todo.id).await;
        assert!(first.contains("selesai"), "{first}");
        let again = todo_done_text(&db, todo.id).await;
        assert!(again.contains("sudah") || again.contains("tidak ditemukan"), "{again}");
    }

    #[test]
    fn coerces_dates_to_rfc3339() {
        assert_eq!(to_rfc3339("2026-06-04T11:32:00Z").as_deref(), Some("2026-06-04T11:32:00Z"));
        assert_eq!(to_rfc3339("2026-06-04T11:32").as_deref(), Some("2026-06-04T11:32:00Z"));
        assert_eq!(to_rfc3339("2026-06-04").as_deref(), Some("2026-06-04T00:00:00Z"));
        assert_eq!(to_rfc3339("kemarin"), None);
    }

    fn review_item(payload_json: &str) -> crate::repo::review_items::ReviewItemRow {
        crate::repo::review_items::ReviewItemRow {
            id: 42,
            batch_id: "tg-1".into(),
            source_kind: "image".into(),
            source_filename: "telegram-photo.jpg".into(),
            source_path: "".into(),
            doc_type: "txn_history".into(),
            status: "pending".into(),
            needs_attention: 0,
            payload_json: payload_json.into(),
            raw_llm_json: "{}".into(),
            suggested_instrument_id: Some(9),
            suggested_account_id: Some(2),
            created_txn_id: None,
            created_at: "2026-06-05T00:00:00Z".into(),
            confirmed_at: None,
        }
    }

    const FULL_PAYLOAD: &str = r#"{
        "entry_type": "buy", "symbol": "BTC", "quantity": "0.00128248",
        "price_native": "1169608882", "fee_native": "0", "currency": "IDR",
        "executed_at": "2026-06-04", "confidence": 0.95
    }"#;

    #[test]
    fn full_items_build_a_confirm_payload() {
        let payload = build_confirm_payload(&review_item(FULL_PAYLOAD)).expect("confirmable");
        assert_eq!(payload.account_id, 2);
        assert_eq!(payload.instrument_id, 9);
        assert_eq!(payload.entry_type, "buy");
        assert_eq!(payload.quantity, "0.00128248");
        assert_eq!(payload.executed_at, "2026-06-04T00:00:00Z");
        assert_eq!(payload.currency, "IDR");
    }

    #[test]
    fn attention_items_are_not_confirmable() {
        let mut item = review_item(FULL_PAYLOAD);
        item.needs_attention = 1;
        assert!(build_confirm_payload(&item).is_err());
    }

    #[test]
    fn items_without_suggestions_are_not_confirmable() {
        let mut item = review_item(FULL_PAYLOAD);
        item.suggested_account_id = None;
        assert!(build_confirm_payload(&item).is_err());

        let mut item = review_item(FULL_PAYLOAD);
        item.suggested_instrument_id = None;
        assert!(build_confirm_payload(&item).is_err());
    }

    #[test]
    fn items_missing_core_fields_are_not_confirmable() {
        let payload = r#"{ "entry_type": "buy", "symbol": "BTC", "confidence": 0.95 }"#;
        assert!(build_confirm_payload(&review_item(payload)).is_err());
    }

    const AMOUNT_ONLY_PAYLOAD: &str = r#"{
        "entry_type": "buy", "instrument_name": "Sucorinvest Bond Fund",
        "amount_native": "13000000", "currency": "IDR", "confidence": 0.72
    }"#;

    #[test]
    fn amount_only_fund_items_build_a_confirm_payload() {
        // Bibit-style: nominal only, no units/NAV. confirm() maps the amount to
        // quantity = amount at price 1, so empty qty/price are intentional here.
        let payload =
            build_confirm_payload(&review_item(AMOUNT_ONLY_PAYLOAD)).expect("confirmable");
        assert_eq!(payload.quantity, "");
        assert_eq!(payload.price_native, "");
        assert_eq!(payload.amount_native.as_deref(), Some("13000000"));
        assert_eq!(payload.currency, "IDR");
    }

    #[test]
    fn amount_only_dividend_is_not_confirmable() {
        let payload = r#"{ "entry_type": "dividend", "amount_native": "100000", "currency": "IDR", "confidence": 0.9 }"#;
        assert!(build_confirm_payload(&review_item(payload)).is_err());
    }

    #[test]
    fn amount_only_summary_shows_the_nominal() {
        let item = review_item(AMOUNT_ONLY_PAYLOAD);
        let entry: crate::ingestion::extract::ExtractedEntry =
            serde_json::from_str(AMOUNT_ONLY_PAYLOAD).unwrap();
        let summary = item_summary(
            &item,
            Some(&entry),
            Some("Pendidikan Noah"),
            Some("Sucorinvest Bond Fund"),
        );
        assert!(summary.contains("Nominal: IDR 13.000.000"), "{summary}");
        assert!(!summary.contains("Qty:"), "{summary}");
    }

    #[test]
    fn summary_shows_the_extracted_details() {
        let item = review_item(FULL_PAYLOAD);
        let entry: crate::ingestion::extract::ExtractedEntry =
            serde_json::from_str(FULL_PAYLOAD).unwrap();
        let summary = item_summary(&item, Some(&entry), Some("Pluang"), Some("BTC (Bitcoin)"));
        assert!(summary.contains("Review #42"), "{summary}");
        assert!(summary.contains("buy BTC"), "{summary}");
        assert!(summary.contains("0,00128248"), "{summary}");
        assert!(summary.contains("1.169.608.882"), "{summary}");
        assert!(summary.contains("Akun: Pluang"), "{summary}");
        assert!(summary.contains("Instrumen: BTC (Bitcoin)"), "{summary}");
    }
}
