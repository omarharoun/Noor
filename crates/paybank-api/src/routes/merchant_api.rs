use crate::auth::AuthedMerchant;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::{Extension, Json};
use chrono::{Duration, Utc};
use paybank_core::AppError;
use paybank_core::PaymentRail;
use paybank_db::{bank_repo, session_repo, transaction_repo};
use serde::{Deserialize, Serialize};
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
        .map_err(|e| AppError::Internal(e.into()))?;

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

pub async fn create_payment_link(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Json(req): Json<CreatePaymentLinkRequest>,
) -> Result<Json<CreatePaymentLinkResponse>, AppError> {
    let bank_id = "021000021";

    let bank =
        bank_repo::get_bank(&state.db.pool, bank_id)
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

    Ok(Json(CreatePaymentLinkResponse {
        checkout_url,
        id,
    }))
}
