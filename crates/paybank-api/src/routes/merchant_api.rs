use crate::auth::AuthedMerchant;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use chrono::{Duration, Utc};
use paybank_core::AppError;
use paybank_core::PaymentRail;
use paybank_db::{bank_repo, session_repo, transaction_repo};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

#[derive(Serialize)]
pub struct BalanceResponse {
    pub merchant_id: Uuid,
    pub available: i64,
    pub pending: i64,
    pub currency: String,
}

pub async fn get_balance(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Path(_path_id): Path<Uuid>,
) -> Result<Json<BalanceResponse>, AppError> {
    // Use the authenticated merchant id, never the path param (closes IDOR).
    // available = net credit on the merchant's Settlement (liability) ledger
    // account; pending = sum of their in-flight (processing) sessions.
    let available =
        paybank_db::ledger_repo::LedgerRepo::merchant_available_cents(&state.db.pool, merchant_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    let pending = session_repo::pending_amount_cents(&state.db.pool, merchant_id)
        .await
        .map_err(AppError::Internal)?;

    Ok(Json(BalanceResponse {
        merchant_id,
        available,
        pending,
        currency: "USD".into(),
    }))
}

pub async fn get_merchant_profile() -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(serde_json::json!({
        "status": "active",
        "kyc_status": "verified",
        "onboarding_completed_at": null
    })))
}

#[derive(Deserialize)]
pub struct PaymentsQuery {
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

pub async fn list_payments(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Query(q): Query<PaymentsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(20);
    let offset = q.page.unwrap_or(0) * limit;
    let (transactions, _total) =
        transaction_repo::list_transactions(&state.db.pool, merchant_id, limit, offset).await?;

    let payments: Vec<serde_json::Value> = transactions
        .iter()
        .map(|t| {
            serde_json::json!({
                "id": t.id,
                "status": "settled",
                "amount": t.amount_cents,
                "currency": "USD",
                "method": t.rail_used,
                "provider": "moderntreasury",
                "created_at": t.created_at,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({ "payments": payments })))
}

#[derive(Deserialize)]
pub struct TimeseriesQuery {
    pub granularity: Option<String>,
}

pub async fn payment_timeseries() -> Result<Json<Vec<serde_json::Value>>, AppError> {
    Ok(Json(vec![]))
}

#[derive(Deserialize)]
pub struct CreatePaymentLinkRequest {
    pub amount: i64,
    pub currency: Option<String>,
    pub method: Option<String>,
    pub country: Option<String>,
    pub description: Option<String>,
    pub checkout_host: Option<String>,
}

#[derive(Serialize)]
pub struct CreatePaymentLinkResponse {
    pub checkout_url: String,
    pub id: Uuid,
}

// ── Merchant bank account (Phase 1: onboard via MT, no Plaid) ───────────────
#[derive(Deserialize)]
pub struct AddBankAccountRequest {
    pub account_holder_name: String,
    pub account_type: String, // checking | savings
    pub routing_number: String,
    pub account_number: String,
}

/// Add (or replace) the merchant's payout/settlement bank account. Sends the
/// details to Modern Treasury to create the counterparty + external account and
/// starts ACH prenote verification. Raw numbers are not stored in Noor — only
/// the MT ids, the verification status, and a masked last-4.
pub async fn add_bank_account(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Json(req): Json<AddBankAccountRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let holder = req.account_holder_name.trim();
    let routing = req.routing_number.trim();
    let account = req.account_number.trim();
    let acct_type = req.account_type.trim().to_lowercase();

    if holder.is_empty() {
        return Err(AppError::BadRequest(
            "account holder name is required".into(),
        ));
    }
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

    let (cp_id, ext_id, status) = paybank_payments::onboard_merchant_bank_account(
        holder, holder, &acct_type, routing, account,
    )
    .await?;

    let last4 = account[account.len() - 4..].to_string();

    sqlx::query(
        "UPDATE merchants SET mt_counterparty_id = $1, mt_external_account_id = $2, \
         bank_account_status = $3, bank_account_last4 = $4, updated_at = NOW() WHERE id = $5",
    )
    .bind(&cp_id)
    .bind(&ext_id)
    .bind(&status)
    .bind(&last4)
    .bind(merchant_id)
    .execute(&state.db.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "status": status,
        "last4": last4,
        "account_type": acct_type,
    })))
}

/// Return the merchant's bank-account status. If an account exists, polls MT for
/// the latest verification status and persists any change.
pub async fn get_bank_account(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query(
        "SELECT mt_external_account_id, bank_account_status, bank_account_last4 \
         FROM merchants WHERE id = $1",
    )
    .bind(merchant_id)
    .fetch_one(&state.db.pool)
    .await?;

    let ext_id: Option<String> = row.try_get("mt_external_account_id").ok().flatten();
    let mut status: Option<String> = row.try_get("bank_account_status").ok().flatten();
    let last4: Option<String> = row.try_get("bank_account_last4").ok().flatten();

    let Some(ext) = ext_id else {
        return Ok(Json(
            serde_json::json!({ "has_account": false, "status": "none" }),
        ));
    };

    // Best-effort refresh from MT; if it changed, persist it.
    if let Ok(latest) = paybank_payments::merchant_bank_account_status(&ext).await {
        if Some(&latest) != status.as_ref() {
            let _ = sqlx::query(
                "UPDATE merchants SET bank_account_status = $1, updated_at = NOW() WHERE id = $2",
            )
            .bind(&latest)
            .bind(merchant_id)
            .execute(&state.db.pool)
            .await;
            status = Some(latest);
        }
    }

    Ok(Json(serde_json::json!({
        "has_account": true,
        "status": status.unwrap_or_else(|| "unverified".into()),
        "last4": last4,
    })))
}

pub async fn create_payment_link(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Json(req): Json<CreatePaymentLinkRequest>,
) -> Result<Json<CreatePaymentLinkResponse>, AppError> {
    let bank_id = "021000021";

    let bank = bank_repo::get_bank(&state.db.pool, bank_id)
        .await?
        .ok_or(AppError::BankNotFound)?;

    let rail = PaymentRail::choose(bank.supports_fednow, bank.supports_rtp, req.amount);
    let expires_at = Utc::now() + Duration::hours(24);
    let id = Uuid::new_v4();

    session_repo::create_session(
        &state.db.pool,
        id,
        merchant_id,
        bank_id,
        req.amount,
        req.description.as_deref(),
        &rail,
        expires_at,
    )
    .await?;

    let host = req
        .checkout_host
        .unwrap_or_else(|| state.config.public_app_url.clone());
    let checkout_url = format!("{}/pay/{}", host, id);

    Ok(Json(CreatePaymentLinkResponse { checkout_url, id }))
}
