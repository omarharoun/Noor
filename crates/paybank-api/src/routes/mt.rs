use crate::state::AppState;
use axum::{extract::{Path, State}, Json};
use paybank_core::AppError as _AppError;

pub async fn get_payment_order(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, _AppError> {
    let order = paybank_payments::moderntreasury::get_payment_order_details(&id).await
        .map_err(|e| _AppError::ModernTreasuryError(e.to_string()))?;
    Ok(Json(order))
}

pub async fn get_counterparty(
    State(_state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, _AppError> {
    let cp = paybank_payments::moderntreasury::get_counterparty_details(&id).await
        .map_err(|e| _AppError::ModernTreasuryError(e.to_string()))?;
    Ok(Json(cp))
}