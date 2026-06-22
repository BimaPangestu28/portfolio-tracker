use crate::error::AppError;
use crate::repo::plan_nodes;
use crate::service::portfolio::build_plan_tree;
use crate::AppState;
use axum::{extract::{Path, State}, Json};

pub async fn get_tree(State(s): State<AppState>) -> Result<Json<Vec<crate::domain::plan_alloc::PlanNodeAllocation>>, AppError> {
    Ok(Json(build_plan_tree(&s.db).await.map_err(AppError::Other)?))
}

pub async fn list_nodes(State(s): State<AppState>) -> Result<Json<Vec<plan_nodes::PlanNodeRow>>, AppError> {
    Ok(Json(plan_nodes::list(&s.db).await.map_err(AppError::Other)?))
}

pub async fn create_node(State(s): State<AppState>, Json(b): Json<plan_nodes::NewPlanNode>) -> Result<Json<plan_nodes::PlanNodeRow>, AppError> {
    // Validate referenced parent up-front for a clear 400 instead of an FK error.
    if let Some(pid) = b.parent_id {
        plan_nodes::get(&s.db, pid).await.map_err(|_| AppError::BadRequest(format!("unknown parent_id {pid}")))?;
    }
    Ok(Json(plan_nodes::create(&s.db, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}

pub async fn update_node(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<plan_nodes::UpdatePlanNode>) -> Result<Json<plan_nodes::PlanNodeRow>, AppError> {
    plan_nodes::get(&s.db, id).await.map_err(|_| AppError::NotFound)?;
    Ok(Json(plan_nodes::update(&s.db, id, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}

pub async fn delete_node(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    plan_nodes::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

pub async fn move_node(State(s): State<AppState>, Path(id): Path<i64>, Json(b): Json<plan_nodes::MovePlanNode>) -> Result<Json<plan_nodes::PlanNodeRow>, AppError> {
    plan_nodes::get(&s.db, id).await.map_err(|_| AppError::NotFound)?;
    Ok(Json(plan_nodes::move_node(&s.db, id, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?))
}
