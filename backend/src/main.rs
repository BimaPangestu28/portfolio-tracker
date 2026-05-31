mod api;
mod db;
mod domain;
mod error;
mod ingestion;
mod llm;
mod pricing;
mod repo;
mod scheduler;
mod service;

use db::Db;

#[derive(Clone)]
pub struct AppState { pub db: Db }

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://portfolio.db".into());
    let db = db::connect(&url).await?;
    let state = AppState { db: db.clone() };
    scheduler::spawn(db, std::time::Duration::from_secs(3600));
    let app = api::router(state);
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8081".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
