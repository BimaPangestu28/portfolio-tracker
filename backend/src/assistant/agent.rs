//! The tool-use agent loop: conversation in, tools executed, final text out.

use crate::db::Db;
use crate::llm::claude::{extract_blocks, ClaudeClient, LlmError, ResponseBlock};

/// Hard cap on model round-trips per user message (cost / runaway guard).
pub const MAX_ITERATIONS: usize = 5;

/// How many prior messages of the channel's conversation the model sees.
const HISTORY_LIMIT: i64 = 12;

pub const ITERATION_CAP_REPLY: &str =
    "Maaf, permintaan ini terlalu rumit untuk diproses sekaligus. Coba pecah jadi beberapa pesan ya.";

/// Fallback when the model legally ends its turn without any usable text.
pub const NO_TEXT_REPLY: &str = "Beres.";

const SYSTEM: &str = "You are a personal assistant for the app owner, reachable via Telegram. \
You manage todos and reminders and can answer questions about the owner's investment portfolio \
via the get_portfolio_summary tool. Reply in the user's language (usually Indonesian). \
Execute todo/reminder actions immediately without asking for confirmation, then summarize what \
you did, including ids and times (times in WIB). All datetimes in tool arguments must be RFC3339 \
with the +07:00 offset — the user's timezone is WIB (Asia/Jakarta). You are replying inside a \
plain-text messenger: do NOT use any Markdown (no tables, no headers, no **bold**). Write short \
lines; for lists use simple dashes or emoji. You have long-term memory: relevant known facts about the owner may be listed \
below — treat them as context, not unquestionable truth. Use the search_memory \
tool for explicit recall questions, and the remember tool when the user asks \
you to remember a fact. \
 You also manage the owner's agenda: create_event (a pre-event reminder is \
created automatically), list_events for schedule questions like 'besok ada \
apa?', and cancel_event. \
 You can also enter transactions the owner sent as photos/PDFs: when they ask \
to 'masukin transaksi tadi' or to confirm one, call list_pending_reviews. If \
the account shows 'belum dikenali', call list_accounts to find a match; if \
none fits, ask the user before calling create_account. If the instrument shows \
'belum dikenali', call list_instruments to find it (auto-matching only catches \
exact names); if it isn't there, tell the user to add it in the web UI → Data \
— instruments can't be created from chat. The account/instrument shown is only \
a guess read from the photo (OCR), not authoritative — if the owner says it's \
wrong (e.g. the photo reads one broker but the holding is in another), find the \
right one with list_accounts/list_instruments and pass its account_id/instrument_id \
to confirm_review to override it; you do NOT need the item to show 'belum \
dikenali' first. Then call confirm_review with the \
account_id and/or instrument_id needed. Unlike todos/reminders, ALWAYS ask the \
user to confirm before create_account or confirm_review — these write financial \
data that can't be silently undone. Use reject_review to discard an item. \
 You also manage freelance projects in ClickUp. When the user wants to add a task \
(e.g. 'tambahin task bikin kontrak'), call list_projects: if the user named no \
project and more than one exists, ask which project; then call create_task with \
that project name and the title. If create_task reports the project 'belum ada', \
ask the user whether to create it, and only after they agree call create_project, \
then retry create_task. ALWAYS ask the user to confirm before create_project — \
it creates data in ClickUp. Creating a task itself is immediate, like a todo. \
 To answer 'ada task apa di <project>?' or 'task hari ini / yang overdue?', call list_tasks \
(pass a project name, or scope 'today'/'overdue'). It shows each task id in brackets; pass that \
id to complete_task when the user says a task is done. \
 When the user says a task is billable or gives a price (e.g. 'task landing page PT AIS, \
billable 10 juta'), pass billable=true and amount (in IDR) to create_task so it can be \
invoiced later. \
 You can also draft Upwork job proposals: when the owner pastes a job and asks for a proposal \
(e.g. 'buatin proposal buat ini'), call draft_proposal with the pasted job_text (and notes if the \
owner specifies emphasis or a bid). The tool returns a ready-to-send English draft — relay it to \
the owner verbatim, without summarizing, translating, or reformatting it. \
 You can also make invoices: when the owner says e.g. 'buatin invoice PT AIS: landing page 10 juta, hosting 2 juta', parse each line into {title, amount in IDR} and call create_invoice with client_name + line_items. Convert '10 juta' to 10000000. First echo the parsed items and the total back to the owner so they can catch typos. If create_invoice reports the client 'belum ada', ask the owner for the client's sub-name/website and retry with client_details. The PDF is sent to Telegram automatically.";

