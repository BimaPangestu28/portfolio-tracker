//! Routes a CS tool call to its implementation. Knows ONLY the four CS tools.

use crate::cs::CsToolCtx;
use crate::cs::str_arg;

/// Dispatch a tool call. `Ok(text)` is fed back to the model as the tool result;
/// `Err(text)` becomes an `is_error` tool result the model can recover from.
pub async fn dispatch(
    ctx: &CsToolCtx<'_>,
    name: &str,
    input: &serde_json::Value,
) -> Result<String, String> {
    match name {
        "kb_search"          => kb_search(ctx, input).await,
        "get_pricing"        => get_pricing(ctx).await,
        "lookup_order"       => lookup_order(ctx, input).await,
        "escalate_to_human"  => escalate_to_human(ctx, input).await,
        _                    => Err(format!("unknown tool: {name}")),
    }
}

async fn kb_search(ctx: &CsToolCtx<'_>, input: &serde_json::Value) -> Result<String, String> {
    let query = str_arg(input, "query").ok_or("missing required argument 'query'")?;
    let hits  = crate::cs::kb::search(ctx.db, ctx.embedder, query, 4)
        .await
        .map_err(|e| format!("kb search error: {e}"))?;
    if hits.is_empty() {
        return Ok("Tidak ada hasil di knowledge base untuk pertanyaan ini.".to_string());
    }
    let joined = hits
        .iter()
        .enumerate()
        .map(|(i, c)| format!("[{}] {}", i + 1, c.text))
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok(joined)
}

