# Upwork Proposal Draft (Assistant Tool) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a `draft_proposal` assistant tool that turns a pasted Upwork job description into a copy-paste-ready English proposal draft, tailored from the owner's long-term memory.

**Architecture:** A focused `assistant/proposal.rs` module mirrors the `assistant/proactive/compose.rs` pattern — pull relevant memory facts, assemble a deterministic data block, make one LLM call with a proposal-specific system prompt, fall back gracefully. A new tool definition + dispatcher arm expose it to the existing tool-use agent; a one-line persona hint tells the agent to relay the draft verbatim.

**Tech Stack:** Rust, the existing `assistant::memory::MemoryClient` (memory-service), `llm::claude::ClaudeClient` (DeepSeek V4 text), serde_json. No DB, no migration, no frontend.

---

## File Structure

| Path | Create/Modify | Responsibility |
|---|---|---|
| `backend/src/assistant/proposal.rs` | Create | `PROPOSAL_SYSTEM` prompt, pure `build_data_block`, async `draft()` orchestration + fallback. |
| `backend/src/assistant/mod.rs` | Modify | Declare `pub mod proposal;`. |
| `backend/src/assistant/tools.rs` | Modify | Add the `draft_proposal` tool definition; extend the two tool tests. |
| `backend/src/assistant/dispatcher.rs` | Modify | Route `draft_proposal` to a handler that validates `job_text` and calls `proposal::draft`. |
| `backend/src/assistant/agent.rs` | Modify | One sentence in the `SYSTEM` prompt: relay the proposal verbatim. |

**Verified facts about the codebase (do not re-derive):**
- `assistant::memory::MemoryClient::from_env() -> Option<MemoryClient>`; `client.search(query: &str, limit: u32) -> Vec<MemoryFact>` (failure-tolerant, logs internally, never errors); `render_facts_block(&[MemoryFact]) -> String` (returns `""` when empty, otherwise a block with its own header). `MemoryFact { pub fact: String, pub valid_at: Option<String>, pub name: String }`.
- `llm::claude::ClaudeClient::from_env() -> Result<ClaudeClient, LlmError>`; `client.complete(system: &str, parts: &[Part]) -> Result<String, LlmError>`; `Part::Text(String)`.
- `assistant::dispatcher` has `fn str_arg(input, key) -> Option<&str>` (returns None for absent/blank), and `dispatch(db: &Db, name: &str, input: &serde_json::Value) -> Result<String, String>` with a `match name { ... }`.
- `assistant::tools::definitions() -> serde_json::Value` (a JSON array). Tests `defines_all_tools_with_schemas` (asserts the exact ordered name list) and `required_fields_are_marked` must be updated when adding a tool.

---

## Task 1: `proposal.rs` — prompt + pure data-block builder

**Files:**
- Create: `backend/src/assistant/proposal.rs`
- Modify: `backend/src/assistant/mod.rs`

- [ ] **Step 1: Declare the module**

In `backend/src/assistant/mod.rs`, add `pub mod proposal;` after `pub mod memory;`:

```rust
pub mod memory;
pub mod proposal;
```

- [ ] **Step 2: Write `proposal.rs` with the prompt, the pure builder, and its tests**

Create `backend/src/assistant/proposal.rs`:

