use crate::domain::allocation::CategoryInput;
use crate::domain::dca::{compute_dca_plan, DcaPlan};
use crate::error::AppError;
use crate::repo::dca_settings::{self, DcaSettingRow, SaveDcaSetting};
use crate::repo::dec;
use crate::service::portfolio::build_summary;
use crate::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use rust_decimal::Decimal;
use serde::Deserialize;

pub async fn get_settings(
    State(s): State<AppState>,
) -> Result<Json<DcaSettingRow>, AppError> {
    Ok(Json(dca_settings::get(&s.db).await.map_err(AppError::Other)?))
}

pub async fn update_settings(
    State(s): State<AppState>,
    Json(body): Json<SaveDcaSetting>,
) -> Result<Json<DcaSettingRow>, AppError> {
    // Validate before persisting.
    dec(&body.monthly_budget).map_err(|e| AppError::BadRequest(e.to_string()))?;
    dec(&body.rounding_step).map_err(|e| AppError::BadRequest(e.to_string()))?;
    if body.frequency != "monthly" && body.frequency != "weekly" {
        return Err(AppError::BadRequest("frequency must be 'monthly' or 'weekly'".into()));
    }
    if !(1..=28).contains(&body.anchor_day) {
        return Err(AppError::BadRequest("anchor_day must be between 1 and 28".into()));
    }
    Ok(Json(dca_settings::upsert(&s.db, &body).await.map_err(AppError::Other)?))
}

#[derive(Deserialize)]
pub struct PlanQuery {
    /// Optional what-if budget override (decimal string). Defaults to saved settings.
    pub budget: Option<String>,
    /// Optional what-if frequency override ('monthly' | 'weekly').
    pub frequency: Option<String>,
}

pub async fn plan(
    State(s): State<AppState>,
    Query(q): Query<PlanQuery>,
) -> Result<Json<DcaPlan>, AppError> {
    let settings = dca_settings::get(&s.db).await.map_err(AppError::Other)?;

    let monthly = match q.budget.as_deref() {
        Some(b) => dec(b).map_err(|e| AppError::BadRequest(e.to_string()))?,
        None => dec(&settings.monthly_budget).map_err(AppError::Other)?,
    };
    let frequency = q.frequency.as_deref().unwrap_or(&settings.frequency);
    if frequency != "monthly" && frequency != "weekly" {
        return Err(AppError::BadRequest("frequency must be 'monthly' or 'weekly'".into()));
    }
    // Weekly slices the monthly budget into 4 (v1 simplification).
    let period_budget = if frequency == "weekly" {
        monthly / Decimal::from(4)
    } else {
        monthly
    };
    let rounding_step = dec(&settings.rounding_step).map_err(AppError::Other)?;

    // Reuse the portfolio summary's category aggregation (includes the "Lainnya" bucket).
    let summary = build_summary(&s.db).await.map_err(AppError::Other)?;
    let categories: Vec<CategoryInput> = summary
        .allocation
        .iter()
        .map(|a| CategoryInput {
            category_id: a.category_id,
            name: a.name.clone(),
            target_pct: a.target_pct,
            tolerance_band_pct: a.tolerance_band_pct,
            value_idr: a.actual_value_idr,
        })
        .collect();

    Ok(Json(compute_dca_plan(&categories, period_budget, rounding_step)))
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn mem_state() -> AppState {
        let db = crate::db::connect("sqlite::memory:").await.unwrap();
        AppState {
            db,
            wa:          Default::default(),
            tg:          Default::default(),
            cs_wa:       Default::default(),
            cs_outbound: crate::cs::wa_outbound::new_queue(),
        }
    }

    #[tokio::test]
    async fn rejects_bad_frequency() {
        let st = mem_state().await;
        let err = update_settings(
            axum::extract::State(st),
            axum::Json(SaveDcaSetting {
                monthly_budget: "55000000".into(),
                frequency: "daily".into(),
                anchor_day: 12,
                rounding_step: "10000".into(),
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn rejects_bad_anchor_day() {
        let st = mem_state().await;
        let err = update_settings(
            axum::extract::State(st),
            axum::Json(SaveDcaSetting {
                monthly_budget: "55000000".into(),
                frequency: "monthly".into(),
                anchor_day: 29,
                rounding_step: "10000".into(),
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn rejects_bad_monthly_budget() {
        let st = mem_state().await;
        let err = update_settings(
            axum::extract::State(st),
            axum::Json(SaveDcaSetting {
                monthly_budget: "not-a-number".into(),
                frequency: "monthly".into(),
                anchor_day: 1,
                rounding_step: "10000".into(),
            }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[tokio::test]
    async fn accepts_valid_settings_and_roundtrips() {
        let st = mem_state().await;
        let result = update_settings(
            axum::extract::State(st),
            axum::Json(SaveDcaSetting {
                monthly_budget: "55000000".into(),
                frequency: "monthly".into(),
                anchor_day: 12,
                rounding_step: "10000".into(),
            }),
        )
        .await
        .unwrap();
        assert_eq!(result.0.monthly_budget, "55000000");
        assert_eq!(result.0.frequency, "monthly");
        assert_eq!(result.0.anchor_day, 12);
    }

    #[tokio::test]
    async fn plan_rejects_bad_frequency_override() {
        let st = mem_state().await;
        let err = plan(
            axum::extract::State(st),
            axum::extract::Query(PlanQuery { budget: None, frequency: Some("daily".into()) }),
        )
        .await
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}
