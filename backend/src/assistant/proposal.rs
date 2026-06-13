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