```rust
//! Draft an Upwork job proposal from a pasted job description, tailored with the
//! owner's long-term-memory facts. Mirrors the `proactive::compose` pattern: a
//! deterministic data block feeds one focused LLM call, with a graceful fallback.
//! The owner reviews and submits manually — nothing here submits anything.

use crate::assistant::memory::{render_facts_block, MemoryFact};

/// System prompt for the proposal writer. English output; never fabricate.
pub const PROPOSAL_SYSTEM: &str = "You write a single Upwork job proposal in professional English \
for the app owner, who will review and submit it manually. Use ONLY the facts provided in the data \
block — never invent experience, clients, metrics, or skills the owner did not state; if the facts \
are thin, keep claims general rather than fabricating specifics. Structure: open with a hook that \
shows you understood the client's stated need; then one or two sentences of relevant experience \
drawn from the provided facts; then a brief approach or first step; then a short, low-pressure call \
to action. Keep it roughly 120-200 words. Plain text only: no Markdown, no headers, no **bold**. \
Write in the owner's voice — confident but not boastful. Output only the proposal text, ready to \
copy and paste.";

/// Assemble the deterministic data block fed to the model. Pure: no network, no
/// LLM. Empty `notes` and empty `facts` sections are omitted.
pub fn build_data_block(job_text: &str, notes: Option<&str>, facts: &[MemoryFact]) -> String {
    let mut block = format!("JOB:\n{}\n", job_text.trim());
    if let Some(n) = notes.map(str::trim).filter(|s| !s.is_empty()) {
        block.push_str(&format!("\nNOTES:\n{n}\n"));
    }
    // render_facts_block returns "" when facts is empty, so the section self-omits.
    block.push_str(&render_facts_block(facts));
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(text: &str) -> MemoryFact {
        MemoryFact { fact: text.to_string(), valid_at: None, name: "REL".to_string() }
    }

    #[test]
    fn block_includes_job_and_omits_empty_sections() {
        let block = build_data_block("Need a Rust API", None, &[]);
        assert!(block.contains("JOB:"));
        assert!(block.contains("Need a Rust API"));
        assert!(!block.contains("NOTES:"));
        assert!(!block.contains("Known facts about the owner"));
    }

    #[test]
    fn block_includes_notes_and_facts_when_present() {
        let block = build_data_block(
            "Need a Rust API",
            Some("emphasize Rust, bid $30/hr"),
            &[fact("Built 3 production Rust backends")],
        );
        assert!(block.contains("NOTES:"));
        assert!(block.contains("emphasize Rust, bid $30/hr"));
        assert!(block.contains("Built 3 production Rust backends"));
    }

    #[test]
    fn blank_notes_are_omitted() {
        let block = build_data_block("job", Some("   "), &[]);
        assert!(!block.contains("NOTES:"));
    }

    #[test]
    fn prompt_demands_english_no_fabrication_plain_text() {
        let lower = PROPOSAL_SYSTEM.to_lowercase();
        assert!(lower.contains("english"));
        assert!(lower.contains("never invent"));
        assert!(lower.contains("no markdown"));
    }
}
```

- [ ] **Step 3: Run the tests to verify they pass**

Run: `cd backend && cargo test assistant::proposal::`
Expected: 4 tests PASS. (A `dead_code` warning on `PROPOSAL_SYSTEM`/`build_data_block` is expected until Task 2/4 wire them up.)

- [ ] **Step 4: Commit**

```bash
git add backend/src/assistant/proposal.rs backend/src/assistant/mod.rs
git commit -m "feat(proposal): system prompt + pure data-block builder"
```

---

## Task 2: `proposal.rs` — `draft()` orchestration

**Files:**
- Modify: `backend/src/assistant/proposal.rs`

- [ ] **Step 1: Add the orchestration function + fallback**

In `backend/src/assistant/proposal.rs`, add these above the `#[cfg(test)]` module:

```rust
use crate::assistant::memory::MemoryClient;
use crate::llm::claude::{ClaudeClient, Part};

/// How many memory facts to pull for tailoring.
const FACT_LIMIT: u32 = 8;
/// Cap the memory query length; a long job description is noise for retrieval.
const QUERY_MAX_CHARS: usize = 500;

/// The message returned when the LLM cannot produce a draft. Plain text the
/// agent relays as-is — never a partial proposal, never an auto-submit.
fn fallback() -> String {
    "⚠️ Couldn't draft the proposal right now (LLM unavailable). Please try again in a bit.".to_string()
}

/// Draft a proposal for `job_text`. Pulls memory facts (best-effort), builds the
/// data block, and makes one focused LLM call. Degrades to `fallback()` on any
/// LLM failure; degrades to no-facts on any memory failure (both logged).
pub async fn draft(job_text: &str, notes: Option<&str>) -> String {
    let facts = match MemoryClient::from_env() {
        Some(client) => {
            let query: String = job_text.chars().take(QUERY_MAX_CHARS).collect();
            client.search(&query, FACT_LIMIT).await
        }
        None => Vec::new(),
    };
    let block = build_data_block(job_text, notes, &facts);

    let client = match ClaudeClient::from_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("proposal draft: llm unavailable ({e}); using fallback");
            return fallback();
        }
    };
    match client.complete(PROPOSAL_SYSTEM, &[Part::Text(block)]).await {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => {
            tracing::warn!("proposal draft: empty reply; using fallback");
            fallback()
        }
        Err(e) => {
            tracing::warn!("proposal draft failed ({e}); using fallback");
            fallback()
        }
    }
}
```

- [ ] **Step 2: Add a test for the fallback message**

Add to the `tests` module in `backend/src/assistant/proposal.rs`:

