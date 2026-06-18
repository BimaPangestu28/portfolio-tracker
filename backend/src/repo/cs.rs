use crate::db::Db;
use serde::Serialize;

// --- Conversation ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ConversationRow {
    pub id: i64,
    pub channel: String,
    pub visitor_name: Option<String>,
    pub visitor_email: Option<String>,
    pub visitor_phone: Option<String>,
    pub session_token: String,
    pub status: String,
    pub created_at: String,
    pub last_msg_at: String,
    pub wa_jid: Option<String>,
}

pub async fn conversation_create(
    db: &Db,
    channel: &str,
    name: Option<&str>,
    email: Option<&str>,
    phone: Option<&str>,
    session_token: &str,
) -> anyhow::Result<ConversationRow> {
    if !matches!(channel, "web" | "whatsapp") {
        anyhow::bail!("invalid channel '{}': must be 'web' or 'whatsapp'", channel);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_conversation \
         (channel, visitor_name, visitor_email, visitor_phone, session_token, status, created_at, last_msg_at) \
         VALUES (?, ?, ?, ?, ?, 'bot', ?, ?)",
    )
    .bind(channel)
    .bind(name)
    .bind(email)
    .bind(phone)
    .bind(session_token)
    .bind(&now)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, ConversationRow>("SELECT * FROM cs_conversation WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn conversation_by_token(db: &Db, token: &str) -> anyhow::Result<Option<ConversationRow>> {
    let row = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation WHERE session_token = ?",
    )
    .bind(token)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

pub async fn conversation_set_status(db: &Db, id: i64, status: &str) -> anyhow::Result<()> {
    if !matches!(status, "bot" | "needs_human" | "resolved") {
        anyhow::bail!("invalid status '{}'", status);
    }
    let res = sqlx::query("UPDATE cs_conversation SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(db)
        .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("conversation {id} not found");
    }
    Ok(())
}

pub async fn conversation_touch(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE cs_conversation SET last_msg_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("conversation {id} not found");
    }
    Ok(())
}

/// Look up a single conversation by its row id. Returns None when not found.
pub async fn conversation_get(db: &Db, id: i64) -> anyhow::Result<Option<ConversationRow>> {
    let row = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Conversations most-recently-active first — for the admin CS inbox.
pub async fn conversation_list_recent(db: &Db, limit: i64) -> anyhow::Result<Vec<ConversationRow>> {
    let rows = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation ORDER BY last_msg_at DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// Look up a WhatsApp CS conversation by the sender JID. Returns None if no
/// conversation for this JID exists yet.
pub async fn conversation_by_wa_jid(db: &Db, jid: &str) -> anyhow::Result<Option<ConversationRow>> {
    let row = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation WHERE wa_jid = ?",
    )
    .bind(jid)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// Create a new WhatsApp CS conversation for a sender JID. The `session_token`
/// is caller-supplied (the row needs one for uniqueness); `wa_jid` is the
/// per-sender lookup key.
pub async fn conversation_create_wa(
    db: &Db,
    jid: &str,
    phone: &str,
    session_token: &str,
) -> anyhow::Result<ConversationRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_conversation \
         (channel, visitor_name, visitor_email, visitor_phone, session_token, \
          status, created_at, last_msg_at, wa_jid) \
         VALUES ('whatsapp', NULL, NULL, ?, ?, 'bot', ?, ?, ?)",
    )
    .bind(phone)
    .bind(session_token)
    .bind(&now)
    .bind(&now)
    .bind(jid)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, ConversationRow>(
        "SELECT * FROM cs_conversation WHERE id = ?",
    )
    .bind(id)
    .fetch_one(db)
    .await?;
    Ok(row)
}

// --- Message ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MessageRow {
    pub id: i64,
    pub conversation_id: i64,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

