use crate::state::AppState;
use axum::{extract::State, Json};
use paybank_core::{AppError, SessionStatus};
use paybank_db::session_repo;
use paybank_payments::BankAccountDetails;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ConfirmRequest {
    pub session_id: Uuid,
    pub customer_name: String,
    pub customer_city: String,
    pub customer_state: String,
    pub customer_address_line_1: String,
    pub customer_postal_code: String,
    pub customer_country_code: String,
}

pub async fn confirm_payment(
    State(state): State<AppState>,
    Json(req): Json<ConfirmRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = session_repo::get_session(&state.db.pool, req.session_id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    if session.status == SessionStatus::Expired || chrono::Utc::now() > session.expires_at {
        return Err(AppError::SessionExpired);
    }
    if session.status == SessionStatus::Completed {
        return Ok(Json(serde_json::json!({ "status": "completed" })));
    }

    session_repo::update_customer_details(
        &state.db.pool,
        req.session_id,
        &req.customer_name,
        "customer@example.com",
        "5555555555",
        &req.customer_address_line_1,
        &req.customer_city,
        &req.customer_state,
        &req.customer_postal_code,
        &req.customer_country_code,
        &session.customer_account_number.clone().unwrap_or_default(),
        &session.customer_routing_number.clone().unwrap_or_default(),
        &session.customer_account_type.clone().unwrap_or_default(),
    )
    .await?;

    let counterparty_id = if let (Some(acct_num), Some(rout_num), Some(acct_type)) = (
        &session.customer_account_number,
        &session.customer_routing_number,
        &session.customer_account_type,
    ) {
        let details = BankAccountDetails {
            account_id: String::new(),
            account_name: req.customer_name.clone(),
            account_number: acct_num.clone(),
            routing_number: rout_num.clone(),
            account_type: acct_type.clone(),
            subtype: acct_type.clone(),
            available_balance: None,
            current_balance: None,
            iso_currency_code: None,
        };
        let result = paybank_payments::create_counterparty(&details, &req.customer_name).await?;
        let stored = format!("{}|{}", result.counterparty_id, result.external_account_id);
        session_repo::update_counterparty_id(&state.db.pool, req.session_id, &stored)
            .await?;
        Some(stored)
    } else {
        None
    };

    session_repo::update_status(
        &state.db.pool,
        req.session_id,
        &SessionStatus::Authorized,
        None,
    )
    .await?;

    Ok(Json(serde_json::json!({
        "status": "authorized",
        "session_id": req.session_id,
        "counterparty_id": counterparty_id,
    })))
}