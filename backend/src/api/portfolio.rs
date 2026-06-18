use crate::error::AppError;
use crate::repo::snapshots;
use crate::service::insights::{build_insights, Insights};
use crate::service::performance::{build_performance, PerformanceView};
use crate::service::portfolio::{build_summary, PortfolioSummary};
use crate::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use serde::Deserialize;

pub async fn summary(State(s): State<AppState>) -> Result<Json<PortfolioSummary>, AppError> {
    Ok(Json(build_summary(&s.db).await.map_err(AppError::Other)?))
}
pub async fn history(State(s): State<AppState>) -> Result<Json<Vec<snapshots::SnapshotRow>>, AppError> {
    Ok(Json(snapshots::history(&s.db).await.map_err(AppError::Other)?))
}
pub async fn refresh(State(s): State<AppState>) -> Result<Json<()>, AppError> {
    crate::pricing::service::refresh_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
pub async fn insights(State(s): State<AppState>) -> Result<Json<Insights>, AppError> {
    Ok(Json(build_insights(&s.db).await.map_err(AppError::Other)?))
}

/// Top day-movers among held positions (largest absolute IDR impact first).
pub async fn movers(State(s): State<AppState>) -> Result<Json<Vec<crate::service::movers::Mover>>, AppError> {
    Ok(Json(
        crate::service::movers::daily_movers(&s.db, 6)
            .await
            .map_err(AppError::Other)?,
    ))
}

#[derive(serde::Serialize)]
pub struct BenchmarkRow {
    pub label: String,
    /// Period return as a ratio (0.05 = +5%), matching performance metrics.
    pub return_ratio: f64,
}

/// 1-year returns: portfolio TWR vs index benchmarks. Best-effort — an index
/// that fails to fetch is omitted rather than failing the whole response.
pub async fn benchmark(State(s): State<AppState>) -> Result<Json<Vec<BenchmarkRow>>, AppError> {
    let mut rows = Vec::new();
    let perf = build_performance(&s.db, "idr", "1y")
        .await
        .map_err(AppError::Other)?;
    if !perf.insufficient_data {
        rows.push(BenchmarkRow {
            label: "Portofolio".into(),
            return_ratio: perf.metrics.total_return,
        });
    }
    let yahoo = crate::pricing::yahoo::Yahoo::new();
    for (label, symbol) in [("IHSG", "%5EJKSE"), ("S&P 500", "%5EGSPC")] {
        match yahoo.range_return(symbol, "1y").await {
            Ok(ratio) => rows.push(BenchmarkRow { label: label.into(), return_ratio: ratio }),
            Err(e) => tracing::warn!("benchmark fetch for {label} failed: {e}"),
        }
    }
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct PerfQuery {
    pub base: Option<String>,
    pub period: Option<String>,
}

pub async fn performance(
    State(s): State<AppState>,
    Query(q): Query<PerfQuery>,
) -> Result<Json<PerformanceView>, AppError> {
    let base = q.base.as_deref().unwrap_or("idr");
    if base != "idr" && base != "usd" {
        return Err(AppError::BadRequest("base must be idr or usd".into()));
    }
    let period = q.period.as_deref().unwrap_or("1y");
    if !["1m", "3m", "6m", "ytd", "1y", "all"].contains(&period) {
        return Err(AppError::BadRequest("invalid period".into()));
    }
    Ok(Json(
        build_performance(&s.db, base, period)
            .await
            .map_err(AppError::Other)?,
    ))
}

pub async fn hyperliquid(
    State(s): State<AppState>,
) -> Result<Json<crate::service::hyperliquid::HyperliquidView>, AppError> {
    Ok(Json(
        crate::service::hyperliquid::build_hyperliquid_view(&s.db)
            .await
            .map_err(AppError::Other)?,
    ))
}
