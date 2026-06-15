pub mod auth;
pub mod cashflow;
pub mod chat;
pub mod connectors;
pub mod crud;
pub mod events;
pub mod goals;
pub mod google;
pub mod inbox;
pub mod ingest;
pub mod invoices;
pub mod reminders;
pub mod todos;
pub mod upwork;
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
        .route("/auth/login", post(auth::login))
        .route("/google/oauth/callback", get(google::callback))
        .route("/upwork/oauth/callback", get(upwork::callback));

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
        .route("/google/oauth/start", get(google::start))
        .route("/google/status", get(google::status))
        .route("/google/sync", post(google::sync))
        .route("/google/disconnect", post(google::disconnect))
        .route("/upwork/oauth/start", get(upwork::start))
        .route("/upwork/status", get(upwork::status))
        .route("/upwork/sync", post(upwork::sync))
        .route("/upwork/disconnect", post(upwork::disconnect))
        .route("/events", get(events::list).post(events::create))
        .route("/events/:id", axum::routing::patch(events::update))
        .route("/events/:id/cancel", post(events::cancel))
        .route("/todos", get(todos::list).post(todos::create))
        .route("/todos/:id", axum::routing::patch(todos::update))
        .route("/todos/:id/complete", post(todos::complete))
        .route("/todos/:id/reopen", post(todos::reopen))
        .route("/reminders", get(reminders::list).post(reminders::create))
        .route("/reminders/:id/cancel", post(reminders::cancel))
        .route("/inbox", get(inbox::list))
        .route("/inbox/:id/resolve", post(inbox::resolve))
        .route("/inbox/:id/unresolve", post(inbox::unresolve))
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
        .route("/portfolio/movers", get(portfolio::movers))
        .route("/portfolio/benchmark", get(portfolio::benchmark))
        .route("/invoices", get(invoices::list))
        .route("/invoices/:id", get(invoices::get))
        .route("/invoices/:id/pdf", get(invoices::pdf))
        .route("/clients", get(invoices::list_clients))
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
    async fn google_start_is_protected_but_callback_is_public() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-google");

        let app = router(test_state().await);
        let res = app.clone().oneshot(
            Request::builder().uri("/google/oauth/start").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app.oneshot(
            Request::builder().uri("/google/oauth/callback").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn upwork_start_is_protected_but_callback_is_public() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-upwork");

        let app = router(test_state().await);
        let res = app.clone().oneshot(
            Request::builder().uri("/upwork/oauth/start").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = app.oneshot(
            Request::builder().uri("/upwork/oauth/callback").body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_ne!(res.status(), StatusCode::UNAUTHORIZED);

        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn events_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-events");
        let app = router(test_state().await);
        let res = app.oneshot(
            Request::builder().uri("/events?from=2026-06-01T00:00:00Z&to=2026-07-01T00:00:00Z")
                .body(Body::empty()).unwrap()
        ).await.unwrap();
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn assistant_read_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-assistant");
        let app = router(test_state().await);
        for uri in ["/todos", "/reminders", "/inbox"] {
            let res = app.clone().oneshot(
                Request::builder().uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn assistant_write_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-assistant-write");
        let app = router(test_state().await);
        let cases = [
            ("/todos", "POST"),
            ("/todos/1/complete", "POST"),
            ("/reminders/1/cancel", "POST"),
            ("/inbox/1/resolve", "POST"),
        ];
        for (uri, method) in cases {
            let res = app.clone().oneshot(
                Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn reminder_create_inbox_unresolve_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-rem-create");
        let app = router(test_state().await);
        let cases = [("/reminders", "POST"), ("/inbox/1/unresolve", "POST")];
        for (uri, method) in cases {
            let res = app.clone().oneshot(
                Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn todo_edit_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-todo-edit");
        let app = router(test_state().await);
        let cases = [("/todos/1", "PATCH"), ("/todos/1/reopen", "POST")];
        for (uri, method) in cases {
            let res = app.clone().oneshot(
                Request::builder().method(method).uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{method} {uri} should be protected");
        }
        std::env::remove_var("AUTH_PASSWORD");
        std::env::remove_var("JWT_SECRET");
    }

    #[serial]
    #[tokio::test]
    async fn invoice_routes_are_protected() {
        std::env::set_var("AUTH_PASSWORD", "pw");
        std::env::set_var("JWT_SECRET", "router-test-invoice");
        let app = router(test_state().await);
        let uris = ["/invoices", "/invoices/1", "/invoices/1/pdf", "/clients"];
        for uri in uris {
            let res = app.clone().oneshot(
                Request::builder().method("GET").uri(uri).body(Body::empty()).unwrap()
            ).await.unwrap();
            assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "GET {uri} should be protected");
        }
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