pub async fn message_add(db: &Db, conversation_id: i64, role: &str, content: &str) -> anyhow::Result<MessageRow> {
    if !matches!(role, "user" | "assistant" | "system") {
        anyhow::bail!("invalid role '{}'", role);
    }
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_message (conversation_id, role, content, created_at) VALUES (?, ?, ?, ?)",
    )
    .bind(conversation_id)
    .bind(role)
    .bind(content)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, MessageRow>("SELECT * FROM cs_message WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

/// Full transcript for one conversation, oldest first.
pub async fn message_all(db: &Db, conversation_id: i64) -> anyhow::Result<Vec<MessageRow>> {
    let rows = sqlx::query_as::<_, MessageRow>(
        "SELECT * FROM cs_message WHERE conversation_id = ? ORDER BY id ASC",
    )
    .bind(conversation_id)
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// COUNT(*) of messages in a conversation — cheaper than fetching all rows.
pub async fn message_count(db: &Db, conversation_id: i64) -> anyhow::Result<i64> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cs_message WHERE conversation_id = ?",
    )
    .bind(conversation_id)
    .fetch_one(db)
    .await?;
    Ok(count)
}

/// Last `limit` messages for one conversation, oldest first — LLM context window.
pub async fn message_recent(db: &Db, conversation_id: i64, limit: i64) -> anyhow::Result<Vec<MessageRow>> {
    let mut rows = sqlx::query_as::<_, MessageRow>(
        "SELECT * FROM cs_message WHERE conversation_id = ? ORDER BY id DESC LIMIT ?",
    )
    .bind(conversation_id)
    .bind(limit)
    .fetch_all(db)
    .await?;
    rows.reverse();
    Ok(rows)
}

// --- KB doc + chunk ---

/// Encode an f32 vector as little-endian bytes for BLOB storage.
pub fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 4);
    for f in v {
        out.extend_from_slice(&f.to_le_bytes());
    }
    out
}

/// Decode a little-endian f32 BLOB back into a vector. Trailing bytes (not a
/// multiple of 4) are ignored.
pub fn blob_to_embedding(b: &[u8]) -> Vec<f32> {
    b.chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct KbDocRow {
    pub id: i64,
    pub title: String,
    pub source: Option<String>,
    pub body: String,
    pub updated_at: String,
}

/// A chunk + decoded embedding, ready for similarity search.
pub struct KbChunkVec {
    pub id: i64,
    pub doc_id: i64,
    pub text: String,
    pub vector: Vec<f32>,
}

pub async fn kb_doc_insert(db: &Db, title: &str, source: Option<&str>, body: &str) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query("INSERT INTO cs_kb_doc (title, source, body, updated_at) VALUES (?, ?, ?, ?)")
        .bind(title)
        .bind(source)
        .bind(body)
        .bind(&now)
        .execute(db)
        .await?
        .last_insert_rowid();
    Ok(id)
}

pub async fn kb_doc_update(db: &Db, id: i64, title: &str, source: Option<&str>, body: &str) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE cs_kb_doc SET title = ?, source = ?, body = ?, updated_at = ? WHERE id = ?")
        .bind(title)
        .bind(source)
        .bind(body)
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("kb_doc {id} not found");
    }
    Ok(())
}

