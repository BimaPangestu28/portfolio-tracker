use crate::error::AppError;
use crate::repo::goals;
use crate::service::insights::build_insights;
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct GoalResponse {
    pub id: i64,
    pub label: String,
    pub note: Option<String>,
    pub target_idr: String,
    pub current_kind: String,
    pub current_manual_idr: Option<String>,
    pub sort_order: i64,
    pub created_at: String,
    /// Computed current value in IDR (as string decimal)
    pub current_idr: String,
}

pub async fn list_goals(State(s): State<AppState>) -> Result<Json<Vec<GoalResponse>>, AppError> {
    let goal_rows = goals::list(&s.db).await.map_err(AppError::Other)?;
    if goal_rows.is_empty() {
        return Ok(Json(vec![]));
    }

    // Build insights to get liquid + net_worth for computed current values
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let liquid = insights.liquid_idr;
    let net_worth = insights.net_worth_idr;

    let responses: Vec<GoalResponse> = goal_rows
        .into_iter()
        .map(|g| {
            let current_idr = compute_current(&g, liquid, net_worth);
            GoalResponse {
                id: g.id,
                label: g.label,
                note: g.note,
                target_idr: g.target_idr,
                current_kind: g.current_kind,
                current_manual_idr: g.current_manual_idr,
                sort_order: g.sort_order,
                created_at: g.created_at,
                current_idr,
            }
        })
        .collect();

    Ok(Json(responses))
}

fn compute_current(g: &goals::GoalRow, liquid: Decimal, net_worth: Decimal) -> String {
    match g.current_kind.as_str() {
        "cash" => liquid.to_string(),
        "networth" => net_worth.to_string(),
        "manual" => g
            .current_manual_idr
            .clone()
            .unwrap_or_else(|| "0".to_string()),
        _ => "0".to_string(),
    }
}

pub async fn create_goal(
    State(s): State<AppState>,
    Json(body): Json<goals::NewGoal>,
) -> Result<Json<goals::GoalRow>, AppError> {
    let row = goals::create(&s.db, &body)
        .await
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(row))
}

pub async fn delete_goal(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    goals::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
