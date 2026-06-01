use crate::error::AppError;
use crate::repo::connectors::{self, NewConnector};
use crate::AppState;
use axum::{extract::{Path, State}, Json};

#[derive(serde::Serialize)]
pub struct ConnectorOut {
    pub id: i64,
    pub account_id: i64,
    pub kind: String,
    pub label: String,
    pub config_public: serde_json::Value,  // config_json with secret keys removed
    pub cursor: Option<String>,
    pub last_synced_at: Option<String>,
    pub enabled: i64,
    pub created_at: String,
}

fn redact(config_json: &str) -> serde_json::Value {
    let mut v: serde_json::Value = serde_json::from_str(config_json).unwrap_or(serde_json::json!({}));
    if let Some(obj) = v.as_object_mut() {
        for k in ["api_key", "secret", "api_secret", "apiSecret", "passphrase", "private_key", "privateKey"] {
            obj.remove(k);
        }
    }
    v
}

fn to_out(r: crate::repo::connectors::ConnectorRow) -> ConnectorOut {
    ConnectorOut {
        id: r.id, account_id: r.account_id, kind: r.kind, label: r.label,
        config_public: redact(&r.config_json),
        cursor: r.cursor, last_synced_at: r.last_synced_at, enabled: r.enabled, created_at: r.created_at,
    }
}

pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<ConnectorOut>>, AppError> {
    let rows = connectors::list(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows.into_iter().map(to_out).collect()))
}

pub async fn create(
    State(s): State<AppState>,
    Json(b): Json<NewConnector>,
) -> Result<Json<ConnectorOut>, AppError> {
    let row = connectors::create(&s.db, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(to_out(row)))
}

pub async fn delete(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    connectors::delete(&s.db, id).await.map_err(AppError::Other)?;
    Ok(Json(()))
}

pub async fn sync(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<crate::service::sync::SyncReport>, AppError> {
    let row = crate::repo::connectors::get(&s.db, id).await?;
    let conn = crate::connectors::factory::build(&row)
        .map_err(|e| AppError::BadRequest(e.to_string()))?;
    let report = crate::service::sync::run_sync(&s.db, &row, conn.as_ref())
        .await
        .map_err(AppError::Other)?;
    Ok(Json(report))
}
