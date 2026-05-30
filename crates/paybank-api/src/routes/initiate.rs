use crate::state::AppState;
use axum::{extract::State, Json};
use paybank_core::{AppError, SessionStatus};
use paybank_db::session_repo;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct InitiateRequest {
    pub session_id: Uuid,
}

pub async fn initiate_session(
    State(state): State<AppState>,
    Json(req): Json<InitiateRequest>,
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

    let rail = session
        .rail_used
        .as_ref()
        .ok_or(AppError::SessionNotFound)?;
    let raw = session.column_counterparty_id.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "No counterparty linked. Use /confirm instead."
        ))
    })?;
    let counterparty_id = raw.split('|').next().unwrap_or(&raw);
    let ext_account_id = raw.split('|').nth(1).unwrap_or(counterparty_id);

    let actual_recipient_id = if std::env::var("MODERN_TREASURY_API_KEY").is_ok() {
        ext_account_id
    } else {
        counterparty_id
    };

    session_repo::update_status(
        &state.db.pool,
        req.session_id,
        &SessionStatus::Processing,
        None,
    )
    .await?;

    let result = paybank_payments::initiate_transfer(
        req.session_id,
        &actual_recipient_id,
        rail,
        session.amount_cents,
        &format!("Session {}", req.session_id),
    )
    .await;

    match result {
        Ok(transfer) => {
            session_repo::update_status(
                &state.db.pool,
                req.session_id,
                &SessionStatus::Completed,
                Some(&transfer.provider_transfer_id),
            )
            .await?;

            let txn_id = Uuid::new_v4();
            paybank_db::transaction_repo::create_transaction(
                &state.db.pool,
                txn_id,
                req.session_id,
                session.merchant_id,
                session.amount_cents,
                rail,
                Some(&transfer.provider_transfer_id),
                &format!("txn_{}", req.session_id),
                chrono::Utc::now(),
            )
            .await?;

            Ok(Json(serde_json::json!({
                "status": "completed",
                "transaction_id": txn_id,
            })))
        }
        Err(e) => {
            session_repo::update_status(
                &state.db.pool,
                req.session_id,
                &SessionStatus::Expired,
                None,
            )
            .await?;
            Err(e)
        }
    }
}