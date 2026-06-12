//! Google OAuth: env config, signed `state`, consent URL, and token
//! exchange/refresh. The `state` is a short-lived JWT signed with JWT_SECRET,
//! so the unauthenticated callback can trust it (CSRF guard).

use serde::{Deserialize, Serialize};

const AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const REVOKE_ENDPOINT: &str = "https://oauth2.googleapis.com/revoke";
pub const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
const STATE_TTL_SECS: i64 = 600;

/// OAuth client config from the environment.
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OAuthConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client_id: std::env::var("GOOGLE_CLIENT_ID")
                .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_ID is not set"))?,
            client_secret: std::env::var("GOOGLE_CLIENT_SECRET")
                .map_err(|_| anyhow::anyhow!("GOOGLE_CLIENT_SECRET is not set"))?,
            redirect_uri: std::env::var("GOOGLE_REDIRECT_URI")
                .map_err(|_| anyhow::anyhow!("GOOGLE_REDIRECT_URI is not set"))?,
        })
    }
}

fn enc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Build the Google consent URL. `state` is the signed CSRF token.
pub fn consent_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{AUTH_ENDPOINT}?response_type=code&access_type=offline&prompt=consent\
         &client_id={}&redirect_uri={}&scope={}&state={}",
        enc(client_id), enc(redirect_uri), enc(SCOPE), enc(state)
    )
}

#[derive(Serialize, Deserialize)]
struct StateClaims {
    exp: i64,
    nonce: String,
}

pub fn sign_state_with(secret: &str, now: i64) -> anyhow::Result<String> {
    use jsonwebtoken::{encode, EncodingKey, Header};
    let mut nonce = [0u8; 16];
    rand::RngCore::fill_bytes(&mut rand::thread_rng(), &mut nonce);
    let claims = StateClaims {
        exp: now + STATE_TTL_SECS,
        nonce: base64::Engine::encode(&base64::engine::general_purpose::STANDARD, nonce),
    };
    Ok(encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))?)
}

pub fn verify_state_with(secret: &str, token: &str, now: i64) -> bool {
    use jsonwebtoken::{decode, DecodingKey, Validation};
    let mut validation = Validation::default();
    validation.validate_exp = false;
    match decode::<StateClaims>(token, &DecodingKey::from_secret(secret.as_bytes()), &validation) {
        Ok(data) => now <= data.claims.exp,
        Err(_) => false,
    }
}

/// Tokens returned by the code-exchange / refresh endpoints.
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    pub expires_in: i64,
    #[serde(default)]
    pub scope: Option<String>,
}

/// Compute an RFC3339 expiry from `expires_in` seconds, with a 60s safety skew.
pub fn expiry_from_now(expires_in: i64) -> String {
    (chrono::Utc::now() + chrono::Duration::seconds(expires_in - 60)).to_rfc3339()
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(cfg: &OAuthConfig, code: &str) -> anyhow::Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
            ("redirect_uri", cfg.redirect_uri.as_str()),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "token exchange failed: {} {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    Ok(resp.json().await?)
}

/// Refresh the access token using the stored refresh token.
pub async fn refresh_access(cfg: &OAuthConfig, refresh_token: &str) -> anyhow::Result<TokenResponse> {
    let client = reqwest::Client::new();
    let resp = client
        .post(TOKEN_ENDPOINT)
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", cfg.client_id.as_str()),
            ("client_secret", cfg.client_secret.as_str()),
        ])
        .send()
        .await?;
    if !resp.status().is_success() {
        anyhow::bail!(
            "token refresh failed: {} {}",
            resp.status(),
            resp.text().await.unwrap_or_default()
        );
    }
    Ok(resp.json().await?)
}

/// Best-effort token revocation on disconnect.
pub async fn revoke(token: &str) -> anyhow::Result<()> {
    reqwest::Client::new()
        .post(REVOKE_ENDPOINT)
        .form(&[("token", token)])
        .send()
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_url_has_required_params() {
        let url = consent_url("client-123", "https://app/api/google/oauth/callback", "STATEVAL");
        assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
        assert!(url.contains("client_id=client-123"));
        assert!(url.contains("access_type=offline"));
        assert!(url.contains("prompt=consent"));
        assert!(url.contains("state=STATEVAL"));
        assert!(url.contains("scope=https%3A%2F%2Fwww.googleapis.com%2Fauth%2Fcalendar.events"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp%2Fapi%2Fgoogle%2Foauth%2Fcallback"));
    }

    #[test]
    fn state_round_trips_and_rejects_tampering() {
        let secret = "test-jwt-secret";
        let now = 1_000_000;
        let token = sign_state_with(secret, now).unwrap();
        assert!(verify_state_with(secret, &token, now + 60));
        assert!(!verify_state_with(secret, &token, now + 601));
        assert!(!verify_state_with("other-secret", &token, now + 60));
    }
}
