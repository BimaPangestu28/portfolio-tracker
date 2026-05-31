use crate::error::AppError;
use crate::repo::{accounts, categories, instruments, prices, transactions, dec};
use crate::AppState;
use axum::{extract::{Path, State}, Json};

pub async fn list_accounts(State(s): State<AppState>) -> Result<Json<Vec<accounts::AccountRow>>, AppError> {
    Ok(Json(accounts::list(&s.db).await.map_err(AppError::Other)?))
}
pub async fn create_account(State(s): State<AppState>, Json(b): Json<accounts::NewAccount>) -> Result<Json<accounts::AccountRow>, AppError> {
    Ok(Json(accounts::create(&s.db, &b).await.map_err(AppError::Other)?))
}
pub async fn delete_account(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    accounts::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

pub async fn list_categories(State(s): State<AppState>) -> Result<Json<Vec<categories::CategoryRow>>, AppError> {
    Ok(Json(categories::list(&s.db).await.map_err(AppError::Other)?))
}
pub async fn create_category(State(s): State<AppState>, Json(b): Json<categories::NewCategory>) -> Result<Json<categories::CategoryRow>, AppError> {
    Ok(Json(categories::create(&s.db, &b).await.map_err(AppError::Other)?))
}
pub async fn delete_category(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    categories::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

pub async fn list_instruments(State(s): State<AppState>) -> Result<Json<Vec<instruments::InstrumentRow>>, AppError> {
    Ok(Json(instruments::list(&s.db).await.map_err(AppError::Other)?))
}
pub async fn create_instrument(State(s): State<AppState>, Json(b): Json<instruments::NewInstrument>) -> Result<Json<instruments::InstrumentRow>, AppError> {
    Ok(Json(instruments::create(&s.db, &b).await.map_err(AppError::Other)?))
}
pub async fn delete_instrument(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    instruments::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

pub async fn list_transactions(State(s): State<AppState>) -> Result<Json<Vec<crate::domain::models::Transaction>>, AppError> {
    let txns = transactions::list_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(txns))
}
pub async fn create_transaction(State(s): State<AppState>, Json(b): Json<transactions::NewTransaction>) -> Result<Json<crate::domain::models::Transaction>, AppError> {
    let t = transactions::create(&s.db, &b).await.map_err(|e| AppError::BadRequest(e.to_string()))?;
    Ok(Json(t))
}
pub async fn delete_transaction(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Json<()>, AppError> {
    transactions::delete(&s.db, id).await.map_err(AppError::Other)?; Ok(Json(()))
}

#[derive(serde::Deserialize)]
pub struct ManualPrice { pub instrument_id: i64, pub price: String, pub currency: String, pub as_of: String }
pub async fn manual_price(State(s): State<AppState>, Json(b): Json<ManualPrice>) -> Result<Json<()>, AppError> {
    let price = dec(&b.price).map_err(|e| AppError::BadRequest(e.to_string()))?;
    prices::upsert_latest(&s.db, b.instrument_id, price, &b.currency, "manual", &b.as_of).await.map_err(AppError::Other)?;
    Ok(Json(()))
}