pub async fn kb_doc_delete(db: &Db, id: i64) -> anyhow::Result<()> {
    // Explicit chunk delete first, so the test passes regardless of PRAGMA state.
    sqlx::query("DELETE FROM cs_kb_chunk WHERE doc_id = ?").bind(id).execute(db).await?;
    sqlx::query("DELETE FROM cs_kb_doc WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

pub async fn kb_doc_list(db: &Db) -> anyhow::Result<Vec<KbDocRow>> {
    let rows = sqlx::query_as::<_, KbDocRow>("SELECT * FROM cs_kb_doc ORDER BY updated_at DESC")
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Replace all chunks for a doc: delete existing, insert fresh ones with NULL embedding.
pub async fn kb_replace_chunks(db: &Db, doc_id: i64, texts: &[String]) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("DELETE FROM cs_kb_chunk WHERE doc_id = ?").bind(doc_id).execute(db).await?;
    for text in texts {
        sqlx::query("INSERT INTO cs_kb_chunk (doc_id, text, embedding, updated_at) VALUES (?, ?, NULL, ?)")
            .bind(doc_id)
            .bind(text)
            .bind(&now)
            .execute(db)
            .await?;
    }
    Ok(())
}

/// Chunks that still need an embedding computed. Returns (chunk_id, text).
pub async fn kb_chunks_without_embedding(db: &Db) -> anyhow::Result<Vec<(i64, String)>> {
    let rows: Vec<(i64, String)> = sqlx::query_as(
        "SELECT id, text FROM cs_kb_chunk WHERE embedding IS NULL ORDER BY id ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn kb_set_chunk_embedding(db: &Db, chunk_id: i64, blob: &[u8]) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query("UPDATE cs_kb_chunk SET embedding = ?, updated_at = ? WHERE id = ?")
        .bind(blob)
        .bind(&now)
        .bind(chunk_id)
        .execute(db)
        .await?;
    Ok(())
}

/// All chunks that have an embedding, decoded for cosine search.
pub async fn kb_chunks_with_embedding(db: &Db) -> anyhow::Result<Vec<KbChunkVec>> {
    let rows: Vec<(i64, i64, String, Vec<u8>)> = sqlx::query_as(
        "SELECT id, doc_id, text, embedding FROM cs_kb_chunk WHERE embedding IS NOT NULL ORDER BY id ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, doc_id, text, blob)| KbChunkVec {
            id,
            doc_id,
            text,
            vector: blob_to_embedding(&blob),
        })
        .collect())
}

// --- Product ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ProductRow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub availability: Option<String>,
    pub active: i64,
    pub updated_at: String,
}

pub async fn product_insert(
    db: &Db,
    name: &str,
    description: Option<&str>,
    price: Option<f64>,
    currency: Option<&str>,
    availability: Option<&str>,
) -> anyhow::Result<i64> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_product (name, description, price, currency, availability, active, updated_at) \
         VALUES (?, ?, ?, ?, ?, 1, ?)",
    )
    .bind(name)
    .bind(description)
    .bind(price)
    .bind(currency)
    .bind(availability)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();
    Ok(id)
}

pub async fn product_set_active(db: &Db, id: i64, active: bool) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE cs_product SET active = ?, updated_at = ? WHERE id = ?")
        .bind(if active { 1 } else { 0 })
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("product {id} not found");
    }
    Ok(())
}

pub async fn product_list_active(db: &Db) -> anyhow::Result<Vec<ProductRow>> {
    let rows = sqlx::query_as::<_, ProductRow>(
        "SELECT * FROM cs_product WHERE active = 1 ORDER BY name ASC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

/// All products including inactive — for the admin pricing manager.
pub async fn product_list_all(db: &Db) -> anyhow::Result<Vec<ProductRow>> {
    let rows = sqlx::query_as::<_, ProductRow>("SELECT * FROM cs_product ORDER BY name ASC")
        .fetch_all(db)
        .await?;
    Ok(rows)
}

pub async fn product_update(
    db: &Db,
    id: i64,
    name: &str,
    description: Option<&str>,
    price: Option<f64>,
    currency: Option<&str>,
    availability: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query(
        "UPDATE cs_product SET name = ?, description = ?, price = ?, currency = ?, availability = ?, updated_at = ? WHERE id = ?",
    )
    .bind(name)
    .bind(description)
    .bind(price)
    .bind(currency)
    .bind(availability)
    .bind(&now)
    .bind(id)
    .execute(db)
    .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("product {id} not found");
    }
    Ok(())
}

pub async fn product_delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM cs_product WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

// --- Order ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OrderRow {
    pub id: i64,
    pub external_ref: String,
    pub customer_name: Option<String>,
    pub customer_contact: Option<String>,
    pub status: String,
    pub details_json: Option<String>,
    pub updated_at: String,
}

