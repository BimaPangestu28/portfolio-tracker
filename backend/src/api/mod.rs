pub mod auth;
pub mod cashflow;
pub mod chat;
pub mod connectors;
pub mod crud;
pub mod goals;
pub mod ingest;
pub mod portfolio;
pub mod whatsapp;

use crate::AppState;
use axum::{routing::{get, post, delete}, Router};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/chat", post(chat::post_chat))
        .route("/chat/history", get(chat::history))
        .route("/chat/whatsapp/inbound", post(whatsapp::inbound))
        .route("/whatsapp/state", post(whatsapp::push_state))
        .route("/whatsapp/commands", get(whatsapp::poll_commands))
        .route("/whatsapp/status", get(whatsapp::status))
        .route("/whatsapp/connect", post(whatsapp::connect))
        .route("/whatsapp/disconnect", post(whatsapp::disconnect))
        .route("/accounts", get(crud::list_accounts).post(crud::create_account))
        .route("/accounts/:id", delete(crud::delete_account))
        .route("/categories", get(crud::list_categories).post(crud::create_category))
        .route("/categories/:id", delete(crud::delete_category))
        .route("/instruments", get(crud::list_instruments).post(crud::create_instrument))
        .route("/instruments/:id", delete(crud::delete_instrument))
        .route("/transactions", get(crud::list_transactions).post(crud::create_transaction))
        .route("/transactions/:id", delete(crud::delete_transaction))
        .route("/prices/manual", post(crud::manual_price))
        .route("/fx/manual", post(crud::manual_fx))
        .route("/prices/refresh", post(portfolio::refresh))
        .route("/portfolio/summary", get(portfolio::summary))
        .route("/portfolio/history", get(portfolio::history))
        .route("/portfolio/insights", get(portfolio::insights))
        .route("/goals", get(goals::list_goals).post(goals::create_goal))
        .route("/goals/:id", delete(goals::delete_goal))
        .route("/cashflow", get(cashflow::list_cashflow).post(cashflow::create_cashflow))
        .route("/cashflow/categories", get(cashflow::list_categories).post(cashflow::create_category))
        .route("/cashflow/categories/:id", delete(cashflow::delete_category))
        .route("/cashflow/summary", get(cashflow::summary))
        .route("/cashflow/:id", delete(cashflow::delete_cashflow))
        .route("/connectors", get(connectors::list).post(connectors::create))
        .route("/connectors/:id/sync", post(connectors::sync))
        .route("/connectors/:id", delete(connectors::delete))
        .route("/ingest", post(ingest::ingest))
        .route("/ingest/csv", post(ingest::ingest_csv))
        .route("/ingest/review", get(ingest::list_review))
        .route("/ingest/review/:id", axum::routing::patch(ingest::patch_review))
        .route("/ingest/review/:id/confirm", post(ingest::confirm_review))
        .route("/ingest/review/:id/reject", post(ingest::reject_review))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
