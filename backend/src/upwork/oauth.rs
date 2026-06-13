//! Upwork OAuth2: env config, consent URL, and code exchange / refresh. The
//! signed `state` CSRF helpers and the token-response shape are reused from
//! `google::oauth`. Upwork scope is governed by the API key's configured
//! permissions, so no scope param is sent in the consent URL.

pub use crate::google::oauth::{expiry_from_now, TokenResponse};

const AUTHORIZE_ENDPOINT: &str = "https://www.upwork.com/ab/account-security/oauth2/authorize";
const TOKEN_ENDPOINT: &str = "https://www.upwork.com/api/v3/oauth2/token";

pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
}

impl OAuthConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            client_id: std::env::var("UPWORK_CLIENT_ID")
                .map_err(|_| anyhow::anyhow!("UPWORK_CLIENT_ID is not set"))?,
            client_secret: std::env::var("UPWORK_CLIENT_SECRET")
                .map_err(|_| anyhow::anyhow!("UPWORK_CLIENT_SECRET is not set"))?,
            redirect_uri: std::env::var("UPWORK_REDIRECT_URI")
                .map_err(|_| anyhow::anyhow!("UPWORK_REDIRECT_URI is not set"))?,
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

/// Build the Upwork consent URL. `state` is the signed CSRF token.
pub fn consent_url(client_id: &str, redirect_uri: &str, state: &str) -> String {
    format!(
        "{AUTHORIZE_ENDPOINT}?response_type=code&client_id={}&redirect_uri={}&state={}",
        enc(client_id), enc(redirect_uri), enc(state)
    )
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(cfg: &OAuthConfig, code: &str) -> anyhow::Result<TokenResponse> {
    let resp = reqwest::Client::new()
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
        anyhow::bail!("upwork token exchange failed: {} {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

/// Refresh the access token using the stored refresh token.
pub async fn refresh_access(cfg: &OAuthConfig, refresh_token: &str) -> anyhow::Result<TokenResponse> {
    let resp = reqwest::Client::new()
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
        anyhow::bail!("upwork token refresh failed: {} {}", resp.status(), resp.text().await.unwrap_or_default());
    }
    Ok(resp.json().await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_url_has_required_params() {
        let url = consent_url("cid-1", "https://app/api/upwork/oauth/callback", "STATE");
        assert!(url.starts_with("https://www.upwork.com/ab/account-security/oauth2/authorize?"));
        assert!(url.contains("response_type=code"));
        assert!(url.contains("client_id=cid-1"));
        assert!(url.contains("state=STATE"));
        assert!(url.contains("redirect_uri=https%3A%2F%2Fapp%2Fapi%2Fupwork%2Foauth%2Fcallback"));
    }
}