/// Insert or update by `external_ref` (UNIQUE). Owner-populated.
pub async fn order_upsert(
    db: &Db,
    external_ref: &str,
    customer_name: Option<&str>,
    customer_contact: Option<&str>,
    status: &str,
    details_json: Option<&str>,
) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO cs_order (external_ref, customer_name, customer_contact, status, details_json, updated_at) \
         VALUES (?, ?, ?, ?, ?, ?) \
         ON CONFLICT(external_ref) DO UPDATE SET \
           customer_name = excluded.customer_name, \
           customer_contact = excluded.customer_contact, \
           status = excluded.status, \
           details_json = excluded.details_json, \
           updated_at = excluded.updated_at",
    )
    .bind(external_ref)
    .bind(customer_name)
    .bind(customer_contact)
    .bind(status)
    .bind(details_json)
    .bind(&now)
    .execute(db)
    .await?;
    Ok(())
}

/// Guarded lookup: only returns the order when BOTH the ref and the contact match
/// (contact compared case-insensitively). Prevents enumeration by ref alone.
///
/// Orders whose `customer_contact` is NULL are intentionally unreachable via this
/// function — a customer cannot prove ownership without a contact to match against.
/// This is part of the anti-enumeration guard, not a bug: NULL-contact orders can
/// only be accessed through the admin `order_list` path.
pub async fn order_lookup(db: &Db, external_ref: &str, contact: &str) -> anyhow::Result<Option<OrderRow>> {
    let row = sqlx::query_as::<_, OrderRow>(
        "SELECT * FROM cs_order WHERE external_ref = ? AND LOWER(customer_contact) = LOWER(?)",
    )
    .bind(external_ref)
    .bind(contact)
    .fetch_optional(db)
    .await?;
    Ok(row)
}

/// All orders — for the admin orders manager.
pub async fn order_list(db: &Db, limit: i64) -> anyhow::Result<Vec<OrderRow>> {
    let rows = sqlx::query_as::<_, OrderRow>("SELECT * FROM cs_order ORDER BY updated_at DESC LIMIT ?")
        .bind(limit)
        .fetch_all(db)
        .await?;
    Ok(rows)
}

