use crate::auth;
use crate::error::AppError;
use axum::{extract::Request, http::header, middleware::Next, response::Response, Json};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct LoginIn {
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginOut {
    pub token: String,
}

/// Exchange the master password for a JWT.
pub async fn login(Json(body): Json<LoginIn>) -> Result<Json<LoginOut>, AppError> {
    if !auth::password_ok(&body.password) {
        return Err(AppError::Unauthorized("Sandi salah".into()));
    }
    let now = chrono::Utc::now().timestamp();
    let token = auth::issue_token(now).map_err(AppError::Other)?;
    Ok(Json(LoginOut { token }))
}

#[derive(Serialize)]
pub struct MeOut {
    pub ok: bool,
}

/// Lightweight protected endpoint the frontend uses to validate a stored token.
pub async fn me() -> Json<MeOut> {
    Json(MeOut { ok: true })
}

/// Middleware: require a valid `Authorization: Bearer <jwt>` on protected routes.
/// No-op when auth is not configured (dev).
pub async fn require_auth(req: Request, next: Next) -> Result<Response, AppError> {
    let configured = auth::is_configured();
    let secret = auth::jwt_secret();
    let header_val = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok());
    if auth::authorize(configured, secret.as_deref(), header_val) {
        Ok(next.run(req).await)
    } else {
        Err(AppError::Unauthorized("unauthorized".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_rejects_wrong_password() {
        // With a configured password, a wrong candidate must be rejected.
        assert!(!crate::auth::password_ok_with(Some("right"), "wrong"));
    }

    #[test]
    fn login_accepts_right_password() {
        assert!(crate::auth::password_ok_with(Some("right"), "right"));
    }
}
