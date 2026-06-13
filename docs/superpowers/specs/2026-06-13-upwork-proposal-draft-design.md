# Upwork Proposal Draft (Assistant Tool) — Design

**Date:** 2026-06-13
**Status:** Approved (design); pending implementation plan
**Scope:** Sub-project 3 of the Upwork integration. Generate a copy-paste-ready Upwork
proposal draft from a pasted job description, via the existing assistant (Telegram/chat).

---

## 1. Purpose

Let the owner paste an Upwork job description into the assistant (Telegram/chat) and get back a
ready-to-submit **English** proposal draft, tailored using the owner's skills/experience already
held in the assistant's long-term memory.

The owner reviews and submits manually — the tool **never** submits anything (Upwork API auto-apply
is forbidden and out of scope).

### Non-goals (v1)
- Pulling the job from the Upwork API / job feed (manual paste only; the job-feed sub-project is
  separate and API-key gated).
- Storing/managing generated proposals or tracking win-rate.
- Auto-submitting proposals.
- Languages other than English.
- A dedicated web UI (the assistant chat/Telegram surface is the only entry point).

---

## 2. Context

The assistant already has the pieces this feature composes:

- **Tool-use loop** — `assistant/agent.rs` runs the LLM with `assistant/tools.rs::definitions()`
  and dispatches calls via `assistant/dispatcher.rs`.
- **Long-term memory** — `assistant/memory.rs`: `MemoryClient::from_env()`,
  `search(query, limit) -> Vec<MemoryFact>`, and `render_facts_block(&[MemoryFact]) -> String`,
  backed by the `memory-service`.
- **Single-turn text generation** — `llm::claude::ClaudeClient::from_env()` then
  `complete(system, &[Part::Text(block)])` (DeepSeek V4, text). The established
  "deterministic data block → one LLM call → prose, with fallback" pattern lives in
  `assistant/proactive/compose.rs`.

This feature adds a focused `draft_proposal` tool that mirrors the `compose.rs` pattern: it pulls
relevant memory facts, assembles a deterministic data block, and makes one focused LLM call with a
proposal-specific system prompt.

---

## 3. Components

### 3.1 New module `backend/src/assistant/proposal.rs`

- **`PROPOSAL_SYSTEM: &str`** — the proposal-writing system prompt. Requirements encoded in it:
  - Write in **professional English**.
  - Structure: a hook addressing the client's stated need → relevant experience drawn from the
    provided facts → a brief approach/plan → a light call-to-action.
  - ~120–200 words, **plain text** (no Markdown, no headers, no bold), copy-paste ready.
  - **Use only the provided facts; never fabricate experience, clients, or numbers.** If facts are
    thin, keep claims general rather than inventing specifics.

- **`build_data_block(job_text: &str, notes: Option<&str>, facts: &[MemoryFact]) -> String`** —
  pure function. Assembles a deterministic block:
  ```
  JOB:
  <job_text>

  NOTES:            (omitted if notes is None/empty)
  <notes>

  ABOUT ME (verified facts — do not exceed these):
  <render_facts_block(facts)>   (omitted if facts is empty)
  ```
  Empty sections are skipped. No network, no LLM — fully unit-testable.

- **`async fn draft(job_text: &str, notes: Option<&str>) -> String`** — orchestrates:
  1. `MemoryClient::from_env()`; if present, `search(query, FACT_LIMIT)` where `query` is derived
     from `job_text` (truncated to a reasonable length, e.g. first 500 chars). On memory failure or
     absence → empty facts (logged), proceed.
  2. `build_data_block(job_text, notes, &facts)`.
  3. `ClaudeClient::from_env()` then `complete(PROPOSAL_SYSTEM, &[Part::Text(block)])`.
  4. Return the draft on success; on LLM-unavailable / empty / error, return a clear fallback
     string (e.g. `"⚠️ Couldn't draft the proposal right now (LLM unavailable). Please try again."`).
     The fallback is a plain message the agent relays — it is **not** a partial/auto-submitted
     proposal.

  `FACT_LIMIT` constant (e.g. 8).

### 3.2 Tool definition — `backend/src/assistant/tools.rs`

Add to `definitions()`:
```json
{
  "name": "draft_proposal",
  "description": "Draft an Upwork proposal in English from a job description the user pastes. Pulls the owner's skills/experience from memory. Returns the draft for the user to review and submit manually; never submits anything.",
  "input_schema": {
    "type": "object",
    "properties": {
      "job_text": { "type": "string", "description": "The full Upwork job description the user pasted." },
      "notes":    { "type": "string", "description": "Optional emphasis or constraints, e.g. 'emphasize React, bid $30/hr'." }
    },
    "required": ["job_text"]
  }
}
```

### 3.3 Dispatch — `backend/src/assistant/dispatcher.rs`

Add a match arm for `"draft_proposal"`:
- Parse `job_text` (required; missing/empty → `Err` with a message telling the user to paste the
  job). Parse optional `notes`.
- Call `crate::assistant::proposal::draft(&job_text, notes.as_deref()).await`.
- Return `Ok(draft)`.

### 3.4 Agent persona hint — `backend/src/assistant/agent.rs`

Add one line to the system prompt: when the `draft_proposal` tool returns a proposal, relay it to
the user **verbatim** (so it stays copy-paste ready) rather than summarizing or rephrasing it.

---

## 4. Data flow

```
User (Telegram/chat): "buatin proposal buat ini: <pasted job>"
  → agent LLM recognizes intent, calls draft_proposal{ job_text, notes? }
    → dispatcher → proposal::draft()
        → MemoryClient.search(query from job)  → facts
        → build_data_block(job, notes, facts)  → block
        → ClaudeClient.complete(PROPOSAL_SYSTEM, block) → English draft
    → tool returns draft
  → agent relays the draft verbatim → user copies & submits on Upwork manually
```

No portfolio, cashflow, or Upwork-API code is touched. No new DB tables or migrations.

---

## 5. Error handling

| Condition | Behavior |
|---|---|
| `job_text` missing/empty | Tool returns `Err`; agent asks the user to paste the job. |
| Memory unavailable / search fails | Proceed with empty facts (proposal less personal), logged via `tracing::warn!`. |
| LLM unavailable / empty / error | Return the plain fallback message; agent relays it. Never a partial proposal, never auto-submit. |

---

## 6. Testing (TDD)

- **`build_data_block`** (pure, table-driven): asserts the block contains the job text; includes
  `NOTES`/`ABOUT ME` sections when provided and **omits them when empty**; renders facts via
  `render_facts_block`.
- **`PROPOSAL_SYSTEM`** assertions (mirror `compose.rs` prompt tests): contains "English",
  a do-not-fabricate instruction ("only" + "facts" / "never invent"), and "no markdown".
- **`tools.rs`** — extend the existing `defines_all_tools_with_schemas` / `required_fields_are_marked`
  tests so `draft_proposal` is present and `job_text` is in `required`.
- **`dispatcher.rs`** — dispatching `draft_proposal` with empty/missing `job_text` returns an error
  (the generation path itself needs the LLM/memory services, so it is exercised only by the pure
  data-block + prompt tests, not a live unit test).

---

## 7. Out of scope (restated)

Job-feed/API ingestion, proposal storage & win-tracking, auto-submit, non-English output, web UI.
These are deliberately excluded from v1; the design leaves room for a future job-feed sub-project to
pass `job_text` from a stored marketplace job instead of a manual paste.
