use crate::auth::AuthedOperator;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use paybank_core::{AppError, Merchant};
use paybank_db::{admin_repo, audit_repo, operator_repo};
use serde::Deserialize;
use sqlx::Row;
use uuid::Uuid;

/// Operator oversight: all payouts across merchants (read-only).
pub async fn list_all_payouts(
    State(state): State<AppState>,
    Extension(AuthedOperator(_claims)): Extension<AuthedOperator>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT p.payee_name, p.amount_cents, p.rail, p.status, p.created_at, m.name AS merchant_name \
         FROM payouts p JOIN merchants m ON m.id = p.merchant_id \
         ORDER BY p.created_at DESC LIMIT 200",
    )
    .fetch_all(&state.db.pool)
    .await?;
    let payouts: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "merchant": r.try_get::<String,_>("merchant_name").unwrap_or_default(),
                "payee": r.try_get::<String,_>("payee_name").unwrap_or_default(),
                "amount": r.try_get::<i64,_>("amount_cents").unwrap_or(0),
                "rail": r.try_get::<String,_>("rail").unwrap_or_default(),
                "status": r.try_get::<String,_>("status").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").ok(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "payouts": payouts })))
}

/// Operator oversight: all invoices across merchants.
pub async fn list_all_invoices(
    State(state): State<AppState>,
    Extension(AuthedOperator(_claims)): Extension<AuthedOperator>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT i.number, i.customer_name, i.amount_cents, i.status, i.created_at, m.name AS merchant_name \
         FROM invoices i JOIN merchants m ON m.id = i.merchant_id \
         ORDER BY i.created_at DESC LIMIT 200",
    )
    .fetch_all(&state.db.pool)
    .await?;
    let invoices: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "merchant": r.try_get::<String,_>("merchant_name").unwrap_or_default(),
                "number": r.try_get::<Option<String>,_>("number").ok().flatten(),
                "customer": r.try_get::<String,_>("customer_name").unwrap_or_default(),
                "amount": r.try_get::<i64,_>("amount_cents").unwrap_or(0),
                "status": r.try_get::<String,_>("status").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").ok(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "invoices": invoices })))
}

/// One-time maintenance: backfill `supported_rails` for payees onboarded before
/// rail-aware routing existed. Looks up each payee's routing number on its MT
/// external account, then asks MT which rails it supports. Idempotent — only
/// touches rows with no cached rails — so it's safe to re-run.
pub async fn backfill_payee_rails(
    State(state): State<AppState>,
    Extension(AuthedOperator(_claims)): Extension<AuthedOperator>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, mt_external_account_id FROM payees \
         WHERE supported_rails IS NULL OR supported_rails = ''",
    )
    .fetch_all(&state.db.pool)
    .await?;
    let candidates = rows.len();
    let mut updated = 0u32;
    for r in &rows {
        let id: Uuid = match r.try_get("id") {
            Ok(v) => v,
            Err(_) => continue,
        };
        let ext: Option<String> = r.try_get("mt_external_account_id").ok().flatten();
        let Some(ext) = ext.filter(|s| !s.is_empty()) else {
            continue;
        };
        let Some(routing) = paybank_payments::external_account_routing(&ext).await else {
            continue;
        };
        let rails = paybank_payments::supported_rails(&routing).await;
        if rails.is_empty() {
            continue;
        }
        let _ = sqlx::query("UPDATE payees SET supported_rails = $1 WHERE id = $2")
            .bind(rails.join(","))
            .bind(id)
            .execute(&state.db.pool)
            .await;
        updated += 1;
    }
    Ok(Json(
        serde_json::json!({ "candidates": candidates, "backfilled": updated }),
    ))
}

/// Operator oversight: all wallet top-ups (deposits) across merchants.
pub async fn list_all_deposits(
    State(state): State<AppState>,
    Extension(AuthedOperator(_claims)): Extension<AuthedOperator>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT d.amount_cents, d.status, d.created_at, m.name AS merchant_name \
         FROM deposits d JOIN merchants m ON m.id = d.merchant_id \
         ORDER BY d.created_at DESC LIMIT 200",
    )
    .fetch_all(&state.db.pool)
    .await?;
    let deposits: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "merchant": r.try_get::<String,_>("merchant_name").unwrap_or_default(),
                "amount": r.try_get::<i64,_>("amount_cents").unwrap_or(0),
                "status": r.try_get::<String,_>("status").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").ok(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "deposits": deposits })))
}

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

pub async fn get_stats(State(state): State<AppState>) -> Result<Json<serde_json::Value>, AppError> {
    let stats = admin_repo::get_dashboard_stats(&state.db.pool)
        .await
        .map_err(AppError::Internal)?;
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
    .map_err(AppError::Internal)?;
    Ok(Json(
        serde_json::json!({ "sessions": sessions, "total": total }),
    ))
}

pub async fn get_session_detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = paybank_db::session_repo::get_session(&state.db.pool, id)
        .await
        .map_err(AppError::Internal)?;
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
        .map_err(AppError::Internal)?;
    let merchants: Vec<_> = merchants.iter().map(redact_merchant).collect();
    Ok(Json(
        serde_json::json!({ "merchants": merchants, "total": total }),
    ))
}

