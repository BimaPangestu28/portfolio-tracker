pub mod crud;
pub mod ingest;
pub mod portfolio;

use crate::AppState;
use axum::{routing::{get, post, delete}, Router};
use tower_http::cors::CorsLayer;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
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
        .route("/ingest", post(ingest::ingest))
        .route("/ingest/review", get(ingest::list_review))
        .route("/ingest/review/:id", axum::routing::patch(ingest::patch_review))
        .route("/ingest/review/:id/confirm", post(ingest::confirm_review))
        .route("/ingest/review/:id/reject", post(ingest::reject_review))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
