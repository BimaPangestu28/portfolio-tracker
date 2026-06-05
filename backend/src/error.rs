use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("not found")]
    NotFound,
    #[error("invalid input: {0}")]
    BadRequest(String),
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("unauthorized: {0}")]
    Unauthorized(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, msg) = match &self {
            AppError::NotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::Conflict(_) => (StatusCode::CONFLICT, self.to_string()),
            AppError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Other(e) => {
                tracing::error!("internal error: {e:#}");
                // Single-user app behind auth: return the full error chain so the
                // owner can act on it, instead of an opaque "internal error".
                (StatusCode::INTERNAL_SERVER_ERROR, format!("{e:#}"))
            }
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        match e {
            sqlx::Error::RowNotFound => AppError::NotFound,
            other => AppError::Other(other.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_json(err: AppError) -> (StatusCode, serde_json::Value) {
        let resp = err.into_response();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.expect("read body");
        (status, serde_json::from_slice(&bytes).expect("json body"))
    }

    #[tokio::test]
    async fn internal_error_response_carries_the_error_chain() {
        // Single-user app behind auth: surfacing the cause beats a blind
        // {"error":"internal error"} the owner can't act on.
        let root = anyhow::anyhow!("FOREIGN KEY constraint failed")
            .context("delete txn 5");
        let (status, body) = body_json(AppError::Other(root)).await;
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        let msg = body["error"].as_str().expect("error string");
        assert!(msg.contains("delete txn 5"), "missing context: {msg}");
        assert!(msg.contains("FOREIGN KEY constraint failed"), "missing cause: {msg}");
    }

    #[tokio::test]
    async fn bad_request_keeps_its_message() {
        let (status, body) = body_json(AppError::BadRequest("quantity is not a number".into())).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(body["error"], "invalid input: quantity is not a number");
    }
}
