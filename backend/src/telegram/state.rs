//! In-memory Telegram linking state.
//!
//! Holds the active one-time link code (10-minute TTL, consumed on first
//! successful verification) and whether the bot token was rejected by
//! Telegram. Ephemeral by design — a backend restart simply requires
//! generating a fresh code.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// A link code older than this is rejected.
const CODE_TTL: Duration = Duration::from_secs(600);

/// Surface CODE_TTL to the API layer (expires_in in seconds).
pub const CODE_TTL_SECS: u64 = CODE_TTL.as_secs();

#[derive(Debug, Default)]
pub struct TgState {
    /// Active link code and when it was generated.
    code: Option<(String, Instant)>,
    /// Set by the poller when Telegram rejects the bot token (401).
    auth_failed: bool,
}

pub type SharedTgState = Arc<Mutex<TgState>>;

impl TgState {
    /// Generate a fresh 6-digit link code, replacing any previous one.
    pub fn generate_code(&mut self, now: Instant) -> String {
        let code = format!("{:06}", rand::random::<u32>() % 1_000_000);
        self.code = Some((code.clone(), now));
        code
    }

    /// Check `input` against the active code. A match consumes the code
    /// (single-use); a mismatch leaves it in place for another attempt.
    pub fn verify_code(&mut self, input: &str, now: Instant) -> bool {
        let matches = match &self.code {
            Some((code, created)) => {
                now.duration_since(*created) <= CODE_TTL && input.trim() == code
            }
            None => false,
        };
        if matches {
            self.code = None;
        }
        matches
    }

    /// Record that Telegram rejected the bot token (401).
    pub fn set_auth_failed(&mut self) {
        self.auth_failed = true;
    }

    /// Whether the bot token was rejected by Telegram.
    pub fn auth_failed(&self) -> bool {
        self.auth_failed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_code_is_six_digits() {
        let mut state = TgState::default();
        let code = state.generate_code(Instant::now());
        assert_eq!(code.len(), 6);
        assert!(code.chars().all(|c| c.is_ascii_digit()), "non-digit in {code}");
    }

    #[test]
    fn fresh_code_verifies_once_then_is_consumed() {
        let mut state = TgState::default();
        let now = Instant::now();
        let code = state.generate_code(now);
        assert!(state.verify_code(&code, now));
        // Single-use: the same code must not verify twice.
        assert!(!state.verify_code(&code, now));
    }

    #[test]
    fn wrong_code_is_rejected_and_does_not_consume() {
        let mut state = TgState::default();
        let now = Instant::now();
        let code = state.generate_code(now);
        assert!(!state.verify_code("000000", now));
        // The real code still works after a failed attempt.
        assert!(state.verify_code(&code, now));
    }

    #[test]
    fn expired_code_is_rejected() {
        let mut state = TgState::default();
        let created = Instant::now();
        let code = state.generate_code(created);
        let later = created + CODE_TTL + Duration::from_secs(1);
        assert!(!state.verify_code(&code, later));
    }

    #[test]
    fn verify_trims_surrounding_whitespace() {
        let mut state = TgState::default();
        let now = Instant::now();
        let code = state.generate_code(now);
        assert!(state.verify_code(&format!("  {code} \n"), now));
    }

    #[test]
    fn regenerating_invalidates_the_previous_code() {
        let mut state = TgState::default();
        let now = Instant::now();
        let first = state.generate_code(now);
        let second = state.generate_code(now);
        assert!(!state.verify_code(&first, now) || first == second);
        assert!(state.verify_code(&second, now) || first == second);
    }

    #[test]
    fn auth_failed_flag_round_trips() {
        let mut state = TgState::default();
        assert!(!state.auth_failed());
        state.set_auth_failed();
        assert!(state.auth_failed());
    }
}
