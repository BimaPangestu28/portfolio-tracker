//! Server-side authentication: master-password login + JWT issue/verify.
//!
//! Enforced only when both AUTH_PASSWORD and JWT_SECRET are set. When unset
//! (local dev / tests), login returns a placeholder token and the middleware
//! allows all requests — mirroring the gateway-token `None => allow` pattern.

use constant_time_eq::constant_time_eq;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub iat: usize,
    pub exp: usize,
}

// ── env config ───────────────────────────────────────────────────────────────

fn auth_password() -> Option<String> {
    std::env::var("AUTH_PASSWORD")
        .ok()
        .filter(|s| !s.is_empty())
}

pub fn jwt_secret() -> Option<String> {
    std::env::var("JWT_SECRET").ok().filter(|s| !s.is_empty())
}

fn ttl_days() -> i64 {
    std::env::var("AUTH_TOKEN_TTL_DAYS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30)
}

/// True when both password and secret are configured (auth is enforced).
pub fn is_configured() -> bool {
    auth_password().is_some() && jwt_secret().is_some()
}

/// Validate the auth-related env config, failing closed on a partial setup.
///
/// Both set => enforced; both unset => dev/open; exactly one set => error.
///
/// @returns `Ok(())` when the config is coherent
/// @throws `String` describing the misconfiguration when exactly one of
///         AUTH_PASSWORD / JWT_SECRET is set
pub fn validate_env_config() -> Result<(), String> {
    check_config(auth_password().is_some(), jwt_secret().is_some())
}

// ── env-reading wrappers (used by handlers/middleware) ───────────────────────

pub fn password_ok(candidate: &str) -> bool {
    password_ok_with(auth_password().as_deref(), candidate)
}

pub fn issue_token(now: i64) -> anyhow::Result<String> {
    issue_token_with(jwt_secret().as_deref(), ttl_days(), now)
}

// ── pure logic (unit-tested) ─────────────────────────────────────────────────

/// Error when exactly one of AUTH_PASSWORD / JWT_SECRET is set. Both-set =
/// enforced; both-unset = dev/open; one-set = almost certainly a mistake.
pub fn check_config(password_set: bool, secret_set: bool) -> Result<(), String> {
    if password_set != secret_set {
        return Err(
            "AUTH_PASSWORD and JWT_SECRET must be set together (or both unset)".into(),
        );
    }
    Ok(())
}

/// Constant-time password comparison. `None` expected => accept anything (dev).
pub fn password_ok_with(expected: Option<&str>, candidate: &str) -> bool {
    match expected {
        Some(pw) => constant_time_eq(pw.as_bytes(), candidate.as_bytes()),
        None => true,
    }
}

/// Issue a signed JWT. `None` secret => dev placeholder token.
pub fn issue_token_with(secret: Option<&str>, ttl_days: i64, now: i64) -> anyhow::Result<String> {
    let Some(secret) = secret else {
        return Ok("dev-token".into());
    };
    let claims = Claims {
        sub: "owner".into(),
        iat: now as usize,
        exp: (now + ttl_days * 86_400) as usize,
    };
    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )?;
    Ok(token)
}

/// Verify a JWT's signature + expiry. `None` secret => dev mode, always true.
pub fn verify_token_with(secret: Option<&str>, token: &str) -> bool {
    let Some(secret) = secret else {
        return true;
    };
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .is_ok()
}

/// Parse the token out of an `Authorization: Bearer <token>` header value.
pub fn parse_bearer(header: &str) -> Option<&str> {
    header.strip_prefix("Bearer ")
}

/// Authorization decision for the middleware. Not configured => allow.
pub fn authorize(configured: bool, secret: Option<&str>, auth_header: Option<&str>) -> bool {
    if !configured {
        return true;
    }
    match auth_header.and_then(parse_bearer) {
        Some(token) => verify_token_with(secret, token),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_config_ok_when_both_set() {
        assert!(check_config(true, true).is_ok());
    }

    #[test]
    fn check_config_ok_when_both_unset() {
        assert!(check_config(false, false).is_ok());
    }

    #[test]
    fn check_config_errs_when_only_password_set() {
        assert!(check_config(true, false).is_err());
    }

    #[test]
    fn check_config_errs_when_only_secret_set() {
        assert!(check_config(false, true).is_err());
    }

    #[test]
    fn password_ok_with_none_allows_anything() {
        assert!(password_ok_with(None, "whatever"));
    }

    #[test]
    fn password_ok_with_value_requires_match() {
        assert!(password_ok_with(Some("secret"), "secret"));
        assert!(!password_ok_with(Some("secret"), "wrong"));
    }

    #[test]
    fn issued_token_verifies_with_same_secret() {
        // Use the current time so the token's exp is in the future — verify
        // checks exp against the real wall clock.
        let now = chrono::Utc::now().timestamp();
        let token = issue_token_with(Some("k"), 30, now).unwrap();
        assert!(verify_token_with(Some("k"), &token));
    }

    #[test]
    fn token_fails_with_wrong_secret() {
        let now = chrono::Utc::now().timestamp();
        let token = issue_token_with(Some("k"), 30, now).unwrap();
        assert!(!verify_token_with(Some("other"), &token));
    }

    #[test]
    fn expired_token_fails() {
        // issued far in the past with a 1-day ttl -> expired relative to "now"
        let past = 1_000_000;
        let token = issue_token_with(Some("k"), 1, past).unwrap();
        // verify uses real wall-clock "now", which is well beyond past+1day
        assert!(!verify_token_with(Some("k"), &token));
    }

    #[test]
    fn no_secret_is_dev_mode() {
        let token = issue_token_with(None, 30, 0).unwrap();
        assert_eq!(token, "dev-token");
        assert!(verify_token_with(None, "anything"));
    }

    #[test]
    fn authorize_open_when_not_configured() {
        assert!(authorize(false, None, None));
        assert!(authorize(false, None, Some("garbage")));
    }

    #[test]
    fn authorize_requires_valid_bearer_when_configured() {
        let now = chrono::Utc::now().timestamp();
        let token = issue_token_with(Some("k"), 30, now).unwrap();
        let header = format!("Bearer {token}");
        assert!(authorize(true, Some("k"), Some(&header)));
        assert!(!authorize(true, Some("k"), None));
        assert!(!authorize(true, Some("k"), Some("Bearer nonsense")));
        assert!(!authorize(true, Some("k"), Some(&token))); // missing "Bearer " prefix
    }
}