/// Fetch a single order by its row id.
pub async fn order_get(db: &Db, id: i64) -> anyhow::Result<Option<OrderRow>> {
    let row = sqlx::query_as::<_, OrderRow>("SELECT * FROM cs_order WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

pub async fn order_delete(db: &Db, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM cs_order WHERE id = ?").bind(id).execute(db).await?;
    Ok(())
}

// --- Escalation ---

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct EscalationRow {
    pub id: i64,
    pub conversation_id: i64,
    pub reason: String,
    pub summary: String,
    pub status: String,
    pub created_at: String,
    pub handled_at: Option<String>,
}

pub async fn escalation_create(db: &Db, conversation_id: i64, reason: &str, summary: &str) -> anyhow::Result<EscalationRow> {
    let now = chrono::Utc::now().to_rfc3339();
    let id = sqlx::query(
        "INSERT INTO cs_escalation (conversation_id, reason, summary, status, created_at) \
         VALUES (?, ?, ?, 'open', ?)",
    )
    .bind(conversation_id)
    .bind(reason)
    .bind(summary)
    .bind(&now)
    .execute(db)
    .await?
    .last_insert_rowid();

    let row = sqlx::query_as::<_, EscalationRow>("SELECT * FROM cs_escalation WHERE id = ?")
        .bind(id)
        .fetch_one(db)
        .await?;
    Ok(row)
}

pub async fn escalation_list_open(db: &Db) -> anyhow::Result<Vec<EscalationRow>> {
    let rows = sqlx::query_as::<_, EscalationRow>(
        "SELECT * FROM cs_escalation WHERE status = 'open' ORDER BY id DESC",
    )
    .fetch_all(db)
    .await?;
    Ok(rows)
}

pub async fn escalation_get(db: &Db, id: i64) -> anyhow::Result<Option<EscalationRow>> {
    let row = sqlx::query_as::<_, EscalationRow>("SELECT * FROM cs_escalation WHERE id = ?")
        .bind(id)
        .fetch_optional(db)
        .await?;
    Ok(row)
}

pub async fn escalation_mark_handled(db: &Db, id: i64) -> anyhow::Result<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let res = sqlx::query("UPDATE cs_escalation SET status = 'handled', handled_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(db)
        .await?;
    if res.rows_affected() == 0 {
        anyhow::bail!("escalation {id} not found");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::db::Db;
    use super::*;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    #[tokio::test]
    async fn migration_creates_cs_tables() {
        let db = mem_db().await;
        // If the migration applied, this query against an empty table succeeds.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cs_conversation")
            .fetch_one(&db)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    // --- Task 2: Conversation ---

    #[tokio::test]
    async fn conversation_get_returns_row_and_none_for_missing() {
        let db   = mem_db().await;
        let conv = conversation_create(&db, "web", Some("Ani"), None, None, "tok-get")
            .await
            .unwrap();
        let found = conversation_get(&db, conv.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, conv.id);

        let missing = conversation_get(&db, 99999).await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn create_and_fetch_conversation_by_token() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", Some("Budi"), Some("budi@mail.com"), None, "tok-abc")
            .await
            .unwrap();
        assert_eq!(conv.channel, "web");
        assert_eq!(conv.status, "bot");
        assert_eq!(conv.visitor_name.as_deref(), Some("Budi"));

        let found = conversation_by_token(&db, "tok-abc").await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, conv.id);

        let missing = conversation_by_token(&db, "nope").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn set_status_and_touch_update_row() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", None, None, None, "tok-1").await.unwrap();

        conversation_set_status(&db, conv.id, "needs_human").await.unwrap();
        let after = conversation_by_token(&db, "tok-1").await.unwrap().unwrap();
        assert_eq!(after.status, "needs_human");

        conversation_touch(&db, conv.id).await.unwrap();
        let touched = conversation_by_token(&db, "tok-1").await.unwrap().unwrap();
        assert!(touched.last_msg_at >= conv.last_msg_at);
    }

    #[tokio::test]
    async fn invalid_status_rejected() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", None, None, None, "tok-2").await.unwrap();
        let res = conversation_set_status(&db, conv.id, "bogus").await;
        assert!(res.is_err());
    }

    // --- Task 3: Message ---

    #[tokio::test]
    async fn add_messages_and_fetch_in_order() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", None, None, None, "tok-m").await.unwrap();

        message_add(&db, conv.id, "user", "halo").await.unwrap();
        message_add(&db, conv.id, "assistant", "halo juga").await.unwrap();
        message_add(&db, conv.id, "user", "harga berapa?").await.unwrap();

        let all = message_all(&db, conv.id).await.unwrap();
        let contents: Vec<&str> = all.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(contents, vec!["halo", "halo juga", "harga berapa?"]);

        let recent = message_recent(&db, conv.id, 2).await.unwrap();
        let recent_contents: Vec<&str> = recent.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(recent_contents, vec!["halo juga", "harga berapa?"]); // last 2, oldest first
    }

    #[tokio::test]
    async fn message_count_returns_correct_total() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", None, None, None, "tok-cnt").await.unwrap();
        assert_eq!(message_count(&db, conv.id).await.unwrap(), 0);
        message_add(&db, conv.id, "user", "a").await.unwrap();
        message_add(&db, conv.id, "assistant", "b").await.unwrap();
        message_add(&db, conv.id, "user", "c").await.unwrap();
        assert_eq!(message_count(&db, conv.id).await.unwrap(), 3);
    }

    #[tokio::test]
    async fn message_invalid_role_rejected() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", None, None, None, "tok-mr").await.unwrap();
        let res = message_add(&db, conv.id, "robot", "x").await;
        assert!(res.is_err());
    }

    // --- Task 4: KB doc + chunk ---

    #[tokio::test]
    async fn blob_roundtrip_preserves_vector() {
        let v = vec![0.0f32, 1.5, -2.25, 3.125];
        let blob = embedding_to_blob(&v);
        let back = blob_to_embedding(&blob);
        assert_eq!(v, back);
    }

    #[tokio::test]
    async fn kb_doc_and_chunks_lifecycle() {
        let db = mem_db().await;
        let doc_id = kb_doc_insert(&db, "Refund policy", Some("faq"), "You can refund within 7 days.")
            .await
            .unwrap();

        // replace_chunks inserts fresh chunks with NULL embedding
        kb_replace_chunks(&db, doc_id, &["chunk one".into(), "chunk two".into()]).await.unwrap();
        let pending = kb_chunks_without_embedding(&db).await.unwrap();
        assert_eq!(pending.len(), 2);

        // set an embedding on the first chunk
        let first_id = pending[0].0;
        kb_set_chunk_embedding(&db, first_id, &embedding_to_blob(&[0.1, 0.2, 0.3])).await.unwrap();

        let pending_after = kb_chunks_without_embedding(&db).await.unwrap();
        assert_eq!(pending_after.len(), 1);

        let embedded = kb_chunks_with_embedding(&db).await.unwrap();
        assert_eq!(embedded.len(), 1);
        assert_eq!(embedded[0].doc_id, doc_id);
        assert_eq!(embedded[0].vector, vec![0.1f32, 0.2, 0.3]);

        // replacing chunks clears the old ones
        kb_replace_chunks(&db, doc_id, &["only one now".into()]).await.unwrap();
        assert_eq!(kb_chunks_without_embedding(&db).await.unwrap().len(), 1);
        assert_eq!(kb_chunks_with_embedding(&db).await.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn kb_doc_delete_cascades_chunks() {
        let db = mem_db().await;
        let doc_id = kb_doc_insert(&db, "Doc", None, "body").await.unwrap();
        kb_replace_chunks(&db, doc_id, &["a".into()]).await.unwrap();
        kb_doc_delete(&db, doc_id).await.unwrap();
        assert_eq!(kb_chunks_without_embedding(&db).await.unwrap().len(), 0);
        assert!(kb_doc_list(&db).await.unwrap().is_empty());
    }

    // --- Task 5: Product ---

    #[tokio::test]
    async fn product_insert_list_active_and_deactivate() {
        let db = mem_db().await;
        let id = product_insert(&db, "Paket A", Some("Basic"), Some(150000.0), Some("IDR"), Some("ready"))
            .await
            .unwrap();
        product_insert(&db, "Paket B", None, Some(300000.0), Some("IDR"), Some("ready")).await.unwrap();

        let active = product_list_active(&db).await.unwrap();
        assert_eq!(active.len(), 2);

        product_set_active(&db, id, false).await.unwrap();
        let active_after = product_list_active(&db).await.unwrap();
        assert_eq!(active_after.len(), 1);
        assert_eq!(active_after[0].name, "Paket B");
    }

    // --- Task 6: Order ---

    #[tokio::test]
    async fn order_upsert_and_guarded_lookup() {
        let db = mem_db().await;
        order_upsert(&db, "ORD-100", Some("Budi"), Some("budi@mail.com"), "shipped", None).await.unwrap();

        // correct ref + contact returns the order
        let hit = order_lookup(&db, "ORD-100", "budi@mail.com").await.unwrap();
        assert!(hit.is_some());
        assert_eq!(hit.unwrap().status, "shipped");

        // correct ref but wrong contact returns nothing (anti-enumeration)
        let miss = order_lookup(&db, "ORD-100", "someone@else.com").await.unwrap();
        assert!(miss.is_none());

        // upsert again updates status in place (no duplicate ref)
        order_upsert(&db, "ORD-100", Some("Budi"), Some("budi@mail.com"), "delivered", None).await.unwrap();
        let updated = order_lookup(&db, "ORD-100", "budi@mail.com").await.unwrap().unwrap();
        assert_eq!(updated.status, "delivered");
    }

    #[tokio::test]
    async fn order_lookup_contact_is_case_insensitive() {
        let db = mem_db().await;
        order_upsert(&db, "ORD-200", Some("Ani"), Some("Ani@Mail.com"), "processing", None).await.unwrap();
        let hit = order_lookup(&db, "ORD-200", "ani@mail.com").await.unwrap();
        assert!(hit.is_some());
    }

    // --- Task 7: Escalation ---

    #[tokio::test]
    async fn escalation_create_list_and_handle() {
        let db = mem_db().await;
        let conv = conversation_create(&db, "web", Some("Budi"), Some("b@x.com"), None, "tok-e").await.unwrap();

        let esc = escalation_create(&db, conv.id, "cannot_answer", "Customer asks about custom integration")
            .await
            .unwrap();
        assert_eq!(esc.status, "open");

        let open = escalation_list_open(&db).await.unwrap();
        assert_eq!(open.len(), 1);

        escalation_mark_handled(&db, esc.id).await.unwrap();
        assert!(escalation_list_open(&db).await.unwrap().is_empty());

        let handled = escalation_get(&db, esc.id).await.unwrap().unwrap();
        assert_eq!(handled.status, "handled");
        assert!(handled.handled_at.is_some());
    }

    // --- Fix 1: not-found errors on UPDATE functions ---

    #[tokio::test]
    async fn set_status_on_missing_conversation_errors() {
        let db = mem_db().await;
        assert!(conversation_set_status(&db, 999, "resolved").await.is_err());
    }

    #[tokio::test]
    async fn touch_on_missing_conversation_errors() {
        let db = mem_db().await;
        assert!(conversation_touch(&db, 999).await.is_err());
    }

    #[tokio::test]
    async fn kb_doc_update_on_missing_doc_errors() {
        let db = mem_db().await;
        assert!(kb_doc_update(&db, 999, "Title", None, "body").await.is_err());
    }

    #[tokio::test]
    async fn product_set_active_on_missing_product_errors() {
        let db = mem_db().await;
        assert!(product_set_active(&db, 999, false).await.is_err());
    }

    #[tokio::test]
    async fn mark_handled_on_missing_escalation_errors() {
        let db = mem_db().await;
        assert!(escalation_mark_handled(&db, 999).await.is_err());
    }

    // --- Task 1 (Plan 4a): product_update / product_delete ---

    #[tokio::test]
    async fn product_update_and_delete() {
        let db = mem_db().await;
        let id = product_insert(&db, "A", Some("x"), Some(100.0), Some("IDR"), Some("ready")).await.unwrap();
        product_update(&db, id, "A2", Some("y"), Some(200.0), Some("USD"), Some("soon")).await.unwrap();
        let all = product_list_all(&db).await.unwrap();
        let p = all.iter().find(|p| p.id == id).unwrap();
        assert_eq!(p.name, "A2");
        assert_eq!(p.price, Some(200.0));
        assert_eq!(p.currency.as_deref(), Some("USD"));

        product_delete(&db, id).await.unwrap();
        assert!(product_list_all(&db).await.unwrap().iter().all(|p| p.id != id));
        // updating a missing row errors
        assert!(product_update(&db, 999, "z", None, None, None, None).await.is_err());
    }

    // --- Task 1 (Plan 4a): order_get / order_delete ---

    #[tokio::test]
    async fn order_get_and_delete() {
        let db = mem_db().await;
        order_upsert(&db, "ORD-1", Some("Budi"), Some("b@x.com"), "shipped", None).await.unwrap();
        let all = order_list(&db, 10).await.unwrap();
        let oid = all[0].id;
        let got = order_get(&db, oid).await.unwrap().unwrap();
        assert_eq!(got.external_ref, "ORD-1");
        order_delete(&db, oid).await.unwrap();
        assert!(order_get(&db, oid).await.unwrap().is_none());
    }

    // --- Task 1 (Plan A): wa_jid column + per-JID conversation ---

    #[tokio::test]
    async fn wa_conversation_find_or_create_is_idempotent_per_jid() {
        let db  = mem_db().await;
        let jid = "628123@s.whatsapp.net";
        let c1  = conversation_create_wa(&db, jid, "628123", "tok-wa-1").await.unwrap();
        assert_eq!(c1.channel, "whatsapp");
        assert_eq!(c1.visitor_phone.as_deref(), Some("628123"));

        // lookup returns the same row
        let found = conversation_by_wa_jid(&db, jid).await.unwrap().unwrap();
        assert_eq!(found.id, c1.id);

        // a different jid is a different conversation
        let c2 = conversation_create_wa(&db, "628999@s.whatsapp.net", "628999", "tok-wa-2")
            .await
            .unwrap();
        assert_ne!(c2.id, c1.id);
        assert!(conversation_by_wa_jid(&db, "000@none").await.unwrap().is_none());
    }
}
