# CS Chatbot — Plan 4a: Admin Backend API Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** JWT-protected admin endpoints so the owner can manage the CS knowledge base (with on-save embedding), pricing, and orders, and work the CS inbox (read transcripts, see escalations, mark resolved/handled).

**Architecture:** A new `api/cs_admin.rs` with thin handlers over `repo::cs`, mounted in the existing `protected` router tier (JWT-enforced). KB save (re)chunks via `cs::kb::chunk_text`, stores chunks, and best-effort embeds via `cs::kb::embed_pending` (durable even if the embedder is down; a `reindex` endpoint re-embeds pending chunks on demand). A few small additive repo functions (`product_update`, `product_delete`, `order_delete`, `order_get`) round out CRUD. No new dependencies.

**Tech Stack:** Rust, axum, sqlx. Depends on Plans 1–3 (`repo::cs`, `cs::kb`).

> **Work in the worktree** `/home/bima-pangestu/Works/portfolio-tracker/.claude/worktrees/cs-chatbot`. **No `cargo fmt`.** Verify with `cargo test` + `cargo clippy`.

---

## File Structure

- Modify: `backend/src/repo/cs.rs` — add `product_update`, `product_delete`, `order_delete`, `order_get` (+ tests).
- Create: `backend/src/api/cs_admin.rs` — all admin handlers.
- Modify: `backend/src/api/mod.rs` — declare `mod cs_admin;`; add the `/cs/admin/*` routes to the `protected` group.

> Admin routes live in the `protected` tier (JWT). They are NOT in the public CS group and NOT under the scoped CORS — they're same-origin SPA calls.

---

## Task 1: Repo CRUD additions

**Files:**
- Modify: `backend/src/repo/cs.rs`

- [ ] **Step 1: Write the failing tests**

Add inside `repo/cs.rs`'s `mod tests`:

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd backend && cargo test repo::cs::tests::product_update_and_delete`
Expected: FAIL — functions not found.

- [ ] **Step 3: Implement**

Add to `backend/src/repo/cs.rs`:

```rust
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
```

- [ ] **Step 4: Run to verify pass**

Run: `cd backend && cargo test repo::cs::tests::product_update_and_delete repo::cs::tests::order_get_and_delete`
(Run each separately if the multi-filter form misbehaves: `cargo test product_update_and_delete` then `cargo test order_get_and_delete`.)
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add backend/src/repo/cs.rs
git commit -m "feat(cs): repo CRUD additions (product update/delete, order get/delete)"
```

---

## Task 2: KB admin handlers (with on-save embedding + reindex)

**Files:**
- Create: `backend/src/api/cs_admin.rs`
- Modify: `backend/src/api/mod.rs` (declare `mod cs_admin;` matching sibling handler module declarations)

