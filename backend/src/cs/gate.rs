//! Public-channel gatekeeping: env config validation, Origin allowlist, widget
//! site-key check, and opaque session-token generation.

use rand::RngCore;

/// `Ok` when CS public config is coherent: both `CS_ALLOWED_ORIGINS` and
/// `CS_WIDGET_KEY` set (enabled), or both unset (disabled). Exactly one => error.
pub fn validate_config() -> Result<(), String> {
    check_config(
        std::env::var("CS_ALLOWED_ORIGINS").is_ok(),
        std::env::var("CS_WIDGET_KEY").is_ok(),
    )
}

pub fn check_config(origins_set: bool, key_set: bool) -> Result<(), String> {
    if origins_set != key_set {
        return Err("CS_ALLOWED_ORIGINS and CS_WIDGET_KEY must be set together (or both unset)".into());
    }
    Ok(())
}

/// True when the CS public channel is enabled (both env vars present).
pub fn is_enabled() -> bool {
    std::env::var("CS_ALLOWED_ORIGINS").is_ok() && std::env::var("CS_WIDGET_KEY").is_ok()
}

/// Split a comma-separated origins string, trimming and dropping empties.
pub fn parse_origins(raw: &str) -> Vec<String> {
    raw.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect()
}

/// The configured allowlist from env (empty if unset).
pub fn allowed_origins() -> Vec<String> {
    std::env::var("CS_ALLOWED_ORIGINS").map(|v| parse_origins(&v)).unwrap_or_default()
}

/// Exact-match origin check. Fails closed: empty allowlist or missing Origin => false.
pub fn origin_allowed(allow: &[String], origin: Option<&str>) -> bool {
    match origin {
        Some(o) => allow.iter().any(|a| a == o),
        None => false,
    }
}

/// Compare the presented site-key against the configured one. Rejects when no
/// key is configured (the endpoint must be explicitly enabled).
pub fn site_key_ok(configured: Option<&str>, presented: Option<&str>) -> bool {
    match (configured, presented) {
        (Some(c), Some(p)) => c == p,
        _ => false,
    }
}

/// 32 random bytes, hex-encoded — an opaque, unguessable session token.
pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_config_requires_both_or_neither() {
        assert!(check_config(false, false).is_ok()); // disabled
        assert!(check_config(true, true).is_ok());    // enabled
        assert!(check_config(true, false).is_err());  // partial
        assert!(check_config(false, true).is_err());
    }

    #[test]
    fn parse_origins_splits_and_trims() {
        let o = parse_origins("https://a.com, https://b.com ,, https://c.com");
        assert_eq!(o, vec!["https://a.com", "https://b.com", "https://c.com"]);
    }

    #[test]
    fn origin_allowed_exact_match_only() {
        let allow = vec!["https://shop.com".to_string()];
        assert!(origin_allowed(&allow, Some("https://shop.com")));
        assert!(!origin_allowed(&allow, Some("https://evil.com")));
        assert!(!origin_allowed(&allow, None));
        // empty allowlist denies everything (fail closed)
        assert!(!origin_allowed(&[], Some("https://shop.com")));
    }

    #[test]
    fn site_key_constant_check() {
        assert!(site_key_ok(Some("secret"), Some("secret")));
        assert!(!site_key_ok(Some("secret"), Some("wrong")));
        assert!(!site_key_ok(Some("secret"), None));
        // when no key configured, reject (public endpoint must be explicitly enabled)
        assert!(!site_key_ok(None, Some("anything")));
    }

    #[test]
    fn session_tokens_are_unique_and_long() {
        let a = new_session_token();
        let b = new_session_token();
        assert_ne!(a, b);
        assert!(a.len() >= 32);
    }
}
