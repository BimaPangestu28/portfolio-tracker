//! Tiny in-memory fixed-window rate limiter. No external deps — a process-global
//! map keyed by a caller-chosen string (IP and/or session). Suitable for a
//! single-tenant deployment; not distributed.

use std::collections::HashMap;

/// One caller's hit count within the current window.
#[derive(Clone, Copy)]
pub struct Window {
    pub window_start: u64, // unix seconds
    pub count: u32,
}

/// Pure core: returns true if the hit is ALLOWED, mutating `state` in place.
/// `now` is unix seconds, `window_secs` the bucket size, `max` the per-window cap.
pub fn check(
    state: &mut HashMap<String, Window>,
    key: &str,
    now: u64,
    window_secs: u64,
    max: u32,
) -> bool {
    let w = state.entry(key.to_string()).or_insert(Window { window_start: now, count: 0 });
    if now.saturating_sub(w.window_start) >= window_secs {
        w.window_start = now;
        w.count = 0;
    }
    if w.count >= max {
        return false;
    }
    w.count += 1;
    true
}

use std::sync::{Mutex, OnceLock};

fn global() -> &'static Mutex<HashMap<String, Window>> {
    static MAP: OnceLock<Mutex<HashMap<String, Window>>> = OnceLock::new();
    MAP.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Current unix seconds. Isolated so tests use the pure `check` directly.
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Process-global allow check. Returns true if the hit is allowed.
pub fn allow(key: &str, window_secs: u64, max: u32) -> bool {
    let mut map = match global().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    check(&mut map, key, now_secs(), window_secs, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_up_to_max_then_blocks_within_window() {
        let mut s = HashMap::new();
        for _ in 0..3 {
            assert!(check(&mut s, "ip-1", 100, 60, 3));
        }
        // 4th in the same window is blocked
        assert!(!check(&mut s, "ip-1", 100, 60, 3));
    }

    #[test]
    fn window_resets_after_elapsed_time() {
        let mut s = HashMap::new();
        assert!(check(&mut s, "ip-1", 100, 60, 1));
        assert!(!check(&mut s, "ip-1", 130, 60, 1)); // still in window
        assert!(check(&mut s, "ip-1", 161, 60, 1));  // window elapsed -> reset
    }

    #[test]
    fn keys_are_independent() {
        let mut s = HashMap::new();
        assert!(check(&mut s, "a", 100, 60, 1));
        assert!(check(&mut s, "b", 100, 60, 1)); // different key unaffected
        assert!(!check(&mut s, "a", 100, 60, 1));
    }
}
