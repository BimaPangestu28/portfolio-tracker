use crate::error::AppError;
use crate::repo::clients::{self, ClientRow};
use crate::repo::invoices::{self, InvoiceRow};
use crate::AppState;
use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
    Json,
};

/// All invoices.
pub async fn list(State(s): State<AppState>) -> Result<Json<Vec<InvoiceRow>>, AppError> {
    let rows = invoices::list_all(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}

/// One invoice by id.
pub async fn get(
    State(s): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<InvoiceRow>, AppError> {
    let row = invoices::get(&s.db, id)
        .await
        .map_err(AppError::Other)?
        .ok_or(AppError::NotFound)?;
    Ok(Json(row))
}

/// Re-render the invoice PDF from its stored row.
pub async fn pdf(State(s): State<AppState>, Path(id): Path<i64>) -> Result<Response, AppError> {
    let row = invoices::get(&s.db, id)
        .await
        .map_err(AppError::Other)?
        .ok_or(AppError::NotFound)?;
    let client = clients::get(&s.db, row.client_id)
        .await
        .map_err(AppError::Other)?;
    let config =
        crate::invoice::config::from_env().map_err(|e| AppError::Other(anyhow::anyhow!(e)))?;
    let data = crate::invoice::rebuild::data_from_row(&row, &client, config).map_err(AppError::Other)?;
    let bytes = crate::invoice::render::render_pdf(&data).map_err(AppError::Other)?;
    let filename = format!("{}.pdf", row.number.replace('/', "-"));
    Ok((
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (
                header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"{filename}\""),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// All clients.
pub async fn list_clients(State(s): State<AppState>) -> Result<Json<Vec<ClientRow>>, AppError> {
    let rows = clients::list(&s.db).await.map_err(AppError::Other)?;
    Ok(Json(rows))
}
