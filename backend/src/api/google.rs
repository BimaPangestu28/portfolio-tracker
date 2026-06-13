use crate::error::AppError;
use crate::AppState;
use axum::{extract::{Query, State}, response::Redirect, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct StartOut { pub consent_url: String }

/// Build the Google consent URL (frontend redirects the browser to it).
pub async fn start() -> Result<Json<StartOut>, AppError> {
    let cfg = crate::google::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("google not configured: {e}")))?;
    let secret = crate::auth::jwt_secret()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("JWT_SECRET not set")))?;
    let now = chrono::Utc::now().timestamp();
    let state = crate::google::oauth::sign_state_with(&secret, now).map_err(AppError::Other)?;
    let consent_url = crate::google::oauth::consent_url(&cfg.client_id, &cfg.redirect_uri, &state);
    Ok(Json(StartOut { consent_url }))
}

#[derive(Deserialize)]
pub struct CallbackQuery { pub code: Option<String>, pub state: Option<String> }

/// Public OAuth callback. Guarded by the signed `state` (not JWT) since Google
/// redirects the browser here without the SPA's token. Redirects to /settings.
pub async fn callback(State(s): State<AppState>, Query(q): Query<CallbackQuery>) -> Result<Redirect, AppError> {
    let (code, state) = match (q.code, q.state) {
        (Some(c), Some(st)) => (c, st),
        _ => return Err(AppError::BadRequest("missing code/state".into())),
    };
    let secret = crate::auth::jwt_secret()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("JWT_SECRET not set")))?;
    let now = chrono::Utc::now().timestamp();
    if !crate::google::oauth::verify_state_with(&secret, &state, now) {
        return Err(AppError::Unauthorized("invalid state".into()));
    }
    let cfg = crate::google::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("google not configured: {e}")))?;
    let key = crate::google::crypto::key_from_env().map_err(AppError::Other)?;
    let tokens = crate::google::oauth::exchange_code(&cfg, &code).await.map_err(AppError::Other)?;
    let refresh = tokens.refresh_token.clone()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("no refresh_token returned; re-consent required")))?;
    let enc_access = crate::google::crypto::encrypt(&tokens.access_token, &key).map_err(AppError::Other)?;
    let enc_refresh = crate::google::crypto::encrypt(&refresh, &key).map_err(AppError::Other)?;
    let expiry = crate::google::oauth::expiry_from_now(tokens.expires_in);
    let scope = tokens.scope.unwrap_or_else(|| crate::google::oauth::SCOPE.to_string());
    crate::repo::google_integration::upsert(&s.db, &enc_access, &enc_refresh, &expiry, &scope)
        .await.map_err(AppError::Other)?;
    Ok(Redirect::to("/settings?google=connected"))
}

#[derive(Serialize)]
pub struct StatusOut {
    pub status: String,
    pub last_error: Option<String>,
    pub last_synced_at: Option<String>,
}

/// Connection status for the Settings UI.
pub async fn status(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    let row = crate::repo::google_integration::get(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(match row {
        Some(r) => StatusOut { status: r.status, last_error: r.last_error, last_synced_at: r.last_synced_at },
        None => StatusOut { status: "disconnected".into(), last_error: None, last_synced_at: None },
    }))
}

#[derive(Serialize)]
pub struct SyncOut {
    pub status: String,
    pub last_error: Option<String>,
    pub last_synced_at: Option<String>,
    pub pushed: usize,
    pub imported: usize,
}

/// Run one sync cycle on demand (the "Sync now" button) and report the result.
/// Surfaces the resulting status/last_error so a silently-failing connection
/// becomes visible immediately.
pub async fn sync(State(s): State<AppState>) -> Result<Json<SyncOut>, AppError> {
    let summary = crate::google::engine::run_cycle(&s.db).await.map_err(AppError::Other)?;
    let row = crate::repo::google_integration::get(&s.db).await.map_err(AppError::Other)?;
    let (status, last_error, last_synced_at) = match row {
        Some(r) => (r.status, r.last_error, r.last_synced_at),
        None => ("disconnected".into(), None, None),
    };
    Ok(Json(SyncOut { status, last_error, last_synced_at, pushed: summary.pushed, imported: summary.imported }))
}

/// Revoke + delete the connection.
pub async fn disconnect(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    if let Some(row) = crate::repo::google_integration::get(&s.db).await.map_err(AppError::Other)? {
        if let Ok(key) = crate::google::crypto::key_from_env() {
            if let Ok(access) = crate::google::crypto::decrypt(&row.access_token, &key) {
                let _ = crate::google::oauth::revoke(&access).await;
            }
        }
    }
    crate::repo::google_integration::delete(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(StatusOut { status: "disconnected".into(), last_error: None, last_synced_at: None }))
}