pub async fn get_merchant(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let merchant = admin_repo::get_merchant(&state.db.pool, id)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(
        merchant
            .as_ref()
            .map(redact_merchant)
            .unwrap_or(serde_json::Value::Null),
    ))
}

#[derive(Deserialize)]
pub struct CreateMerchantUserBody {
    pub email: String,
    pub name: String,
    pub password: String,
    pub role: Option<String>,
}

/// Operator onboards a merchant's first user (typically an `owner`). This is the
/// bootstrap that lets a merchant then sign in and invite their own team.
pub async fn create_merchant_user(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Path(merchant_id): Path<Uuid>,
    Json(body): Json<CreateMerchantUserBody>,
) -> Result<(StatusCode, Json<crate::auth::MerchantUserView>), AppError> {
    if body.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    let role = body.role.as_deref().unwrap_or("owner");
    let user = crate::auth::create_merchant_user(
        &state.db.pool,
        merchant_id,
        body.email.trim(),
        body.name.trim(),
        &body.password,
        role,
    )
    .await?;
    audit_repo::record(
        &state.db.pool,
        &claims.sub,
        Some(&claims.role),
        "merchant_user.create",
        Some("merchant_user"),
        Some(&user.id.to_string()),
        serde_json::json!({ "merchant_id": merchant_id, "role": role }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(user.into())))
}

pub async fn create_merchant(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Json(body): Json<CreateMerchantBody>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let api_key = format!("admin_{}", Uuid::new_v4());
    let merchant = admin_repo::create_merchant(&state.db.pool, &body.name, &body.email, &api_key)
        .await
        .map_err(AppError::Internal)?;
    audit_repo::record(
        &state.db.pool,
        &claims.sub,
        Some(&claims.role),
        "merchant.create",
        Some("merchant"),
        Some(&merchant.id.to_string()),
        serde_json::json!({ "name": merchant.name, "email": merchant.email }),
    )
    .await;
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
    .map_err(AppError::Internal)?;
    Ok(Json(redact_merchant(&merchant)))
}

pub async fn list_banks(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let banks = paybank_db::bank_repo::list_banks(&state.db.pool)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "banks": banks })))
}

pub async fn list_settlements(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, AppError> {
    let settlements = admin_repo::get_settlements(&state.db.pool)
        .await
        .map_err(AppError::Internal)?;
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
        .map_err(AppError::Internal)?;
    Ok(Json(
        serde_json::json!({ "events": events, "total": total }),
    ))
}

/// Recent audit-log entries (who did what).
pub async fn get_audit(
    State(state): State<AppState>,
    Extension(AuthedOperator(_claims)): Extension<AuthedOperator>,
) -> Result<Json<serde_json::Value>, AppError> {
    let entries = audit_repo::list(&state.db.pool, 100)
        .await
        .map_err(AppError::Internal)?;
    Ok(Json(serde_json::json!({ "entries": entries })))
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
    audit_repo::record(
        &state.db.pool,
        &claims.sub,
        Some(&claims.role),
        "operator.create",
        Some("operator"),
        Some(&op.id.to_string()),
        serde_json::json!({ "email": op.email, "role": op.role }),
    )
    .await;
    Ok((StatusCode::CREATED, Json(serde_json::to_value(op).unwrap())))
}

#[derive(Deserialize)]
pub struct UpdateOperatorBody {
    is_active: Option<bool>,
    role: Option<String>,
}

/// Owner-only: deactivate/reactivate or change an operator's role. Guards against
/// the caller locking themselves out (no self-deactivate, no self-demote).
pub async fn update_operator(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateOperatorBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    if claims.role != "owner" {
        return Err(AppError::Unauthorized);
    }
    let target = operator_repo::get_summary_by_id(&state.db.pool, id)
        .await
        .map_err(AppError::Internal)?
        .ok_or(AppError::Unauthorized)?;
    if target.email == claims.sub {
        if body.is_active == Some(false) {
            return Err(AppError::ComplianceRejected(
                "cannot deactivate your own account".into(),
            ));
        }
        if body.role.as_deref() == Some("operator") {
            return Err(AppError::ComplianceRejected(
                "cannot demote your own owner account".into(),
            ));
        }
    }
    let updated = operator_repo::update(&state.db.pool, id, body.is_active, body.role.as_deref())
        .await
        .map_err(AppError::Internal)?;
    audit_repo::record(
        &state.db.pool,
        &claims.sub,
        Some(&claims.role),
        "operator.update",
        Some("operator"),
        Some(&updated.id.to_string()),
        serde_json::json!({ "is_active": updated.is_active, "role": updated.role }),
    )
    .await;
    Ok(Json(serde_json::to_value(updated).unwrap()))
}

/// Requeue a failed/exhausted webhook event for immediate redelivery.
pub async fn retry_webhook(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let requeued = paybank_db::webhook_repo::retry_now(&state.db.pool, id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    audit_repo::record(
        &state.db.pool,
        &claims.sub,
        Some(&claims.role),
        "webhook.retry",
        Some("webhook"),
        Some(&id.to_string()),
        serde_json::json!({ "requeued": requeued }),
    )
    .await;
    Ok(Json(serde_json::json!({ "requeued": requeued })))
}
