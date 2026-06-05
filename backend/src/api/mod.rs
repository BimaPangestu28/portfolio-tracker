pub mod auth;
pub mod cashflow;
pub mod chat;
pub mod connectors;
pub mod crud;
pub mod goals;
pub mod ingest;
pub mod portfolio;
pub mod telegram;
pub mod whatsapp;

use crate::AppState;
use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState) -> Router {
    // Open to anyone — no token required.
    let public = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/login", post(auth::login));

    // Authenticated by the shared x-gateway-token (checked inside the handlers).
    let gateway = Router::new()
        .route("/chat/whatsapp/inbound", post(whatsapp::inbound))
        .route("/whatsapp/state", post(whatsapp::push_state))
        .route("/whatsapp/commands", get(whatsapp::poll_commands));

    // Require a valid JWT (when auth is configured).
    let protected = Router::new()
        .route("/auth/me", get(auth::me))
        .route("/chat", post(chat::post_chat))
        .route("/chat/history", get(chat::history))
        .route("/whatsapp/status", get(whatsapp::status))
        .route("/whatsapp/connect", post(whatsapp::connect))
        .route("/whatsapp/disconnect", post(whatsapp::disconnect))
        .route("/telegram/status", get(telegram::status))
        .route("/telegram/link-code", post(telegram::link_code))
        .route("/telegram/unlink", post(telegram::unlink))
        .route(
            "/accounts",
            get(crud::list_accounts).post(crud::create_account),
        )
        .route("/accounts/:id", delete(crud::delete_account))
        .route(
            "/categories",
            get(crud::list_categories).post(crud::create_category),
        )
        .route(
            "/categories/:id",
            delete(crud::delete_category).patch(crud::update_category),
        )
        .route(
            "/instruments",
            get(crud::list_instruments).post(crud::create_instrument),
        )
        .route(
            "/instruments/:id",
            delete(crud::delete_instrument).patch(crud::update_instrument),
        )
        .route(
            "/transactions",
            get(crud::list_transactions).post(crud::create_transaction),
        )
        .route("/transactions/:id", delete(crud::delete_transaction))
        .route("/prices/manual", post(crud::manual_price))
        .route("/fx/manual", post(crud::manual_fx))
        .route("/prices/refresh", post(portfolio::refresh))
        .route("/portfolio/summary", get(portfolio::summary))
        .route("/portfolio/history", get(portfolio::history))
        .route("/portfolio/insights", get(portfolio::insights))
        .route("/portfolio/performance", get(portfolio::performance))
        .route("/goals", get(goals::list_goals).post(goals::create_goal))
        .route("/goals/:id", delete(goals::delete_goal))
        .route(
            "/cashflow",
            get(cashflow::list_cashflow).post(cashflow::create_cashflow),
        )
        .route(
            "/cashflow/categories",
            get(cashflow::list_categories).post(cashflow::create_category),
        )
        .route(
            "/cashflow/categories/:id",
            delete(cashflow::delete_category),
        )
        .route("/cashflow/summary", get(cashflow::summary))
        .route("/cashflow/:id", delete(cashflow::delete_cashflow))
        .route(
            "/connectors",
            get(connectors::list).post(connectors::create),
        )
        .route("/connectors/:id/sync", post(connectors::sync))
        .route("/connectors/:id", delete(connectors::delete))
        .route("/ingest", post(ingest::ingest))
        .route("/ingest/csv", post(ingest::ingest_csv))
        .route("/ingest/review", get(ingest::list_review))
        .route(
            "/ingest/review/:id",
            axum::routing::patch(ingest::patch_review),
        )
        .route("/ingest/review/:id/confirm", post(ingest::confirm_review))
        .route("/ingest/review/:id/reject", post(ingest::reject_review))
        .route_layer(middleware::from_fn(auth::require_auth));

    public
        .merge(gateway)
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

#[cfg(test)]
mod router_tests {
    use super::*;
    use crate::AppState;
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use serial_test::serial;
    use tower::ServiceExt;

    async fn test_state() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        AppState {
            db,
            wa: Default::default(),
            tg: Default::default(),
        }
    }

    // These tests mutate process env, so they run serially within this module.
    #[serial]
    #[tokio::test]
    async fn protected_route_requires_token_when_configured() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-secret");

        let app = router(test_state().await);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/portfolio/summary")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn health_and_login_are_public_when_configured() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-secret2");

        let app = router(test_state().await);
        let res = app
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }
}
