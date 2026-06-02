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

// ── Invoicing (Phase 2) ─────────────────────────────────────────────────────
#[derive(Deserialize)]
pub struct InvoiceLineItemInput {
    pub name: String,
    pub quantity: i64,
    pub unit_amount: i64,
}

#[derive(Deserialize)]
pub struct CreateInvoiceRequest {
    pub customer_name: String,
    pub customer_email: String,
    pub amount: Option<i64>, // cents, single-line; ignored if line_items given
    pub description: Option<String>,
    pub due_date: Option<String>, // YYYY-MM-DD
    pub line_items: Option<Vec<InvoiceLineItemInput>>,
}

pub async fn create_invoice(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Json(req): Json<CreateInvoiceRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let name = req.customer_name.trim();
    let email = req.customer_email.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("customer name is required".into()));
    }
    if !email.contains('@') {
        return Err(AppError::BadRequest(
            "a valid customer email is required".into(),
        ));
    }

    // Build line items: explicit list, or a single line from amount + description.
    let items: Vec<paybank_payments::InvoiceLineItem> = match &req.line_items {
        Some(lis) if !lis.is_empty() => lis
            .iter()
            .map(|li| paybank_payments::InvoiceLineItem {
                name: li.name.clone(),
                quantity: li.quantity,
                unit_amount: li.unit_amount,
            })
            .collect(),
        _ => {
            let amount = req.amount.unwrap_or(0);
            if amount < 1 {
                return Err(AppError::BadRequest(
                    "amount (cents) or line_items required".into(),
                ));
            }
            vec![paybank_payments::InvoiceLineItem {
                name: req
                    .description
                    .clone()
                    .unwrap_or_else(|| "Invoice".to_string()),
                quantity: 1,
                unit_amount: amount,
            }]
        }
    };

    let created = paybank_payments::create_invoice(
        name,
        email,
        "USD",
        req.due_date.as_deref(),
        req.description.as_deref(),
        &items,
    )
    .await?;

    let id = Uuid::new_v4();
    let due = req
        .due_date
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok());

    sqlx::query(
        "INSERT INTO invoices (id, merchant_id, mt_invoice_id, mt_counterparty_id, number, \
         customer_name, customer_email, description, amount_cents, currency, status, hosted_url, due_date) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'USD',$10,$11,$12)",
    )
    .bind(id)
    .bind(merchant_id)
    .bind(&created.mt_invoice_id)
    .bind(&created.mt_counterparty_id)
    .bind(&created.number)
    .bind(name)
    .bind(email)
    .bind(&req.description)
    .bind(created.total_amount)
    .bind(&created.status)
    .bind(&created.hosted_url)
    .bind(due)
    .execute(&state.db.pool)
    .await?;

    Ok(Json(serde_json::json!({
        "id": id,
        "number": created.number,
        "status": created.status,
        "amount": created.total_amount,
        "currency": "USD",
        "hosted_url": created.hosted_url,
    })))
}

fn invoice_row_json(row: &sqlx::postgres::PgRow) -> serde_json::Value {
    serde_json::json!({
        "id": row.try_get::<Uuid, _>("id").ok(),
        "number": row.try_get::<Option<String>, _>("number").ok().flatten(),
        "customer_name": row.try_get::<String, _>("customer_name").unwrap_or_default(),
        "customer_email": row.try_get::<String, _>("customer_email").unwrap_or_default(),
        "amount": row.try_get::<i64, _>("amount_cents").unwrap_or(0),
        "currency": row.try_get::<String, _>("currency").unwrap_or_else(|_| "USD".into()),
        "status": row.try_get::<String, _>("status").unwrap_or_default(),
        "hosted_url": row.try_get::<Option<String>, _>("hosted_url").ok().flatten(),
        "created_at": row.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at").ok(),
    })
}

pub async fn list_invoices(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, number, customer_name, customer_email, amount_cents, currency, status, \
         hosted_url, created_at FROM invoices WHERE merchant_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(merchant_id)
    .fetch_all(&state.db.pool)
    .await?;
    let invoices: Vec<serde_json::Value> = rows.iter().map(invoice_row_json).collect();
    Ok(Json(serde_json::json!({ "invoices": invoices })))
}

pub async fn get_invoice(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query(
        "SELECT id, mt_invoice_id, number, customer_name, customer_email, amount_cents, currency, \
         status, hosted_url, created_at FROM invoices WHERE id = $1 AND merchant_id = $2",
    )
    .bind(id)
    .bind(merchant_id)
    .fetch_optional(&state.db.pool)
    .await?
    .ok_or(AppError::PaymentNotFound)?;

    // Best-effort status refresh from MT.
    if let Some(mt_id) = row
        .try_get::<Option<String>, _>("mt_invoice_id")
        .ok()
        .flatten()
    {
        if let Ok(latest) = paybank_payments::invoice_status(&mt_id).await {
            let cur: String = row.try_get("status").unwrap_or_default();
            if latest != cur {
                let _ = sqlx::query(
                    "UPDATE invoices SET status = $1, updated_at = NOW() WHERE id = $2",
                )
                .bind(&latest)
                .bind(id)
                .execute(&state.db.pool)
                .await;
                let mut j = invoice_row_json(&row);
                j["status"] = serde_json::Value::String(latest);
                return Ok(Json(j));
            }
        }
    }
    Ok(Json(invoice_row_json(&row)))
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
