//! Public-channel services that do not need an LLM: starting a session (lead
//! capture) and loading a conversation transcript for the widget to restore.

use crate::db::Db;
use serde::Serialize;

#[derive(Serialize)]
pub struct StartedSession {
    pub session_token: String,
}

#[derive(Serialize)]
pub struct HistoryMessage {
    pub role: String,
    pub content: String,
    pub created_at: String,
}

/// Start a web CS conversation with lead capture. Requires a name AND at least
/// one contact (email or phone) — the pre-chat form guarantees this; we enforce
/// it server-side too.
pub async fn start_session(
    db: &Db,
    name: &str,
    email: Option<&str>,
    phone: Option<&str>,
) -> anyhow::Result<StartedSession> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("name is required");
    }
    let has_contact = email.map(|e| !e.trim().is_empty()).unwrap_or(false)
        || phone.map(|p| !p.trim().is_empty()).unwrap_or(false);
    if !has_contact {
        anyhow::bail!("an email or phone is required");
    }
    let token = crate::cs::gate::new_session_token();
    crate::repo::cs::conversation_create(db, "web", Some(name), email, phone, &token).await?;
    Ok(StartedSession { session_token: token })
}

/// Load the transcript for a session token. Errors if the token is unknown.
pub async fn load_history(db: &Db, token: &str) -> anyhow::Result<Vec<HistoryMessage>> {
    let conv = crate::repo::cs::conversation_by_token(db, token)
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown session"))?;
    let rows = crate::repo::cs::message_all(db, conv.id).await?;
    Ok(rows
        .into_iter()
        .filter(|m| m.role == "user" || m.role == "assistant")
        .map(|m| HistoryMessage { role: m.role, content: m.content, created_at: m.created_at })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn start_session_requires_name_and_a_contact() {
        let db = mem_db().await;
        // missing name
        assert!(start_session(&db, "", Some("a@x.com"), None).await.is_err());
        // missing both contacts
        assert!(start_session(&db, "Budi", None, None).await.is_err());
        // ok with email
        let s = start_session(&db, "Budi", Some("a@x.com"), None).await.unwrap();
        assert!(!s.session_token.is_empty());
        // ok with phone
        assert!(start_session(&db, "Ani", None, Some("0812")).await.is_ok());
    }

    #[tokio::test]
    async fn start_session_persists_conversation_resolvable_by_token() {
        let db = mem_db().await;
        let s = start_session(&db, "Budi", Some("a@x.com"), None).await.unwrap();
        let conv = crate::repo::cs::conversation_by_token(&db, &s.session_token).await.unwrap();
        assert!(conv.is_some());
        assert_eq!(conv.unwrap().visitor_name.as_deref(), Some("Budi"));
    }

    #[tokio::test]
    async fn load_history_returns_messages_for_token() {
        let db = mem_db().await;
        let s = start_session(&db, "Budi", Some("a@x.com"), None).await.unwrap();
        let conv = crate::repo::cs::conversation_by_token(&db, &s.session_token).await.unwrap().unwrap();
        crate::repo::cs::message_add(&db, conv.id, "user", "halo").await.unwrap();
        crate::repo::cs::message_add(&db, conv.id, "assistant", "halo juga").await.unwrap();

        let hist = load_history(&db, &s.session_token).await.unwrap();
        assert_eq!(hist.len(), 2);
        assert_eq!(hist[0].role, "user");

        // unknown token -> error (not an empty list, so the widget knows the session is invalid)
        assert!(load_history(&db, "nope").await.is_err());
    }
}
