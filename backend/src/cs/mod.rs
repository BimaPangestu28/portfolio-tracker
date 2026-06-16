pub mod gate;
pub mod kb;
pub mod limiter;
pub mod tools;
pub mod dispatcher;
pub mod escalation;
pub mod agent;

use crate::db::Db;

/// Everything a CS tool call needs. Carries the embedder by reference behind a
/// trait object so `dispatch` stays generic-free and easy to call from the loop.
pub struct CsToolCtx<'a> {
    pub db:              &'a Db,
    pub embedder:        &'a dyn kb::Embedder,
    pub conversation_id: i64,
}

/// Borrow a required string argument from a tool-call input object.
pub fn str_arg<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input.get(key).and_then(|v| v.as_str()).filter(|s| !s.trim().is_empty())
}
