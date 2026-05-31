use crate::auth::AuthedOperator;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use paybank_core::{AppError, Merchant};
use paybank_db::{admin_repo, operator_repo};
use serde::Deserialize;
use uuid::Uuid;

/// Project a Merchant to a safe response — NEVER expose password_hash or api_key
/// to the operator console (P0: these were previously serialized in full).
fn redact_merchant(m: &Merchant) -> serde_json::Value {
    serde_json::json!({
        "id": m.id,
        "name": m.name,
        "email": m.email,
        "webhook_url": m.webhook_url,
        "business_name": m.business_name,
        "business_type": m.business_type,
        "registration_number": m.registration_number,
        "tax_id": m.tax_id,
        "industry_category": m.industry_category,
        "website_url": m.website_url,
        "status": m.status,
        "kyc_status": m.kyc_status,
        "risk_level": m.risk_level,
        "onboarding_completed_at": m.onboarding_completed_at,
        "created_at": m.created_at,
        "updated_at": m.updated_at,
    })
}

#[derive(Deserialize)]
pub struct PaginationQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Deserialize)]
pub struct SessionFilterQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    status: Option<String>,
    rail: Option<String>,
    date_from: Option<chrono::NaiveDate>,
    date_to: Option<chrono::NaiveDate>,
}

#[derive(Deserialize)]
pub struct CreateMerchantBody {
    name: String,
    email: String,
}

#[derive(Deserialize)]
pub struct UpdateMerchantBody {
    business_name: Option<String>,
    business_type: Option<String>,
    tax_id: Option<String>,
    industry_category: Option<String>,
    website_url: Option<String>,
    webhook_url: Option<String>,
}

pub async fn get_stats(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let stats = admin_repo::get_dashboard_stats(&state.db.pool)
        .await
        .map_err(|e| AppError::Internal(e))?;
    Ok(Json(serde_json::to_value(stats).unwrap()))
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<SessionFilterQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (sessions, total) = admin_repo::list_all_sessions(
        &state.db.pool,
        limit,
        offset,
        q.status.as_deref(),
        q.rail.as_deref(),
        q.date_from,
        q.date_to,
    )
    .await
    .map_err(|e| AppError::Internal(e))?;
    Ok(Json(serde_json::json!({ "sessions": sessions, "total": total })))
}

pub async fn get_session_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = paybank_db::session_repo::get_session(&state.db.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::to_value(session).unwrap()))
}

pub async fn list_merchants(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (merchants, total) = admin_repo::list_merchants(&state.db.pool, limit, offset)
        .await
        .map_err(|e| AppError::Internal(e))?;
    let merchants: Vec<_> = merchants.iter().map(redact_merchant).collect();
    Ok(Json(serde_json::json!({ "merchants": merchants, "total": total })))
}

pub async fn get_merchant(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let merchant = admin_repo::get_merchant(&state.db.pool, id)
        .await
        .map_err(|e| AppError::Internal(e))?;
    Ok(Json(merchant.as_ref().map(redact_merchant).unwrap_or(serde_json::Value::Null)))
}

pub async fn create_merchant(
    State(state): State<AppState>,
    Json(body): Json<CreateMerchantBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let api_key = format!("admin_{}", Uuid::new_v4());
    let merchant = admin_repo::create_merchant(&state.db.pool, &body.name, &body.email, &api_key)
        .await
        .map_err(|e| AppError::Internal(e))?;
    Ok((StatusCode::CREATED, Json(redact_merchant(&merchant))))
}

pub async fn update_merchant(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateMerchantBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    let merchant = admin_repo::update_merchant(
        &state.db.pool,
        id,
        body.business_name.as_deref(),
        body.business_type.as_deref(),
        body.tax_id.as_deref(),
        body.industry_category.as_deref(),
        body.website_url.as_deref(),
        body.webhook_url.as_deref(),
    )
    .await
    .map_err(|e| AppError::Internal(e))?;
    Ok(Json(redact_merchant(&merchant)))
}

pub async fn list_banks(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let banks = paybank_db::bank_repo::list_banks(&state.db.pool)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "banks": banks })))
}

pub async fn list_settlements(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let settlements = admin_repo::get_settlements(&state.db.pool)
        .await
        .map_err(|e| AppError::Internal(e))?;
    Ok(Json(serde_json::json!({ "settlements": settlements })))
}

pub async fn list_webhooks(
    State(state): State<AppState>,
    Query(q): Query<PaginationQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (events, total) = admin_repo::list_webhook_events(&state.db.pool, limit, offset)
        .await
        .map_err(|e| AppError::Internal(e))?;
    Ok(Json(serde_json::json!({ "events": events, "total": total })))
}

/// Internal health/issues summary for the ops console.
pub async fn get_health(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let summary = admin_repo::get_issue_summary(&state.db.pool)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::to_value(summary).unwrap()))
}

// ---- operator (console user) management ----------------------------------

pub async fn list_operators(
    State(state): State<AppState>,
    Extension(AuthedOperator(_claims)): Extension<AuthedOperator>,
) -> Result<Json<serde_json::Value>, AppError> {
    let operators = operator_repo::list(&state.db.pool)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "operators": operators })))
}

#[derive(Deserialize)]
pub struct CreateOperatorBody {
    email: String,
    name: String,
    password: String,
    role: Option<String>,
}

pub async fn create_operator(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Json(body): Json<CreateOperatorBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    // Only an `owner` may mint new operators (no privilege escalation).
    if claims.role != "owner" {
        return Err(AppError::Unauthorized);
    }
    if body.password.len() < 12 {
        return Err(AppError::ComplianceRejected(
            "operator password must be at least 12 characters".into(),
        ));
    }
    let role = body.role.unwrap_or_else(|| "operator".into());
    let hash = crate::auth::hash_password(&body.password)?;
    let op = operator_repo::create(&state.db.pool, &body.email, &body.name, &hash, &role)
        .await
        .map_err(AppError::Internal)?;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(op).unwrap())))
}

/// Requeue a failed/exhausted webhook event for immediate redelivery.
pub async fn retry_webhook(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let requeued = paybank_db::webhook_repo::retry_now(&state.db.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    Ok(Json(serde_json::json!({ "requeued": requeued })))
}
