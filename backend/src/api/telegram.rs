//! Frontend-facing Telegram linking endpoints (JWT-protected via the router).
//!
//! The bot itself does not call these — inbound traffic arrives through the
//! long-poller in `crate::telegram`, not through HTTP.

use crate::error::AppError;
use crate::telegram::state::CODE_TTL_SECS;
use crate::AppState;
use axum::{extract::State, Json};
use serde::Serialize;
use std::time::Instant;

#[derive(Serialize)]
pub struct TelegramStatusView {
    /// Token present and not rejected by Telegram.
    pub configured: bool,
    pub linked: bool,
    pub username: Option<String>,
}

#[derive(Serialize)]
pub struct LinkCodeOut {
    pub code: String,
    pub expires_in: u64,
}

fn lock_tg(
    s: &AppState,
) -> Result<std::sync::MutexGuard<'_, crate::telegram::state::TgState>, AppError> {
    s.tg
        .lock()
        .map_err(|_| AppError::Other(anyhow::anyhow!("tg state poisoned")))
}

fn token_configured() -> bool {
    std::env::var("TELEGRAM_BOT_TOKEN").is_ok_and(|t| !t.is_empty())
}

/// Linking status for the web UI. `configured` is false when the token is
/// missing OR Telegram rejected it (auth_failed) — either way the channel
/// is not usable and the UI should say so.
pub async fn status(State(s): State<AppState>) -> Result<Json<TelegramStatusView>, AppError> {
    let auth_failed = lock_tg(&s)?.auth_failed();
    let link = crate::repo::telegram_link::get(&s.db)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(TelegramStatusView {
        configured: token_configured() && !auth_failed,
        linked: link.is_some(),
        username: link.and_then(|l| l.username),
    }))
}

/// Generate a fresh one-time link code (invalidates any previous code).
pub async fn link_code(State(s): State<AppState>) -> Result<Json<LinkCodeOut>, AppError> {
    if !token_configured() {
        return Err(AppError::Conflict(
            "telegram bot is not configured (set TELEGRAM_BOT_TOKEN)".into(),
        ));
    }
    let code = lock_tg(&s)?.generate_code(Instant::now());
    Ok(Json(LinkCodeOut { code, expires_in: CODE_TTL_SECS }))
}

/// Remove the owner link; the bot stops answering until re-linked.
pub async fn unlink(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    crate::repo::telegram_link::clear(&s.db)
        .await
        .map_err(AppError::Other)?;
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;

    async fn test_state() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        AppState {
            db,
            wa: Default::default(),
            tg: Default::default(),
        }
    }

    // These tests mutate TELEGRAM_BOT_TOKEN, so they run serially. Cleanup is
    // not panic-safe (a failed assertion skips the trailing remove_var), so
    // every test that needs a clean env must defensively remove_var at its
    // START rather than rely on the previous test's cleanup.
    #[serial]
    #[tokio::test]
    async fn status_reports_unconfigured_without_token() {
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
        let s = test_state().await;
        let Json(view) = status(State(s)).await.unwrap();
        assert!(!view.configured);
        assert!(!view.linked);
        assert_eq!(view.username, None);
    }

    #[serial]
    #[tokio::test]
    async fn status_reports_linked_username() {
        std::env::set_var("TELEGRAM_BOT_TOKEN", "123:abc");
        let s = test_state().await;
        crate::repo::telegram_link::set(&s.db, 42, Some("bima")).await.unwrap();
        let Json(view) = status(State(s)).await.unwrap();
        assert!(view.configured);
        assert!(view.linked);
        assert_eq!(view.username.as_deref(), Some("bima"));
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
    }

    #[serial]
    #[tokio::test]
    async fn link_code_conflicts_when_unconfigured() {
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
        let s = test_state().await;
        let err = link_code(State(s)).await.err().expect("must fail");
        assert!(matches!(err, AppError::Conflict(_)));
    }

    #[serial]
    #[tokio::test]
    async fn link_code_returns_a_six_digit_code() {
        std::env::set_var("TELEGRAM_BOT_TOKEN", "123:abc");
        let s = test_state().await;
        let Json(out) = link_code(State(s.clone())).await.unwrap();
        assert_eq!(out.code.len(), 6);
        assert_eq!(out.expires_in, CODE_TTL_SECS);
        // The generated code is actually verifiable in the shared state.
        assert!(s.tg.lock().unwrap().verify_code(&out.code, Instant::now()));
        std::env::remove_var("TELEGRAM_BOT_TOKEN");
    }

    #[serial]
    #[tokio::test]
    async fn unlink_clears_the_link() {
        let s = test_state().await;
        crate::repo::telegram_link::set(&s.db, 42, None).await.unwrap();
        let _ = unlink(State(s.clone())).await.unwrap();
        assert!(crate::repo::telegram_link::get(&s.db).await.unwrap().is_none());
    }
}
