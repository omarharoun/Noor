use crate::state::AppState;
use axum::{extract::State, Json};
use paybank_db::bank_repo;

pub async fn list_banks(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, paybank_core::AppError> {
    let banks = bank_repo::list_banks(&state.db.pool)
        .await
        .map_err(|e| paybank_core::AppError::Internal(e))?;

    let response = banks
        .iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id, "name": b.name,
                "logo_url": b.logo_url,
                "supports_fednow": b.supports_fednow,
                "supports_rtp": b.supports_rtp,
                "instant": b.supports_fednow || b.supports_rtp,
            })
        })
        .collect::<Vec<_>>();

    Ok(Json(serde_json::json!({ "banks": response })))
}