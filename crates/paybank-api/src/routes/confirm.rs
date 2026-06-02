use crate::state::AppState;
use axum::{extract::State, Json};
use paybank_core::{AppError, SessionStatus};
use paybank_db::{audit_repo, session_repo};
use paybank_payments::BankAccountDetails;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
pub struct ConfirmRequest {
    pub session_id: Uuid,
    pub customer_name: String,
    pub customer_city: String,
    pub customer_state: String,
    pub customer_address_line_1: String,
    pub customer_postal_code: String,
    pub customer_country_code: String,
    // Bank details entered directly by the customer (no Plaid).
    pub account_number: String,
    pub routing_number: String,
    pub account_type: String, // checking | savings
}

pub async fn confirm_payment(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
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

    // Validate the directly-entered bank details (no Plaid).
    let routing = req.routing_number.trim();
    let account = req.account_number.trim();
    let acct_type = req.account_type.trim().to_lowercase();
    if routing.len() != 9 || !routing.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest(
            "routing number must be 9 digits".into(),
        ));
    }
    if account.len() < 4 || account.len() > 17 || !account.chars().all(|c| c.is_ascii_digit()) {
        return Err(AppError::BadRequest(
            "account number must be 4–17 digits".into(),
        ));
    }
    if acct_type != "checking" && acct_type != "savings" {
        return Err(AppError::BadRequest(
            "account type must be checking or savings".into(),
        ));
    }

    // Fail-closed compliance gate: merchant KYC + customer sanctions screening.
    // Authorize is the single chokepoint before money can move, so gating here
    // transitively gates the initiate endpoints (which require Authorized).
    crate::compliance::gate(
        &state.db.pool,
        session.merchant_id,
        &req.customer_name,
        &req.customer_address_line_1,
    )
    .await?;

    // Optional idempotency keyed on the session's merchant — prevents a retry
    // from creating a SECOND Modern Treasury counterparty for the same session.
    let idem_key = crate::idempotency::key_from(&headers);
    if let Some(key) = &idem_key {
        let request_hash = crate::idempotency::hash(&req);
        match crate::idempotency::begin(&state.db, session.merchant_id, key, &request_hash).await? {
            crate::idempotency::Outcome::Replay(body) => return Ok(Json(body)),
            crate::idempotency::Outcome::Proceed => {}
        }
    }

    // Atomic claim: only one caller transitions Pending -> Authorized. This makes
    // confirm safe against double-submit even WITHOUT an idempotency key (the pay
    // page sends none), so a second click can't create a second MT counterparty.
    let claimed = session_repo::try_transition(
        &state.db.pool,
        req.session_id,
        &[SessionStatus::Pending],
        &SessionStatus::Authorized,
        None,
    )
    .await?;
    if !claimed {
        // Lost the race, or already past pending — report current state idempotently.
        let current = session_repo::get_session(&state.db.pool, req.session_id)
            .await?
            .map(|s| s.status)
            .unwrap_or(SessionStatus::Authorized);
        return Ok(Json(serde_json::json!({
            "status": current,
            "session_id": req.session_id,
        })));
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
        account,
        routing,
        &acct_type,
    )
    .await?;

    // Create the MT counterparty from the customer's entered bank details.
    let counterparty_id = {
        let details = BankAccountDetails {
            account_id: String::new(),
            account_name: req.customer_name.clone(),
            account_number: account.to_string(),
            routing_number: routing.to_string(),
            account_type: acct_type.clone(),
            subtype: acct_type.clone(),
            available_balance: None,
            current_balance: None,
            iso_currency_code: None,
        };
        let result = match paybank_payments::create_counterparty(&details, &req.customer_name).await
        {
            Ok(r) => r,
            Err(e) => {
                // We already claimed Authorized; mark Failed so the session is a
                // clear terminal (not a silent authorized-without-counterparty).
                let _ = session_repo::try_transition(
                    &state.db.pool,
                    req.session_id,
                    &[SessionStatus::Authorized],
                    &SessionStatus::Failed,
                    None,
                )
                .await;
                return Err(e);
            }
        };
        let stored = format!("{}|{}", result.counterparty_id, result.external_account_id);
        session_repo::update_counterparty_id(&state.db.pool, req.session_id, &stored).await?;
        Some(stored)
    };

    // Status already set to Authorized by the atomic claim above.

    let _ = paybank_db::webhook_repo::enqueue(
        &state.db.pool,
        session.merchant_id,
        Some(req.session_id),
        "session.authorized",
        serde_json::json!({ "session_id": req.session_id, "status": "authorized" }),
    )
    .await;

    audit_repo::record(
        &state.db.pool,
        "customer",
        None,
        "session.confirm",
        Some("session"),
        Some(&req.session_id.to_string()),
        serde_json::json!({ "counterparty_id": counterparty_id }),
    )
    .await;

    let response = serde_json::json!({
        "status": "authorized",
        "session_id": req.session_id,
        "counterparty_id": counterparty_id,
    });

    if let Some(key) = &idem_key {
        crate::idempotency::complete(&state.db, session.merchant_id, key, &response).await?;
    }

    Ok(Json(response))
}