> **Context:** Handlers are JWT-protected (mounted in the `protected` tier in Task 6). Saving a doc: persist the doc, (re)chunk via `cs::kb::chunk_text`, `repo::cs::kb_replace_chunks`, then best-effort `cs::kb::embed_pending` (so a doc is saved even when the embedder is unavailable; unembedded chunks just aren't searchable yet). `reindex` re-embeds pending chunks on demand.

- [ ] **Step 1: Write the KB handlers**

Create `backend/src/api/cs_admin.rs`:

```rust
//! JWT-protected admin endpoints for the customer-service chatbot: knowledge
//! base, pricing, orders, and the CS inbox. Thin glue over repo::cs + cs::kb.

use axum::{extract::{Path, State}, Json};
use serde::Deserialize;

use crate::cs::kb::{self, CsEmbedder};
use crate::error::AppError;
use crate::repo::cs as repo;
use crate::AppState;

// ----------------------------- Knowledge base -----------------------------

pub async fn list_docs(State(s): State<AppState>) -> Result<Json<Vec<repo::KbDocRow>>, AppError> {
    Ok(Json(repo::kb_doc_list(&s.db).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct DocIn {
    pub title: String,
    pub source: Option<String>,
    pub body: String,
}

/// Persist the doc + chunks, then best-effort embed. Embedding failure does not
/// fail the save (the doc is durable; chunks embed later via reindex).
async fn save_chunks_and_embed(db: &crate::db::Db, doc_id: i64, body: &str) {
    let chunks = kb::chunk_text(body);
    if let Err(e) = repo::kb_replace_chunks(db, doc_id, &chunks).await {
        tracing::error!("cs admin: replace_chunks failed for doc {doc_id}: {e}");
        return;
    }
    match CsEmbedder::from_env() {
        Ok(embedder) => {
            if let Err(e) = kb::embed_pending(db, &embedder).await {
                tracing::warn!("cs admin: embed_pending failed for doc {doc_id}: {e}");
            }
        }
        Err(e) => tracing::warn!("cs admin: embedder unavailable, doc {doc_id} saved unembedded: {e}"),
    }
}

pub async fn create_doc(State(s): State<AppState>, Json(b): Json<DocIn>) -> Result<Json<repo::KbDocRow>, AppError> {
    if b.title.trim().is_empty() || b.body.trim().is_empty() {
        return Err(AppError::BadRequest("title and body are required".into()));
    }
    let id = repo::kb_doc_insert(&s.db, b.title.trim(), b.source.as_deref(), &b.body)
        .await
        .map_err(AppError::Other)?;
    save_chunks_and_embed(&s.db, id, &b.body).await;
    let doc = repo::kb_doc_list(&s.db)
        .await
        .map_err(AppError::Other)?
        .into_iter()
        .find(|d| d.id == id)
        .ok_or(AppError::NotFound)?;
    Ok(Json(doc))
}

pub async fn update_doc(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(b): Json<DocIn>,
) -> Result<Json<()>, AppError> {
    repo::kb_doc_update(&s.db, id, b.title.trim(), b.source.as_deref(), &b.body)
        .await
        .map_err(AppError::Other)?;
    save_chunks_and_embed(&s.db, id, &b.body).await;
    Ok(Json(()))
}

pub async fn delete_doc(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    repo::kb_doc_delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

/// Re-embed any chunks lacking an embedding (e.g. saved while the embedder was down).
pub async fn reindex_kb(State(s): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let embedder = CsEmbedder::from_env()
        .map_err(|e| AppError::BadRequest(format!("embedder unavailable: {e}")))?;
    let n = kb::embed_pending(&s.db, &embedder).await.map_err(AppError::Other)?;
    Ok(Json(serde_json::json!({ "embedded": n })))
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cd backend && cargo check 2>&1 | tail -6`
Expected: compiles (routes wired in Task 6; `dead_code` expected). Confirm `crate::AppState` + `crate::db::Db` import paths match how `api/chat.rs` does it; fix if needed.

- [ ] **Step 3: Commit**

```bash
git add backend/src/api/cs_admin.rs backend/src/api/mod.rs
git commit -m "feat(cs): admin KB handlers (CRUD + on-save embed + reindex)"
```

---

## Task 3: Pricing admin handlers

**Files:**
- Modify: `backend/src/api/cs_admin.rs`

- [ ] **Step 1: Add the handlers**

Append to `backend/src/api/cs_admin.rs`:

```rust
// ----------------------------- Pricing -----------------------------

pub async fn list_products(State(s): State<AppState>) -> Result<Json<Vec<repo::ProductRow>>, AppError> {
    Ok(Json(repo::product_list_all(&s.db).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct ProductIn {
    pub name: String,
    pub description: Option<String>,
    pub price: Option<f64>,
    pub currency: Option<String>,
    pub availability: Option<String>,
}

pub async fn create_product(State(s): State<AppState>, Json(b): Json<ProductIn>) -> Result<Json<serde_json::Value>, AppError> {
    if b.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    let id = repo::product_insert(&s.db, b.name.trim(), b.description.as_deref(), b.price, b.currency.as_deref(), b.availability.as_deref())
        .await
        .map_err(AppError::Other)?;
    Ok(Json(serde_json::json!({ "id": id })))
}

pub async fn update_product(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<ProductIn>) -> Result<Json<()>, AppError> {
    repo::product_update(&s.db, id, b.name.trim(), b.description.as_deref(), b.price, b.currency.as_deref(), b.availability.as_deref())
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

#[derive(Deserialize)]
pub struct ActiveIn { pub active: bool }

pub async fn set_product_active(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<ActiveIn>) -> Result<Json<()>, AppError> {
    repo::product_set_active(&s.db, id, b.active).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

pub async fn delete_product(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    repo::product_delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
```

- [ ] **Step 2: Verify + commit**

Run: `cd backend && cargo check 2>&1 | tail -4`

```bash
git add backend/src/api/cs_admin.rs
git commit -m "feat(cs): admin pricing handlers"
```

---

## Task 4: Orders admin handlers

**Files:**
- Modify: `backend/src/api/cs_admin.rs`

- [ ] **Step 1: Add the handlers**

Append to `backend/src/api/cs_admin.rs`:

```rust
// ----------------------------- Orders -----------------------------

pub async fn list_orders(State(s): State<AppState>) -> Result<Json<Vec<repo::OrderRow>>, AppError> {
    Ok(Json(repo::order_list(&s.db, 500).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct OrderIn {
    pub external_ref: String,
    pub customer_name: Option<String>,
    pub customer_contact: Option<String>,
    pub status: String,
    pub details_json: Option<String>,
}

/// Upsert by external_ref (owner-populated).
pub async fn upsert_order(State(s): State<AppState>, Json(b): Json<OrderIn>) -> Result<Json<()>, AppError> {
    if b.external_ref.trim().is_empty() || b.status.trim().is_empty() {
        return Err(AppError::BadRequest("external_ref and status are required".into()));
    }
    repo::order_upsert(&s.db, b.external_ref.trim(), b.customer_name.as_deref(), b.customer_contact.as_deref(), b.status.trim(), b.details_json.as_deref())
        .await
        .map_err(AppError::Other)?;
    Ok(Json(()))
}

pub async fn delete_order(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    repo::order_delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
```

- [ ] **Step 2: Verify + commit**

Run: `cd backend && cargo check 2>&1 | tail -4`

```bash
git add backend/src/api/cs_admin.rs
git commit -m "feat(cs): admin orders handlers"
```

---

## Task 5: Inbox + escalation admin handlers

**Files:**
- Modify: `backend/src/api/cs_admin.rs`

- [ ] **Step 1: Add the handlers**

Append to `backend/src/api/cs_admin.rs`:

```rust
// ----------------------------- Inbox / escalations -----------------------------

pub async fn list_conversations(State(s): State<AppState>) -> Result<Json<Vec<repo::ConversationRow>>, AppError> {
    Ok(Json(repo::conversation_list_recent(&s.db, 200).await.map_err(AppError::Other)?))
}

pub async fn conversation_messages(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<Vec<repo::MessageRow>>, AppError> {
    Ok(Json(repo::message_all(&s.db, id).await.map_err(AppError::Other)?))
}

pub async fn resolve_conversation(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    repo::conversation_set_status(&s.db, id, "resolved").await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}

pub async fn list_escalations(State(s): State<AppState>) -> Result<Json<Vec<repo::EscalationRow>>, AppError> {
    Ok(Json(repo::escalation_list_open(&s.db).await.map_err(AppError::Other)?))
}

pub async fn handle_escalation(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    repo::escalation_mark_handled(&s.db, id).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(()))
}
```

- [ ] **Step 2: Verify + commit**

Run: `cd backend && cargo check 2>&1 | tail -4`

```bash
git add backend/src/api/cs_admin.rs
git commit -m "feat(cs): admin inbox + escalation handlers"
```

---

## Task 6: Wire routes into the protected tier

**Files:**
- Modify: `backend/src/api/mod.rs`

- [ ] **Step 1: Add the routes to the `protected` router**

In `backend/src/api/mod.rs`, inside the `protected` router builder (the group that ends with `.route_layer(middleware::from_fn(auth::require_auth))`), add the CS admin routes alongside the existing protected routes:

```rust
        .route("/cs/admin/docs", get(cs_admin::list_docs).post(cs_admin::create_doc))
        .route("/cs/admin/docs/:id", patch(cs_admin::update_doc).delete(cs_admin::delete_doc))
        .route("/cs/admin/kb/reindex", post(cs_admin::reindex_kb))
        .route("/cs/admin/products", get(cs_admin::list_products).post(cs_admin::create_product))
        .route("/cs/admin/products/:id", patch(cs_admin::update_product).delete(cs_admin::delete_product))
        .route("/cs/admin/products/:id/active", post(cs_admin::set_product_active))
        .route("/cs/admin/orders", get(cs_admin::list_orders).post(cs_admin::upsert_order))
        .route("/cs/admin/orders/:id", axum::routing::delete(cs_admin::delete_order))
        .route("/cs/admin/conversations", get(cs_admin::list_conversations))
        .route("/cs/admin/conversations/:id/messages", get(cs_admin::conversation_messages))
        .route("/cs/admin/conversations/:id/resolve", post(cs_admin::resolve_conversation))
        .route("/cs/admin/escalations", get(cs_admin::list_escalations))
        .route("/cs/admin/escalations/:id/handle", post(cs_admin::handle_escalation))
```

> **Implementer notes:**
> - Confirm `patch` is imported from `axum::routing` (the protected group may already use `get`/`post`; add `patch` to the `use axum::routing::{...}` line). For `delete`, either import it too or use `axum::routing::delete` inline as shown.
> - Place these `.route(...)` calls BEFORE the `.route_layer(middleware::from_fn(auth::require_auth))` so they're JWT-protected.
> - Where two methods share a path, chain them (`get(...).post(...)`, `patch(...).delete(...)`) as shown.

- [ ] **Step 2: Full verification**

Run: `cd backend && cargo test cs:: repo::cs:: error:: api:: 2>&1 | tail -6` (run filters separately if needed) and `cargo clippy --all-targets 2>&1 | tail -12`.
Expected: all pass; only `dead_code` warnings (frontend consumes these in Plan 4b). No compile errors.

- [ ] **Step 3: Manual smoke (optional, if a JWT-less dev env is available)**

If `AUTH_PASSWORD`/`JWT_SECRET` are unset (dev/open mode), the protected routes are reachable. With the backend running:

```bash
curl -s localhost:8080/cs/admin/products            # -> []
curl -s -X POST localhost:8080/cs/admin/products -H 'content-type: application/json' \
  -d '{"name":"Paket A","price":150000,"currency":"IDR","availability":"ready"}'
curl -s localhost:8080/cs/admin/products            # -> [{...Paket A...}]
```

State what you ran (or skip if no runnable env; the cargo tests are the gate).

- [ ] **Step 4: Commit**

```bash
git add backend/src/api/mod.rs
git commit -m "feat(cs): mount admin routes in protected tier"
```

---

## Self-Review

**Spec coverage (spec §10 admin — backend half):**
- KB CRUD + on-save embedding + reindex ✓ Task 2.
- Pricing CRUD ✓ Task 3.
- Orders CRUD (upsert + list + delete) ✓ Task 4.
- CS Inbox: conversations list, transcript, resolve; escalations list + handle ✓ Task 5.
- All JWT-protected (protected tier), same-origin (no scoped CORS) ✓ Task 6.
- KB save is durable even if the embedder is down (best-effort embed + reindex) ✓ Task 2.

**Placeholder scan:** No TBD/TODO; implementer-verification notes target real imports (`patch`/`delete` routing, `AppState`/`Db` paths).

**Type consistency:** Handlers return existing repo row types (`KbDocRow`, `ProductRow`, `OrderRow`, `ConversationRow`, `MessageRow`, `EscalationRow`) — already `Serialize` (Plan 1). Repo fns added in Task 1 (`product_update`, `product_delete`, `order_get`, `order_delete`) are used in Tasks 3–4. Route paths match the handler set and are consumed by Plan 4b's hooks.

---

## Downstream

- **Plan 4b — Admin frontend:** schemas + hooks + 4 SPA pages (KB / Pricing / Orders / CS-Inbox) under `/cs/admin/*` routes + an "Admin (CS)" nav group, mirroring `BudgetPage` (list + Dialog form + mutation hooks) and the `useInvalidatingMutation` pattern.
