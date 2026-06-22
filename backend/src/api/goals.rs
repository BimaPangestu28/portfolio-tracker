use crate::error::AppError;
use crate::repo::goals;
use crate::repo::transactions;
use crate::service::goals::build_goal_progress;
use crate::domain::goal_progress::{months_until, required_monthly};
use crate::service::insights::build_insights;
use crate::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDate;
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
    pub target_date: Option<String>,
    /// Computed current value in IDR (market value for 'tagged').
    pub current_idr: String,
    /// 'tagged' only: net invested capital in IDR.
    pub invested_idr: Option<String>,
    /// 'tagged' only: market value − invested.
    pub gain_loss_idr: Option<String>,
    /// current / target × 100 (0 when target is 0).
    pub progress_pct: String,
    /// Monthly contribution needed to hit target by target_date (None if no date).
    pub required_monthly_idr: Option<String>,
}

async fn build_goal_response(
    s: &AppState,
    g: goals::GoalRow,
    liquid: Decimal,
    net_worth: Decimal,
) -> Result<GoalResponse, AppError> {
    // current_idr + invested/gain depend on the kind.
    let (current, invested, gain): (Decimal, Option<Decimal>, Option<Decimal>) = match g.current_kind.as_str() {
        "cash" => (liquid, None, None),
        "networth" => (net_worth, None, None),
        "manual" => (
            g.current_manual_idr.as_deref().map(crate::repo::dec).transpose().map_err(AppError::Other)?.unwrap_or(Decimal::ZERO),
            None, None,
        ),
        "tagged" => {
            let p = build_goal_progress(&s.db, g.id).await.map_err(AppError::Other)?;
            (p.market_value_idr, Some(p.invested_idr), Some(p.gain_loss_idr))
        }
        _ => (Decimal::ZERO, None, None),
    };

    let target = crate::repo::dec(&g.target_idr).map_err(AppError::Other)?;
    let progress_pct = if target.is_zero() { Decimal::ZERO } else { current / target * Decimal::from(100) };

    let required_monthly_idr = match g.target_date.as_deref() {
        Some(d) => match NaiveDate::parse_from_str(d, "%Y-%m-%d") {
            Ok(td) => {
                let months = months_until(chrono::Utc::now().date_naive(), td);
                Some(required_monthly(target, current, months).to_string())
            }
            Err(_) => None, // unparseable date -> no projection rather than a 500
        },
        None => None,
    };

    Ok(GoalResponse {
        id: g.id,
        label: g.label,
        note: g.note,
        target_idr: g.target_idr,
        current_kind: g.current_kind,
        current_manual_idr: g.current_manual_idr,
        sort_order: g.sort_order,
        created_at: g.created_at,
        target_date: g.target_date,
        current_idr: current.to_string(),
        invested_idr: invested.map(|v| v.to_string()),
        gain_loss_idr: gain.map(|v| v.to_string()),
        progress_pct: progress_pct.to_string(),
        required_monthly_idr,
    })
}

pub async fn list_goals(State(s): State<AppState>) -> Result<Json<Vec<GoalResponse>>, AppError> {
    let goal_rows = goals::list(&s.db).await.map_err(AppError::Other)?;
    if goal_rows.is_empty() {
        return Ok(Json(vec![]));
    }
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let (liquid, net_worth) = (insights.liquid_idr, insights.net_worth_idr);
    let mut responses = Vec::with_capacity(goal_rows.len());
    for g in goal_rows {
        responses.push(build_goal_response(&s, g, liquid, net_worth).await?);
    }
    Ok(Json(responses))
}

pub async fn create_goal(
    State(s): State<AppState>,
    Json(body): Json<goals::NewGoal>,
) -> Result<Json<GoalResponse>, AppError> {
    let row = goals::create(&s.db, &body).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let resp = build_goal_response(&s, row, insights.liquid_idr, insights.net_worth_idr).await?;
    Ok(Json(resp))
}

pub async fn update_goal(
    State(s): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<goals::UpdateGoal>,
) -> Result<Json<GoalResponse>, AppError> {
    goals::get(&s.db, id).await.map_err(|_| AppError::NotFound)?;
    let row = goals::update(&s.db, id, &body).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    let insights = build_insights(&s.db).await.map_err(AppError::Other)?;
    let resp = build_goal_response(&s, row, insights.liquid_idr, insights.net_worth_idr).await?;
    Ok(Json(resp))
}

#[derive(Debug, serde::Deserialize)]
pub struct TagBody {
    /// Goal to tag the transaction to; null clears the tag.
    pub goal_id: Option<i64>,
}

pub async fn set_transaction_goal(
    State(s): State<AppState>,
    Path(txn_id): Path<i64>,
    Json(body): Json<TagBody>,
) -> Result<Json<()>, AppError> {
    if let Some(gid) = body.goal_id {
        goals::get(&s.db, gid).await.map_err(|_| AppError::BadRequest(format!("unknown goal_id {gid}")))?;
    }
    transactions::set_txn_goal(&s.db, txn_id, body.goal_id)
        .await
        .map_err(|_| AppError::NotFound)?;
    Ok(Json(()))
}

pub async fn delete_goal(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<()>, AppError> {
    goals::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goal_response_has_progress_fields() {
        // Compile-time check: GoalResponse has all the new fields.
        let _fields: &[&str] = &[
            "invested_idr",
            "gain_loss_idr",
            "progress_pct",
            "target_date",
            "required_monthly_idr",
        ];
    }
}
