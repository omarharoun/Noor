use crate::state::AppState;
use axum::{
    extract::{Query, State},
    Json,
};
use paybank_core::AppError;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct LinkTokenQuery {
    pub session_id: Uuid,
}

pub async fn create_link_token(
    State(_state): State<AppState>,
    Query(query): Query<LinkTokenQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let link_token = paybank_payments::create_link_token(&query.session_id).await?;
    Ok(Json(serde_json::json!({ "link_token": link_token })))
}

#[derive(Deserialize)]
pub struct ExchangeRequest {
    pub session_id: Uuid,
    pub public_token: String,
}

pub async fn exchange_public_token(
    State(state): State<AppState>,
    Json(req): Json<ExchangeRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let access_token = paybank_payments::exchange_public_token(&req.public_token).await?;
    let accounts = paybank_payments::get_auth(&access_token).await?;

    if let Some(first) = accounts.first() {
        let _ = paybank_db::session_repo::update_customer_details(
            &state.db.pool,
            req.session_id,
            "",
            "",
            "",
            "",
            "",
            "",
            "",
            "US",
            &first.account_number,
            &first.routing_number,
            &first.account_type,
        )
        .await;
    }

    Ok(Json(serde_json::json!({
        "accounts": accounts
    })))
}