```rust
    #[test]
    fn fallback_is_plain_and_non_committal() {
        let msg = fallback();
        assert!(msg.contains("Couldn't draft"));
        assert!(!msg.to_lowercase().contains("submitted"));
    }
```

- [ ] **Step 3: Run tests + build**

Run: `cd backend && cargo test assistant::proposal::`
Expected: 5 tests PASS.
Run: `cd backend && cargo build`
Expected: compiles (a `dead_code` warning on `draft` is expected until Task 4 wires the dispatcher).

- [ ] **Step 4: Commit**

```bash
git add backend/src/assistant/proposal.rs
git commit -m "feat(proposal): draft orchestration with memory + llm fallback"
```

---

## Task 3: Tool definition

**Files:**
- Modify: `backend/src/assistant/tools.rs`

- [ ] **Step 1: Update the `defines_all_tools_with_schemas` name list (test first)**

In `backend/src/assistant/tools.rs`, in the `tests` module, add `"draft_proposal",` as the LAST entry of the `vec![ ... ]` name list inside `defines_all_tools_with_schemas`, immediately after `"complete_task",`:

```rust
                "list_tasks",
                "complete_task",
                "draft_proposal",
            ]
        );
```

- [ ] **Step 2: Add a required-fields assertion (test first)**

In the same file, in `required_fields_are_marked`, add after the `complete_task` assertion:

```rust
        assert_eq!(find("draft_proposal")["input_schema"]["required"], serde_json::json!(["job_text"]));
```

- [ ] **Step 3: Run tests to verify they FAIL**

Run: `cd backend && cargo test assistant::tools::`
Expected: FAIL — the name list and `find("draft_proposal")` don't match because the tool isn't defined yet.

- [ ] **Step 4: Add the tool definition**

In `backend/src/assistant/tools.rs`, inside `definitions()`, add this object as the LAST element of the JSON array (immediately after the `complete_task` tool object, before the closing `])`):

```rust
        {
            "name": "draft_proposal",
            "description": "Draft an Upwork job proposal in professional English from a job description the user pastes. Pulls the owner's skills and experience from long-term memory to tailor it. Returns the draft for the user to review and submit manually — it never submits anything. Use when the owner pastes a job and asks for a proposal.",
            "input_schema": {
                "type": "object",
                "properties": {
                    "job_text": { "type": "string", "description": "The full Upwork job description the user pasted." },
                    "notes": { "type": "string", "description": "Optional emphasis or constraints, e.g. 'emphasize React, bid $30/hr'." }
                },
                "required": ["job_text"]
            }
        },
```

