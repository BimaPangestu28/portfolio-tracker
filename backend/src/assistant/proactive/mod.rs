//! Proactive sends: morning briefing, weekly recap, financial alerts.
//! Deterministic gathering → LLM composition (with fallback) → Telegram.

pub mod tick;
pub mod compose;
pub mod alerts;
