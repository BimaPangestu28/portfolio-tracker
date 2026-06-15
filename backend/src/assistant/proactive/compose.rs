//! One LLM call turns a deterministic data block into natural Indonesian
//! prose; any failure falls back to sending the data block itself.

use crate::llm::claude::Part;

pub const BRIEFING_SYSTEM: &str = "You write a short daily morning plan in Indonesian \
for the app owner, delivered over Telegram. Use ONLY the data block provided — copy every \
number exactly as written, never invent or recalculate anything. Plain text only: no Markdown, \
no headers, no **bold**, no tables. At most 15 short lines; use emoji sparingly as bullets. \
Frame it as a plan for the day, not a flat list: open with a one-line greeting (day and date), \
then the agenda at its fixed times, then the todos in the order given (highest priority first) — \
suggest a sensible flow around the events. Add a one-or-two-line portfolio summary (net worth, \
change, notable movers, pending reviews when present), remembered facts only if clearly relevant \
today, and one short grounded closing line. Skip any section whose data is empty.";

pub const RECAP_SYSTEM: &str = "You write a short weekly recap in Indonesian for the app \
owner, delivered over Telegram on Sunday evening. Use ONLY the data block provided — copy \
every number exactly as written, never invent or recalculate anything. Plain text only: no \
Markdown, no headers, no **bold**, no tables. At most 15 short lines; use emoji sparingly. \
Structure: one opening line; productivity (todos done vs created, reminders delivered); the \
week's finances (net worth change, top movers, spending); what's coming next week; one short, \
grounded closing line. Skip any section whose data is empty.";

pub const REVIEW_SYSTEM: &str = "You write a short daily evening review in Indonesian for the \
app owner, delivered over Telegram. Use ONLY the data block provided — copy every item exactly, \
never invent anything. Plain text only: no Markdown, no headers, no **bold**, no tables. At most \
12 short lines; use emoji sparingly. Structure: one warm opening line; what got done today; what \
is still unfinished (overdue or due today). End with exactly one question offering to move the \
unfinished todos to tomorrow, e.g. 'Mau aku geser yang belum kelar ke besok? Balas iya ya.' If \
nothing is unfinished, congratulate briefly and skip the question.";

pub const MONTHLY_RECAP_SYSTEM: &str = "You write a short monthly recap in Indonesian for the \
app owner, delivered over Telegram on the 1st. Use ONLY the data block provided — copy every \
number exactly, never invent anything. Plain text only: no Markdown, no headers, no **bold**, no \
tables. At most 15 short lines; use emoji sparingly. Structure: one opening line naming the month; \
productivity (todos done); the month's finances (net worth change, money in/out, freelance \
invoiced); one short grounded closing line. Skip any section whose data is empty.";

/// The message sent when the LLM is unavailable or returns nothing usable.
pub fn fallback_message(header: &str, data_block: &str) -> String {
    format!("{header}\n{data_block}")
}

/// Compose prose from the data block, degrading to the plain block on any
/// LLM failure — an ugly briefing beats a missing one.
pub async fn compose(system: &str, data_block: &str, fallback_header: &str) -> String {
    let llm = match crate::llm::claude::ClaudeClient::from_env() {
        Ok(client) => client,
        Err(e) => {
            tracing::warn!("proactive compose: llm unavailable ({e}); using fallback");
            return fallback_message(fallback_header, data_block);
        }
    };
    match llm.complete(system, &[Part::Text(data_block.to_string())]).await {
        Ok(text) if !text.trim().is_empty() => text,
        Ok(_) => {
            tracing::warn!("proactive compose: empty reply; using fallback");
            fallback_message(fallback_header, data_block)
        }
        Err(e) => {
            tracing::warn!("proactive compose failed ({e}); using fallback");
            fallback_message(fallback_header, data_block)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_prefixes_the_header() {
        let msg = fallback_message("📋 Briefing (mode ringkas)", "Todo:\n- bayar listrik");
        assert_eq!(msg, "📋 Briefing (mode ringkas)\nTodo:\n- bayar listrik");
    }

    #[test]
    fn prompts_demand_exact_numbers_and_plain_text() {
        for prompt in [BRIEFING_SYSTEM, RECAP_SYSTEM, REVIEW_SYSTEM, MONTHLY_RECAP_SYSTEM] {
            let lower = prompt.to_lowercase();
            assert!(lower.contains("indonesian"), "{prompt}");
            assert!(lower.contains("exactly"), "{prompt}");
            assert!(lower.contains("no markdown"), "{prompt}");
        }
    }
}