async fn get_pricing(ctx: &CsToolCtx<'_>) -> Result<String, String> {
    let products = crate::repo::cs::product_list_active(ctx.db)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    if products.is_empty() {
        return Ok("Belum ada daftar harga yang tersedia.".to_string());
    }
    let lines = products
        .iter()
        .map(|p| {
            let price = match (p.price, &p.currency) {
                (Some(v), Some(c)) => format!("{c} {v}"),
                (Some(v), None)    => format!("{v}"),
                _                  => "-".to_string(),
            };
            let avail = p.availability.clone().unwrap_or_default();
            format!("- {} — {price} {}", p.name, avail).trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(lines)
}

async fn lookup_order(ctx: &CsToolCtx<'_>, input: &serde_json::Value) -> Result<String, String> {
    let order_ref = str_arg(input, "order_ref")
        .ok_or("missing required argument 'order_ref'")?;
    let contact   = str_arg(input, "contact")
        .ok_or("Untuk cek order, saya butuh email/no. HP yang dipakai saat order (untuk verifikasi).")?;
    let order = crate::repo::cs::order_lookup(ctx.db, order_ref, contact)
        .await
        .map_err(|e| format!("db error: {e}"))?;
    match order {
        Some(o) => Ok(format!("Order {} status: {}", o.external_ref, o.status)),
        None    => Ok("Tidak ada order yang cocok dengan referensi dan kontak itu.".to_string()),
    }
}

async fn escalate_to_human(ctx: &CsToolCtx<'_>, input: &serde_json::Value) -> Result<String, String> {
    let reason  = str_arg(input, "reason").unwrap_or("cannot_answer");
    let summary = str_arg(input, "summary")
        .ok_or("missing required argument 'summary'")?;
    crate::cs::escalation::escalate(ctx.db, ctx.conversation_id, reason, summary)
        .await
        .map_err(|e| format!("escalation error: {e}"))?;
    Ok("Sudah saya teruskan ke tim kami — mereka akan menghubungi kamu lewat kontak yang kamu berikan. Ada lagi yang bisa saya bantu?".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cs::kb::Embedder;
    use crate::db::Db;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    struct MockEmbedder;
    #[async_trait::async_trait]
    impl Embedder for MockEmbedder {
        async fn embed(&self, inputs: &[String]) -> Result<Vec<Vec<f32>>, crate::llm::claude::LlmError> {
            Ok(inputs
                .iter()
                .map(|s| vec![s.chars().next().unwrap_or(' ') as u32 as f32, 1.0, 1.0])
                .collect())
        }
    }

    #[tokio::test]
    async fn unknown_tool_is_an_error() {
        let db  = mem_db().await;
        let ctx = CsToolCtx { db: &db, embedder: &MockEmbedder, conversation_id: 1 };
        let out = dispatch(&ctx, "delete_everything", &serde_json::json!({})).await;
        assert!(out.is_err());
        assert!(out.unwrap_err().contains("unknown tool"));
    }

    #[tokio::test]
    async fn get_pricing_lists_active_products() {
        let db = mem_db().await;
        crate::repo::cs::product_insert(&db, "Paket A", Some("basic"), Some(150000.0), Some("IDR"), Some("ready"))
            .await
            .unwrap();
        let ctx = CsToolCtx { db: &db, embedder: &MockEmbedder, conversation_id: 1 };
        let out = dispatch(&ctx, "get_pricing", &serde_json::json!({})).await.unwrap();
        assert!(out.contains("Paket A"));
        assert!(out.contains("150000"));
    }

    #[tokio::test]
    async fn lookup_order_requires_both_args_and_matches_contact() {
        let db   = mem_db().await;
        crate::repo::cs::order_upsert(&db, "ORD-1", Some("Budi"), Some("b@x.com"), "shipped", None)
            .await
            .unwrap();
        let conv = crate::repo::cs::conversation_create(&db, "web", None, None, None, "t-d")
            .await
            .unwrap();
        let ctx  = CsToolCtx { db: &db, embedder: &MockEmbedder, conversation_id: conv.id };

        // missing contact -> error guidance
        let bad = dispatch(&ctx, "lookup_order", &serde_json::json!({ "order_ref": "ORD-1" })).await;
        assert!(bad.is_err());

        // correct -> status
        let ok = dispatch(
            &ctx,
            "lookup_order",
            &serde_json::json!({ "order_ref": "ORD-1", "contact": "b@x.com" }),
        )
        .await
        .unwrap();
        assert!(ok.contains("shipped"));

        // wrong contact -> not found (no leak)
        let miss = dispatch(
            &ctx,
            "lookup_order",
            &serde_json::json!({ "order_ref": "ORD-1", "contact": "x@y.com" }),
        )
        .await
        .unwrap();
        assert!(
            miss.to_lowercase().contains("tidak")
                || miss.to_lowercase().contains("not found")
                || miss.to_lowercase().contains("no order")
        );
    }

    #[tokio::test]
    async fn escalate_tool_records_and_flips_status() {
        let db   = mem_db().await;
        let conv = crate::repo::cs::conversation_create(
            &db, "web", Some("Ani"), Some("a@x.com"), None, "t-e",
        )
        .await
        .unwrap();
        let ctx = CsToolCtx { db: &db, embedder: &MockEmbedder, conversation_id: conv.id };
        let out = dispatch(
            &ctx,
            "escalate_to_human",
            &serde_json::json!({ "reason": "cannot_answer", "summary": "needs custom quote" }),
        )
        .await
        .unwrap();
        assert!(!out.is_empty());
        assert_eq!(
            crate::repo::cs::escalation_list_open(&db).await.unwrap().len(),
            1
        );
        let after = crate::repo::cs::conversation_by_token(&db, "t-e")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.status, "needs_human");
    }

    #[tokio::test]
    async fn dispatcher_rejects_every_owner_tool_name() {
        // The CS dispatcher must expose ONLY the four CS tools. Any Noah/owner tool
        // name must be rejected — this is the core isolation guarantee.
        let db  = mem_db().await;
        let ctx = CsToolCtx { db: &db, embedder: &MockEmbedder, conversation_id: 1 };
        for owner_tool in [
            "create_todo", "list_todos", "capture_to_inbox", "create_invoice",
            "portfolio_summary", "list_reminders", "create_event", "clickup_create_task",
        ] {
            let out = dispatch(&ctx, owner_tool, &serde_json::json!({})).await;
            assert!(out.is_err(), "owner tool '{owner_tool}' must not be dispatchable by CS");
        }
    }

    #[test]
    fn cs_tool_names_do_not_overlap_owner_tools() {
        let cs_names: Vec<String> = crate::cs::tools::definitions()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        // crate::assistant::tools::definitions() is pub — use it directly.
        let owner_names: Vec<String> = crate::assistant::tools::definitions()
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap().to_string())
            .collect();
        for n in &cs_names {
            assert!(
                !owner_names.contains(n),
                "CS tool '{n}' collides with an owner tool name"
            );
        }
    }
}