(Match the existing array's trailing-comma and brace style — each tool object is followed by a comma.)

- [ ] **Step 5: Run tests to verify they PASS**

Run: `cd backend && cargo test assistant::tools::`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/tools.rs
git commit -m "feat(proposal): draft_proposal tool definition"
```

---

## Task 4: Dispatcher wiring

**Files:**
- Modify: `backend/src/assistant/dispatcher.rs`

- [ ] **Step 1: Write the failing test**

In `backend/src/assistant/dispatcher.rs`, in its `#[cfg(test)] mod tests`, add a test that the handler errors when `job_text` is missing (this path needs no DB or external services):

```rust
    #[tokio::test]
    async fn draft_proposal_requires_job_text() {
        let err = super::draft_proposal(&serde_json::json!({})).await;
        assert!(err.is_err(), "missing job_text must error");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd backend && cargo test assistant::dispatcher::tests::draft_proposal_requires_job_text`
Expected: FAIL — `draft_proposal` function not found.

- [ ] **Step 3: Add the dispatch arm**

In `backend/src/assistant/dispatcher.rs`, in the `match name { ... }` inside `dispatch`, add this arm after the `"complete_task" => ...` arm (and before the `_ => Err(...)` fallback):

```rust
        "draft_proposal" => draft_proposal(input).await,
```

- [ ] **Step 4: Add the handler function**

In the same file, add this handler (near the other private handlers, e.g. after `search_memory`):

```rust
/// Draft an Upwork proposal from a pasted job. Validation only here; the
/// memory + LLM work lives in `assistant::proposal`.
async fn draft_proposal(input: &serde_json::Value) -> Result<String, String> {
    let job_text = str_arg(input, "job_text")
        .ok_or("missing required argument 'job_text' — paste the job description")?;
    let notes = str_arg(input, "notes");
    Ok(super::proposal::draft(job_text, notes).await)
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cd backend && cargo test assistant::dispatcher::tests::draft_proposal_requires_job_text`
Expected: PASS.

- [ ] **Step 6: Build to confirm the whole crate compiles (dead_code warnings should now be gone)**

Run: `cd backend && cargo build`
Expected: clean compile.

- [ ] **Step 7: Commit**

```bash
git add backend/src/assistant/dispatcher.rs
git commit -m "feat(proposal): dispatch draft_proposal tool"
```

---

## Task 5: Agent persona hint

**Files:**
- Modify: `backend/src/assistant/agent.rs`

- [ ] **Step 1: Write the failing test**

In `backend/src/assistant/agent.rs`, find the `#[cfg(test)] mod tests` block (or create one at the end of the file if none exists) and add:

```rust
    #[test]
    fn system_prompt_mentions_proposal_relay() {
        assert!(SYSTEM.contains("draft_proposal"));
        assert!(SYSTEM.contains("verbatim"));
    }
```

If the file has no `tests` module, add this at the end of the file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_mentions_proposal_relay() {
        assert!(SYSTEM.contains("draft_proposal"));
        assert!(SYSTEM.contains("verbatim"));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd backend && cargo test assistant::agent::tests::system_prompt_mentions_proposal_relay`
Expected: FAIL — `SYSTEM` doesn't contain those strings yet.

- [ ] **Step 3: Extend the `SYSTEM` prompt**

In `backend/src/assistant/agent.rs`, find the `SYSTEM` const string. It ends with `...so it can be \ninvoiced later.";`. Insert this sentence immediately before the closing `";` (continuing the same string literal — keep the trailing backslash line-continuation style used throughout):

```rust
 You can also draft Upwork job proposals: when the owner pastes a job and asks for a proposal \
(e.g. 'buatin proposal buat ini'), call draft_proposal with the pasted job_text (and notes if the \
owner specifies emphasis or a bid). The tool returns a ready-to-send English draft — relay it to \
the owner verbatim, without summarizing, translating, or reformatting it.
```

So the tail of the literal becomes:

```rust
... billable 10 juta'), pass billable=true and amount (in IDR) to create_task so it can be \
invoiced later. \
 You can also draft Upwork job proposals: when the owner pastes a job and asks for a proposal \
(e.g. 'buatin proposal buat ini'), call draft_proposal with the pasted job_text (and notes if the \
owner specifies emphasis or a bid). The tool returns a ready-to-send English draft — relay it to \
the owner verbatim, without summarizing, translating, or reformatting it.";
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd backend && cargo test assistant::agent::tests::system_prompt_mentions_proposal_relay`
Expected: PASS.

- [ ] **Step 5: Run the full assistant test suite + build**

Run: `cd backend && cargo test assistant::`
Expected: all PASS.
Run: `cd backend && cargo build`
Expected: clean compile.

- [ ] **Step 6: Commit**

```bash
git add backend/src/assistant/agent.rs
git commit -m "feat(proposal): teach agent to relay proposal drafts verbatim"
```

---

## Final verification

- [ ] `cd backend && cargo test` → all green (no failures; pre-existing ignored tests stay ignored).
- [ ] `cd backend && cargo build` → clean, no new warnings.
- [ ] **Manual smoke (optional, needs memory-service + LLM env configured):** in a chat/Telegram session, paste a job description and ask "buatin proposal buat ini"; confirm the agent calls `draft_proposal` and returns an English draft verbatim. With services unconfigured, confirm a missing `job_text` errors and a present `job_text` returns the fallback line (not a crash).

---

## Self-review notes (author)

- **Spec coverage:** module + prompt (Task 1), `draft()` orchestration with memory + LLM + fallback (Task 2), tool definition (Task 3), dispatch + `job_text` validation (Task 4), agent verbatim-relay hint (Task 5), error handling (fallback in Task 2, missing-job_text in Task 4, memory-failure tolerated via `search` in Task 2), testing (pure builder + prompt + tool schema + dispatch error + prompt-hint). Out-of-scope items (job-feed/API, storage, auto-submit, non-English, web UI) have no tasks — correct.
- **Type consistency:** `build_data_block(&str, Option<&str>, &[MemoryFact]) -> String`, `draft(&str, Option<&str>) -> String`, `PROPOSAL_SYSTEM`, `fallback()`, dispatcher `draft_proposal(&Value) -> Result<String,String>`, tool name `"draft_proposal"`, required `["job_text"]` — consistent across Tasks 1–5. `MemoryFact` fields and `render_facts_block`/`ClaudeClient`/`Part` signatures match the verified-facts list.
- **No DB/migration/portfolio/cashflow code touched.**