/// The slice of the LLM client the agent loop needs — a seam for test doubles.
#[async_trait::async_trait]
pub trait ToolModel {
    async fn complete_tools(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value, LlmError>;
}

#[async_trait::async_trait]
impl ToolModel for ClaudeClient {
    async fn complete_tools(
        &self,
        system: &str,
        messages: &[serde_json::Value],
        tools: &serde_json::Value,
    ) -> Result<serde_json::Value, LlmError> {
        ClaudeClient::complete_tools(self, system, messages, tools).await
    }
}

/// System prompt with the current WIB time embedded, so the model can resolve
/// "besok jam 9" itself — no hand-written date parser.
fn system_prompt(now_wib: &str) -> String {
    format!("{SYSTEM}\n\nCurrent datetime: {now_wib}")
}

/// How many facts are auto-injected into the system prompt per message.
const INJECT_FACT_LIMIT: u32 = 8;

/// Full system prompt: persona + current time + any long-term-memory facts.
fn compose_system(now_wib: &str, facts: &[super::memory::MemoryFact]) -> String {
    format!("{}{}", system_prompt(now_wib), super::memory::render_facts_block(facts))
}

/// Prior turns as plain-text messages, then the new user message. Leading
/// assistant turns are dropped (API requires the first message to be a user's).
fn build_messages(history: &[(String, String)], user_msg: &str) -> Vec<serde_json::Value> {
    let first_user = history.iter().position(|(role, _)| role == "user").unwrap_or(history.len());
    let mut messages: Vec<serde_json::Value> = history[first_user..]
        .iter()
        .map(|(role, content)| serde_json::json!({ "role": role, "content": content }))
        .collect();
    messages.push(serde_json::json!({ "role": "user", "content": user_msg }));
    messages
}

/// Render one dispatcher outcome as a tool_result block.
fn tool_result_block(id: &str, outcome: &Result<String, String>) -> serde_json::Value {
    match outcome {
        Ok(text) => serde_json::json!({
            "type": "tool_result", "tool_use_id": id, "content": text
        }),
        Err(text) => serde_json::json!({
            "type": "tool_result", "tool_use_id": id, "content": text, "is_error": true
        }),
    }
}

/// Persist the finished turn and (when memory is configured) ingest it as an
/// episode, fire-and-forget — ingestion must never delay or fail the reply.
async fn store_and_ingest(
    db: &Db,
    memory: Option<super::memory::MemoryClient>,
    channel: &str,
    user_msg: &str,
    reply: &str,
) -> anyhow::Result<()> {
    crate::repo::chat::add(db, "user", user_msg, channel).await?;
    crate::repo::chat::add(db, "assistant", reply, channel).await?;
    if let Some(client) = memory {
        let episode = format!("User: {user_msg}\nAssistant: {reply}");
        tokio::spawn(async move {
            if let Err(e) = client.add_episode(&episode, "chat").await {
                tracing::warn!("memory ingest failed: {e}");
            }
        });
    }
    Ok(())
}

/// Load the channel's recent chat history as (role, content) pairs.
async fn load_history(db: &Db, channel: &str) -> Vec<(String, String)> {
    crate::repo::chat::recent_by_channel(db, channel, HISTORY_LIMIT)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|m| (m.role, m.content))
        .collect()
}

