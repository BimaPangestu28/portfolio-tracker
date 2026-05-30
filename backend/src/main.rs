mod db;
mod domain;
mod error;

use axum::{routing::get, Router};
use db::Db;

#[derive(Clone)]
pub struct AppState {
    pub db: Db,
}

async fn health() -> &'static str {
    "ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let url =
        std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://portfolio.db".into());
    let db = db::connect(&url).await?;
    let state = AppState { db };
    let app = Router::new()
        .route("/health", get(health))
        .with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:8080").await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
