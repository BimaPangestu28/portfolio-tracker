#![recursion_limit = "256"]
mod api;
mod assistant;
mod auth;
mod clickup;
mod cs;
mod connectors;
mod google;
mod db;
mod domain;
mod error;
mod ingestion;
mod invoice;
mod llm;
mod pricing;
mod repo;
mod scheduler;
mod service;
mod setup;
mod telegram;
mod upwork;
mod wa_state;

use db::Db;
use std::sync::{Arc, Mutex};
use telegram::state::{SharedTgState, TgState};
use wa_state::{SharedWaState, WaState};

#[derive(Clone)]
pub struct AppState {
    pub db:          Db,
    pub wa:          SharedWaState,
    pub tg:          SharedTgState,
    pub cs_wa:       SharedWaState,
    pub cs_outbound: crate::cs::wa_outbound::SharedOutbound,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    if let Err(e) = auth::validate_env_config() {
        anyhow::bail!("{e}");
    }
    if let Err(e) = cs::gate::validate_config() {
        anyhow::bail!("{e}");
    }
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://portfolio.db".into());
    let db = db::connect(&url).await?;
    let state = AppState {
        db:          db.clone(),
        wa:          Arc::new(Mutex::new(WaState::default())),
        tg:          Arc::new(Mutex::new(TgState::default())),
        cs_wa:       Arc::new(Mutex::new(WaState::default())),
        cs_outbound: crate::cs::wa_outbound::new_queue(),
    };
    telegram::spawn(db.clone(), state.tg.clone());
    assistant::reminder_tick::spawn(db.clone());
    assistant::proactive::tick::spawn(db.clone());
    google::engine::spawn(db.clone());
    upwork::jobs::spawn(db.clone());
    upwork::contracts::spawn(db.clone());
    if let Ok(wallet) = std::env::var("HYPERLIQUID_WALLET") {
        if let Err(e) = setup::ensure_hyperliquid_account(&db, &wallet).await {
            tracing::warn!("hyperliquid setup failed: {e:#}");
        }
    }
    scheduler::spawn(db, std::time::Duration::from_secs(3600));
    let app = api::router(state);
    let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:8080".into());
    let listener = tokio::net::TcpListener::bind(&bind_addr).await?;
    tracing::info!("listening on {}", listener.local_addr()?);
    axum::serve(listener, app).await?;
    Ok(())
}
