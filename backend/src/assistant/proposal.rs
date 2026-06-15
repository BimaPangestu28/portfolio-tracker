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

    #[test]
    fn fallback_is_plain_and_non_committal() {
        let msg = fallback();
        assert!(msg.contains("Couldn't draft"));
        assert!(!msg.to_lowercase().contains("submitted"));
    }
}
