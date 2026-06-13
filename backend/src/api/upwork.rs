use crate::error::AppError;
use crate::AppState;
use axum::{extract::{Query, State}, response::Redirect, Json};
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
pub struct StartOut { pub consent_url: String }

/// Build the Upwork consent URL (frontend redirects the browser to it).
pub async fn start() -> Result<Json<StartOut>, AppError> {
    let cfg = crate::upwork::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("upwork not configured: {e}")))?;
    let secret = crate::auth::jwt_secret()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("JWT_SECRET not set")))?;
    let now = chrono::Utc::now().timestamp();
    let state = crate::google::oauth::sign_state_with(&secret, now).map_err(AppError::Other)?;
    let consent_url = crate::upwork::oauth::consent_url(&cfg.client_id, &cfg.redirect_uri, &state);
    Ok(Json(StartOut { consent_url }))
}

#[derive(Deserialize)]
pub struct CallbackQuery { pub code: Option<String>, pub state: Option<String> }

/// Public OAuth callback. Guarded by the signed `state` (CSRF). Redirects to settings.
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
    let cfg = crate::upwork::oauth::OAuthConfig::from_env()
        .map_err(|e| AppError::Other(anyhow::anyhow!("upwork not configured: {e}")))?;
    let key = crate::upwork::crypto::key_from_env().map_err(AppError::Other)?;
    let tokens = crate::upwork::oauth::exchange_code(&cfg, &code).await.map_err(AppError::Other)?;
    let refresh = tokens.refresh_token.clone()
        .ok_or_else(|| AppError::Other(anyhow::anyhow!("no refresh_token returned; re-consent required")))?;
    let enc_access = crate::upwork::crypto::encrypt(&tokens.access_token, &key).map_err(AppError::Other)?;
    let enc_refresh = crate::upwork::crypto::encrypt(&refresh, &key).map_err(AppError::Other)?;
    let expiry = crate::upwork::oauth::expiry_from_now(tokens.expires_in);
    let scope = tokens.scope.unwrap_or_default();
    crate::repo::upwork_integration::upsert(&s.db, &enc_access, &enc_refresh, &expiry, &scope)
        .await.map_err(AppError::Other)?;
    Ok(Redirect::to("/settings?upwork=connected"))
}

#[derive(Serialize)]
pub struct StatusOut { pub status: String, pub last_error: Option<String> }

pub async fn status(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    let row = crate::repo::upwork_integration::get(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(match row {
        Some(r) => StatusOut { status: r.status, last_error: r.last_error },
        None => StatusOut { status: "disconnected".into(), last_error: None },
    }))
}

#[derive(Serialize)]
pub struct SyncOut { pub inserted: usize }

/// Trigger an earnings sync now (manual; no background loop in v1).
pub async fn sync(State(s): State<AppState>) -> Result<Json<SyncOut>, AppError> {
    let inserted = crate::upwork::engine::run_cycle(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(SyncOut { inserted }))
}

pub async fn disconnect(State(s): State<AppState>) -> Result<Json<StatusOut>, AppError> {
    crate::repo::upwork_integration::delete(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(StatusOut { status: "disconnected".into(), last_error: None }))
}