/// Drive the tool-use loop over `messages` until the model returns text or hits
/// the iteration cap. Returns the reply only — persisting is the caller's job.
/// A shape anomaly or the cap yields a fallback reply (Ok), never an Err; only a
/// transport/LLM error propagates.
async fn run_tool_loop<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    system: &str,
    mut messages: Vec<serde_json::Value>,
) -> anyhow::Result<String> {
    let tools = super::tools::definitions();
    for _ in 0..MAX_ITERATIONS {
        let resp = model
            .complete_tools(system, &messages, &tools)
            .await
            .map_err(|e| anyhow::anyhow!("llm error: {e}"))?;
        let blocks = match extract_blocks(&resp) {
            Ok(blocks) => blocks,
            Err(e) => {
                // Prior iterations' tool side effects are already committed; end
                // the turn with a fallback reply (the caller still persists it)
                // rather than erroring and leaving the user with nothing.
                tracing::warn!("assistant: unusable model response ({e}); using fallback reply");
                return Ok(NO_TEXT_REPLY.to_string());
            }
        };
        let tool_uses: Vec<(String, String, serde_json::Value)> = blocks
            .iter()
            .filter_map(|b| match b {
                ResponseBlock::ToolUse { id, name, input } => {
                    Some((id.clone(), name.clone(), input.clone()))
                }
                _ => None,
            })
            .collect();

        if tool_uses.is_empty() {
            let mut reply: String = blocks
                .into_iter()
                .filter_map(|b| match b {
                    ResponseBlock::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            if reply.trim().is_empty() {
                reply = NO_TEXT_REPLY.to_string();
            }
            return Ok(reply);
        }

        messages.push(serde_json::json!({ "role": "assistant", "content": resp["content"].clone() }));
        let mut results = Vec::new();
        for (id, name, input) in &tool_uses {
            let outcome = super::dispatcher::dispatch(db, name, input).await;
            tracing::info!(
                "assistant tool {name}: {}",
                if outcome.is_ok() { "ok" } else { "error" }
            );
            results.push(tool_result_block(id, &outcome));
        }
        messages.push(serde_json::json!({ "role": "user", "content": results }));
    }
    Ok(ITERATION_CAP_REPLY.to_string())
}

/// Run the agent loop for one inbound message. Stores the user message and
/// the final reply in chat history only on success (no orphaned rows).
pub async fn handle_message<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    channel: &str,
    user_msg: &str,
) -> anyhow::Result<String> {
    let now_wib = chrono::Utc::now().with_timezone(&super::time::wib()).to_rfc3339();
    let memory = super::memory::MemoryClient::from_env();
    let facts = match &memory {
        Some(client) => client.search(user_msg, INJECT_FACT_LIMIT).await,
        None => Vec::new(),
    };
    let system = compose_system(&now_wib, &facts);
    let history = load_history(db, channel).await;
    let messages = build_messages(&history, user_msg);

    // Tool side effects commit eagerly per iteration and are intentionally NOT
    // rolled back if a later model call fails. Only chat rows wait for success.
    let reply = run_tool_loop(db, model, &system, messages).await?;
    store_and_ingest(db, memory, channel, user_msg, &reply).await?;
    Ok(reply)
}

/// Kick off the assistant after a file upload. The model sees `seed` (the staged
/// items plus how to handle them) as the opening turn and replies naturally, but
/// chat history stores the concise `history_marker` in place of the verbose seed,
/// so a later "iya" still has the assistant's question for context. Long-term
/// memory is not consulted here — the seed already carries the resolved account.
pub async fn handle_upload_event<M: ToolModel + Sync>(
    db: &Db,
    model: &M,
    channel: &str,
    seed: &str,
    history_marker: &str,
) -> anyhow::Result<String> {
    let now_wib = chrono::Utc::now().with_timezone(&super::time::wib()).to_rfc3339();
    // No memory lookup here — the upload seed is self-contained.
    let system = compose_system(&now_wib, &[]);
    let history = load_history(db, channel).await;
    let messages = build_messages(&history, seed);

    let reply = run_tool_loop(db, model, &system, messages).await?;
    store_and_ingest(db, None, channel, history_marker, &reply).await?;
    Ok(reply)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::sync::Mutex;

    async fn mem_db() -> Db {
        crate::db::connect("sqlite::memory:").await.unwrap()
    }

    /// Scripted model: pops one canned response per call, counts calls.
    struct ScriptedModel {
        responses: Mutex<VecDeque<serde_json::Value>>,
        calls: Mutex<usize>,
        seen: Mutex<Vec<Vec<serde_json::Value>>>,
    }

    impl ScriptedModel {
        fn new(responses: Vec<serde_json::Value>) -> Self {
            Self {
                responses: Mutex::new(responses.into()),
                calls: Mutex::new(0),
                seen: Mutex::new(Vec::new()),
            }
        }
        fn call_count(&self) -> usize {
            *self.calls.lock().unwrap()
        }
        fn messages_of_call(&self, idx: usize) -> Vec<serde_json::Value> {
            self.seen.lock().unwrap()[idx].clone()
        }
    }

    #[async_trait::async_trait]
    impl ToolModel for ScriptedModel {
        async fn complete_tools(
            &self,
            _system: &str,
            messages: &[serde_json::Value],
            _tools: &serde_json::Value,
        ) -> Result<serde_json::Value, LlmError> {
            self.seen.lock().unwrap().push(messages.to_vec());
            *self.calls.lock().unwrap() += 1;
            Ok(self
                .responses
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(serde_json::json!({ "content": [
                    { "type": "tool_use", "id": "loop", "name": "list_todos", "input": {} }
                ]})))
        }
    }

    fn text_response(text: &str) -> serde_json::Value {
        serde_json::json!({ "content": [{ "type": "text", "text": text }] })
    }

    #[tokio::test]
    async fn plain_text_response_is_returned_and_stored() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![text_response("halo!")]);
        let reply = handle_message(&db, &model, "telegram", "halo").await.unwrap();
        assert_eq!(reply, "halo!");
        let history = crate::repo::chat::recent_by_channel(&db, "telegram", 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "halo");
        assert_eq!(history[1].content, "halo!");
    }

    #[tokio::test]
    async fn tool_use_executes_against_the_db_then_replies() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![
            serde_json::json!({ "content": [
                { "type": "tool_use", "id": "tu_1", "name": "create_todo",
                  "input": { "title": "bayar listrik" } }
            ]}),
            text_response("Sip, todo dibuat."),
        ]);
        let reply = handle_message(&db, &model, "telegram", "catat: bayar listrik").await.unwrap();
        assert_eq!(reply, "Sip, todo dibuat.");
        assert_eq!(model.call_count(), 2);
        let todos = crate::repo::todos::list_open(&db).await.unwrap();
        assert_eq!(todos.len(), 1);
        assert_eq!(todos[0].title, "bayar listrik");

        // Pin the Messages API contract: the 2nd call must see the assistant
        // turn replayed verbatim, then a user turn answering the tool_use.
        let second = model.messages_of_call(1);
        assert_eq!(second.len(), 3, "{second:?}");
        assert_eq!(second[1]["role"], "assistant");
        assert_eq!(second[1]["content"][0]["type"], "tool_use");
        assert_eq!(second[1]["content"][0]["id"], "tu_1");
        assert_eq!(second[2]["role"], "user");
        assert_eq!(second[2]["content"][0]["type"], "tool_result");
        assert_eq!(second[2]["content"][0]["tool_use_id"], "tu_1");
        assert!(second[2]["content"][0].get("is_error").is_none());
    }

    #[tokio::test]
    async fn tool_errors_feed_back_and_the_model_recovers() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![
            serde_json::json!({ "content": [
                { "type": "tool_use", "id": "tu_1", "name": "complete_todo",
                  "input": { "id": 999 } }
            ]}),
            text_response("Todo #999 tidak ada."),
        ]);
        let reply = handle_message(&db, &model, "telegram", "selesaikan todo 999").await.unwrap();
        assert_eq!(reply, "Todo #999 tidak ada.");

        // The failed dispatch must surface as an is_error tool_result.
        let second = model.messages_of_call(1);
        assert_eq!(second[2]["content"][0]["is_error"], serde_json::json!(true));
    }

    #[tokio::test]
    async fn iteration_cap_returns_apology() {
        let db = mem_db().await;
        // Empty script: every call falls back to a tool_use response, forever.
        let model = ScriptedModel::new(vec![]);
        let reply = handle_message(&db, &model, "telegram", "x").await.unwrap();
        assert_eq!(reply, ITERATION_CAP_REPLY);
        assert_eq!(model.call_count(), MAX_ITERATIONS);
    }

    #[tokio::test]
    async fn empty_text_reply_falls_back() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![text_response("")]);
        let reply = handle_message(&db, &model, "telegram", "halo").await.unwrap();
        assert_eq!(reply, NO_TEXT_REPLY);
    }

    #[tokio::test]
    async fn unusable_response_yields_fallback_not_error() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![serde_json::json!({ "content": [] })]);
        let reply = handle_message(&db, &model, "telegram", "halo").await.unwrap();
        assert_eq!(reply, NO_TEXT_REPLY);
        let history = crate::repo::chat::recent_by_channel(&db, "telegram", 10).await.unwrap();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn build_messages_drops_leading_assistant_history() {
        let history = vec![
            ("assistant".to_string(), "a0".to_string()),
            ("user".to_string(), "q1".to_string()),
            ("assistant".to_string(), "a1".to_string()),
        ];
        let messages = build_messages(&history, "q2");
        assert_eq!(messages.len(), 3);
        assert_eq!(messages[0]["role"], "user");
        assert_eq!(messages[2]["content"], "q2");
    }

    #[test]
    fn system_prompt_embeds_the_current_time() {
        let prompt = system_prompt("2026-06-11T15:00:00+07:00");
        assert!(prompt.contains("2026-06-11T15:00:00+07:00"));
        assert!(prompt.contains("+07:00"));
    }

    #[test]
    fn compose_system_without_facts_is_just_the_prompt() {
        let system = compose_system("2026-06-11T15:00:00+07:00", &[]);
        assert_eq!(system, system_prompt("2026-06-11T15:00:00+07:00"));
    }

    #[test]
    fn compose_system_appends_the_facts_block() {
        let facts = vec![crate::assistant::memory::MemoryFact {
            fact: "Noah is the owner's son".into(),
            valid_at: None,
            name: "IS_SON_OF".into(),
        }];
        let system = compose_system("2026-06-11T15:00:00+07:00", &facts);
        assert!(system.starts_with(&system_prompt("2026-06-11T15:00:00+07:00")), "{system}");
        assert!(system.contains("Known facts about the owner"), "{system}");
        assert!(system.contains("- Noah is the owner's son"), "{system}");
    }

    #[test]
    fn system_prompt_mentions_the_memory_tools() {
        let prompt = system_prompt("2026-06-11T15:00:00+07:00");
        assert!(prompt.contains("search_memory"), "{prompt}");
        assert!(prompt.contains("remember"), "{prompt}");
    }

    #[test]
    fn system_prompt_mentions_the_agenda_tools() {
        let prompt = system_prompt("2026-06-12T15:00:00+07:00");
        assert!(prompt.contains("create_event"), "{prompt}");
        assert!(prompt.contains("list_events"), "{prompt}");
    }

    #[test]
    fn system_prompt_mentions_the_review_tools() {
        let prompt = system_prompt("2026-06-12T10:00:00+07:00");
        assert!(prompt.contains("list_pending_reviews"), "{prompt}");
        assert!(prompt.contains("confirm_review"), "{prompt}");
        assert!(prompt.contains("create_account"), "{prompt}");
        assert!(prompt.contains("list_instruments"), "{prompt}");
    }

    /// The detected account is only an OCR guess from the photo. When the owner
    /// says it's wrong, the assistant must override it via confirm_review — not
    /// give up because the item isn't flagged 'belum dikenali'.
    #[test]
    fn system_prompt_allows_overriding_a_wrongly_detected_account() {
        let prompt = system_prompt("2026-06-12T10:00:00+07:00").to_lowercase();
        assert!(
            prompt.contains("override"),
            "prompt must tell the assistant it can override a wrongly-detected account: {prompt}"
        );
    }

    #[test]
    fn system_prompt_mentions_the_project_tools() {
        let prompt = system_prompt("2026-06-13T10:00:00+07:00");
        assert!(prompt.contains("list_projects"), "{prompt}");
        assert!(prompt.contains("create_project"), "{prompt}");
        assert!(prompt.contains("create_task"), "{prompt}");
    }

    #[test]
    fn system_prompt_mentions_task_reading_tools() {
        let prompt = system_prompt("2026-06-13T10:00:00+07:00");
        assert!(prompt.contains("list_tasks"), "{prompt}");
        assert!(prompt.contains("complete_task"), "{prompt}");
    }

    #[test]
    fn system_prompt_mentions_billable() {
        let prompt = system_prompt("2026-06-13T10:00:00+07:00");
        assert!(prompt.contains("billable"), "{prompt}");
    }

    #[test]
    fn system_prompt_mentions_proposal_relay() {
        assert!(SYSTEM.contains("draft_proposal"));
        assert!(SYSTEM.contains("verbatim"));
    }

    #[test]
    fn system_prompt_mentions_invoicing() {
        let prompt = system_prompt("2026-06-13T10:00:00+07:00");
        assert!(prompt.contains("create_invoice"), "{prompt}");
    }

    #[tokio::test]
    async fn upload_event_seeds_model_and_stores_marker() {
        let db = mem_db().await;
        let model = ScriptedModel::new(vec![text_response("Aku baca beli QQQM Rp2jt, catat ke IBKR ya?")]);
        let reply = handle_upload_event(
            &db, &model, "telegram",
            "SEED-CTX-XYZ beli QQQM ke IBKR",
            "(kirim 1 bukti transaksi)",
        ).await.unwrap();
        assert_eq!(reply, "Aku baca beli QQQM Rp2jt, catat ke IBKR ya?");

        // The model saw the verbose seed as the trailing user message.
        let seen = model.messages_of_call(0);
        let last = seen.last().unwrap();
        assert_eq!(last["role"], "user");
        assert!(last["content"].as_str().unwrap().contains("SEED-CTX-XYZ"), "{last:?}");

        // Chat history stores the concise marker, not the seed.
        let history = crate::repo::chat::recent_by_channel(&db, "telegram", 10).await.unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].content, "(kirim 1 bukti transaksi)");
        assert_eq!(history[1].content, "Aku baca beli QQQM Rp2jt, catat ke IBKR ya?");
    }

    #[tokio::test]
    async fn followup_after_upload_sees_prior_question() {
        let db = mem_db().await;
        let model1 = ScriptedModel::new(vec![text_response("Beli QQQM Rp2jt, catat ke IBKR ya?")]);
        handle_upload_event(&db, &model1, "telegram", "seed", "(kirim 1 bukti transaksi)").await.unwrap();

        // The owner replies "iya"; the model must see its own prior question.
        let model2 = ScriptedModel::new(vec![text_response("Sip, kecatat.")]);
        handle_message(&db, &model2, "telegram", "iya").await.unwrap();
        let seen = model2.messages_of_call(0);
        assert!(
            seen.iter().any(|m| m["content"].as_str().is_some_and(|c| c.contains("IBKR"))),
            "follow-up turn should include the prior assistant question: {seen:?}"
        );
    }
}
