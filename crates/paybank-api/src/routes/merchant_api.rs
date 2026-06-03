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

// ── Wallet (Phase 6): add funds (top-up) + withdraw ─────────────────────────
/// Credit a deposit's ledger entry exactly once (CAS on `credited`).
async fn settle_deposit(pool: &sqlx::PgPool, deposit_id: Uuid, merchant_id: Uuid, amount: i64) {
    let claimed = sqlx::query(
        "UPDATE deposits SET status='completed', credited=true, updated_at=NOW() \
         WHERE id=$1 AND credited=false RETURNING id",
    )
    .bind(deposit_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();
    if claimed.is_some() {
        let _ = paybank_db::ledger_repo::LedgerRepo::record_deposit(
            pool,
            merchant_id,
            deposit_id,
            amount,
        )
        .await;
    }
}

#[derive(Deserialize)]
pub struct AmountRequest {
    pub amount: i64,
}

pub async fn add_funds(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Json(req): Json<AmountRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if req.amount < 1 {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    let row = sqlx::query("SELECT mt_external_account_id FROM merchants WHERE id=$1")
        .bind(merchant_id)
        .fetch_one(&state.db.pool)
        .await?;
    let ext: Option<String> = row.try_get("mt_external_account_id").ok().flatten();
    let ext = ext.ok_or_else(|| {
        AppError::BadRequest("add and verify a bank account before adding funds".into())
    })?;

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO deposits (id, merchant_id, amount_cents, currency, status) \
         VALUES ($1,$2,$3,'USD','pending')",
    )
    .bind(id)
    .bind(merchant_id)
    .bind(req.amount)
    .execute(&state.db.pool)
    .await?;

    let idem = format!("noor-deposit-{}", id);
    let (mt_id, _) = paybank_payments::add_funds(&ext, req.amount, "Wallet top-up", &idem).await?;
    let _ = sqlx::query("UPDATE deposits SET mt_payment_order_id=$1 WHERE id=$2")
        .bind(&mt_id)
        .bind(id)
        .execute(&state.db.pool)
        .await;

    // Poll briefly for settlement (the rail clears in seconds in sandbox);
    // otherwise the reconcile poller credits it when it completes.
    let mut status = "pending".to_string();
    for _ in 0..3 {
        match paybank_payments::check_transfer_status("modern_treasury", &mt_id).await {
            Ok(s) if matches!(s.as_str(), "completed" | "settled") => {
                settle_deposit(&state.db.pool, id, merchant_id, req.amount).await;
                status = "completed".into();
                break;
            }
            Ok(s) if matches!(s.as_str(), "failed" | "returned" | "cancelled") => {
                let _ = sqlx::query(
                    "UPDATE deposits SET status='failed', updated_at=NOW() WHERE id=$1",
                )
                .bind(id)
                .execute(&state.db.pool)
                .await;
                status = "failed".into();
                break;
            }
            _ => tokio::time::sleep(std::time::Duration::from_millis(1200)).await,
        }
    }
    Ok(Json(serde_json::json!({
        "id": id, "amount": req.amount, "status": status,
    })))
}

pub async fn list_deposits(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, amount_cents, status, created_at FROM deposits \
         WHERE merchant_id=$1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(merchant_id)
    .fetch_all(&state.db.pool)
    .await?;
    let deposits: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.try_get::<Uuid,_>("id").ok(),
                "amount": r.try_get::<i64,_>("amount_cents").unwrap_or(0),
                "status": r.try_get::<String,_>("status").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").ok(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "deposits": deposits })))
}

/// Withdraw to the merchant's own onboarded bank account — a payout to self,
/// following the same dual-control rules as any payout.
pub async fn withdraw(
    State(state): State<AppState>,
    actor: crate::auth::AuthedMerchantUser,
    Json(req): Json<AmountRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if req.amount < 1 {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    let row = sqlx::query(
        "SELECT mt_counterparty_id, mt_external_account_id, bank_account_last4 \
         FROM merchants WHERE id=$1",
    )
    .bind(actor.merchant_id)
    .fetch_one(&state.db.pool)
    .await?;
    let cp: Option<String> = row.try_get("mt_counterparty_id").ok().flatten();
    let ext: Option<String> = row.try_get("mt_external_account_id").ok().flatten();
    let last4: Option<String> = row.try_get("bank_account_last4").ok().flatten();
    let ext = ext.ok_or_else(|| {
        AppError::BadRequest("add and verify a bank account before withdrawing".into())
    })?;

    let spendable = merchant_spendable(&state.db.pool, actor.merchant_id).await?;
    if req.amount > spendable {
        return Err(AppError::BadRequest(format!(
            "amount exceeds available balance (${:.2})",
            spendable as f64 / 100.0
        )));
    }

    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO payouts (id, merchant_id, payee_name, payee_counterparty_id, \
         payee_external_account_id, payee_last4, amount_cents, currency, rail, description, \
         status, initiated_by) VALUES ($1,$2,$3,$4,$5,$6,$7,'USD','ach',$8,'pending_approval',$9)",
    )
    .bind(id)
    .bind(actor.merchant_id)
    .bind("Withdrawal to bank")
    .bind(&cp)
    .bind(&ext)
    .bind(&last4)
    .bind(req.amount)
    .bind("Withdrawal to bank")
    .bind(actor.user_id)
    .execute(&state.db.pool)
    .await?;

    let owner_initiated = actor.role == "owner";
    let status = if owner_initiated {
        execute_payout(&state.db.pool, id, actor.user_id).await?
    } else {
        "pending_approval".into()
    };
    Ok(Json(serde_json::json!({
        "id": id, "status": status, "amount": req.amount, "needs_approval": !owner_initiated,
    })))
}

// ── Accounting (Phase 5): reports + CSV export ──────────────────────────────
pub async fn report_summary(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
) -> Result<Json<serde_json::Value>, AppError> {
    let pool = &state.db.pool;
    let available =
        paybank_db::ledger_repo::LedgerRepo::merchant_available_cents(pool, merchant_id)
            .await
            .map_err(|e| AppError::Internal(e.into()))?;
    let pending = session_repo::pending_amount_cents(pool, merchant_id)
        .await
        .map_err(AppError::Internal)?;

    let scal = |sql: &'static str| {
        let pool = pool.clone();
        async move {
            sqlx::query_scalar::<_, i64>(sql)
                .bind(merchant_id)
                .fetch_one(&pool)
                .await
                .unwrap_or(0)
        }
    };
    let collected =
        scal("SELECT COALESCE(SUM(amount_cents),0)::int8 FROM transactions WHERE merchant_id=$1")
            .await;
    let txn_count = scal("SELECT COUNT(*)::int8 FROM transactions WHERE merchant_id=$1").await;
    let invoices_total =
        scal("SELECT COALESCE(SUM(amount_cents),0)::int8 FROM invoices WHERE merchant_id=$1").await;
    let invoices_count = scal("SELECT COUNT(*)::int8 FROM invoices WHERE merchant_id=$1").await;
    let invoices_paid =
        scal("SELECT COUNT(*)::int8 FROM invoices WHERE merchant_id=$1 AND status='paid'").await;
    let invoices_unpaid =
        scal("SELECT COUNT(*)::int8 FROM invoices WHERE merchant_id=$1 AND status<>'paid'").await;
    let payouts_total = scal(
        "SELECT COALESCE(SUM(amount_cents),0)::int8 FROM payouts WHERE merchant_id=$1 AND status IN ('processing','completed')",
    )
    .await;
    let payouts_count = scal(
        "SELECT COUNT(*)::int8 FROM payouts WHERE merchant_id=$1 AND status IN ('processing','completed')",
    )
    .await;
    let payouts_pending = scal(
        "SELECT COUNT(*)::int8 FROM payouts WHERE merchant_id=$1 AND status='pending_approval'",
    )
    .await;

    Ok(Json(serde_json::json!({
        "balance": { "available": available, "pending": pending, "currency": "USD" },
        "collected": { "total": collected, "count": txn_count },
        "invoices": { "count": invoices_count, "paid": invoices_paid, "unpaid": invoices_unpaid, "total": invoices_total },
        "payouts": { "count": payouts_count, "total": payouts_total, "pending_approval": payouts_pending },
    })))
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// CSV export of payments | invoices | payouts for the authenticated merchant.
pub async fn export_csv(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Path(kind): Path<String>,
) -> Result<axum::response::Response, AppError> {
    use axum::response::IntoResponse;
    let pool = &state.db.pool;
    let (header, rows): (&str, Vec<String>) = match kind.as_str() {
        "payments" => {
            let (txns, _) =
                transaction_repo::list_transactions(pool, merchant_id, 10000, 0).await?;
            let rows = txns
                .iter()
                .map(|t| {
                    format!(
                        "{},{},{:.2},{},{}",
                        t.id,
                        t.session_id,
                        t.amount_cents as f64 / 100.0,
                        csv_escape(&format!("{:?}", t.rail_used)),
                        t.created_at.format("%Y-%m-%d")
                    )
                })
                .collect();
            ("id,session_id,amount,rail,date", rows)
        }
        "invoices" => {
            let recs = sqlx::query(
                "SELECT number, customer_name, customer_email, amount_cents, status, created_at \
                 FROM invoices WHERE merchant_id=$1 ORDER BY created_at DESC LIMIT 10000",
            )
            .bind(merchant_id)
            .fetch_all(pool)
            .await?;
            let rows = recs
                .iter()
                .map(|r| {
                    format!(
                        "{},{},{},{:.2},{},{}",
                        csv_escape(
                            &r.try_get::<Option<String>, _>("number")
                                .ok()
                                .flatten()
                                .unwrap_or_default()
                        ),
                        csv_escape(&r.try_get::<String, _>("customer_name").unwrap_or_default()),
                        csv_escape(&r.try_get::<String, _>("customer_email").unwrap_or_default()),
                        r.try_get::<i64, _>("amount_cents").unwrap_or(0) as f64 / 100.0,
                        r.try_get::<String, _>("status").unwrap_or_default(),
                        r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default(),
                    )
                })
                .collect();
            ("number,customer,email,amount,status,date", rows)
        }
        "payouts" => {
            let recs = sqlx::query(
                "SELECT payee_name, amount_cents, rail, status, created_at \
                 FROM payouts WHERE merchant_id=$1 ORDER BY created_at DESC LIMIT 10000",
            )
            .bind(merchant_id)
            .fetch_all(pool)
            .await?;
            let rows = recs
                .iter()
                .map(|r| {
                    format!(
                        "{},{:.2},{},{},{}",
                        csv_escape(&r.try_get::<String, _>("payee_name").unwrap_or_default()),
                        r.try_get::<i64, _>("amount_cents").unwrap_or(0) as f64 / 100.0,
                        r.try_get::<String, _>("rail").unwrap_or_default(),
                        r.try_get::<String, _>("status").unwrap_or_default(),
                        r.try_get::<chrono::DateTime<chrono::Utc>, _>("created_at")
                            .map(|d| d.format("%Y-%m-%d").to_string())
                            .unwrap_or_default(),
                    )
                })
                .collect();
            ("payee,amount,rail,status,date", rows)
        }
        _ => return Err(AppError::BadRequest("unknown export type".into())),
    };
    let mut body = String::from(header);
    body.push('\n');
    for r in rows {
        body.push_str(&r);
        body.push('\n');
    }
    Ok((
        [
            (axum::http::header::CONTENT_TYPE, "text/csv".to_string()),
            (
                axum::http::header::CONTENT_DISPOSITION,
                format!("attachment; filename=\"noor-{}.csv\"", kind),
            ),
        ],
        body,
    )
        .into_response())
}

// ── Payouts (Phase 3b): send money with dual-control approval ───────────────
fn map_rail(s: &str) -> PaymentRail {
    match s.to_lowercase().as_str() {
        "wire" => PaymentRail::Wire,
        "rtp" => PaymentRail::Rtp,
        "fednow" => PaymentRail::FedNow,
        _ => PaymentRail::Ach,
    }
}

/// Spendable = ledger balance minus funds held by payouts still awaiting
/// approval (those haven't posted a ledger debit yet).
async fn merchant_spendable(pool: &sqlx::PgPool, merchant_id: Uuid) -> Result<i64, AppError> {
    let avail = paybank_db::ledger_repo::LedgerRepo::merchant_available_cents(pool, merchant_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let held: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_cents),0)::int8 FROM payouts \
         WHERE merchant_id = $1 AND status = 'pending_approval'",
    )
    .bind(merchant_id)
    .fetch_one(pool)
    .await?;
    Ok(avail - held)
}

/// Execute an approved payout: claim it (CAS), debit the ledger, send via the
/// rails. Reverses the ledger debit if the provider call fails.
async fn execute_payout(
    pool: &sqlx::PgPool,
    payout_id: Uuid,
    approver_id: Uuid,
) -> Result<String, AppError> {
    // CAS-claim so a payout executes exactly once.
    let row = sqlx::query(
        "UPDATE payouts SET status='processing', approved_by=$2, updated_at=NOW() \
         WHERE id=$1 AND status IN ('pending_approval','approved') \
         RETURNING merchant_id, payee_external_account_id, amount_cents, rail, description",
    )
    .bind(payout_id)
    .bind(approver_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::BadRequest("payout is not in an approvable state".into()))?;

    let merchant_id: Uuid = row.try_get("merchant_id")?;
    let ext: Option<String> = row.try_get("payee_external_account_id").ok().flatten();
    let amount: i64 = row.try_get("amount_cents")?;
    let rail: String = row.try_get("rail").unwrap_or_else(|_| "ach".into());
    let desc: Option<String> = row.try_get("description").ok().flatten();
    let ext =
        ext.ok_or_else(|| AppError::Internal(anyhow::anyhow!("payout missing payee account")))?;

    // Re-check the ledger balance at execution time (it may have moved).
    let avail = paybank_db::ledger_repo::LedgerRepo::merchant_available_cents(pool, merchant_id)
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    if amount > avail {
        let _ = sqlx::query(
            "UPDATE payouts SET status='pending_approval', updated_at=NOW() WHERE id=$1",
        )
        .bind(payout_id)
        .execute(pool)
        .await;
        return Err(AppError::BadRequest(
            "insufficient balance to execute this payout".into(),
        ));
    }

    // Debit the ledger first, then send. Reverse the debit if the send fails.
    paybank_db::ledger_repo::LedgerRepo::record_payout(pool, merchant_id, payout_id, amount)
        .await?;

    let idem = format!("noor-payout-{}", payout_id);
    match paybank_payments::send_payout(
        &ext,
        &map_rail(&rail),
        amount,
        desc.as_deref().unwrap_or("Payout"),
        &idem,
    )
    .await
    {
        Ok((mt_id, _mt_status)) => {
            let _ = sqlx::query(
                "UPDATE payouts SET mt_payment_order_id=$1, status='processing', updated_at=NOW() WHERE id=$2",
            )
            .bind(&mt_id)
            .bind(payout_id)
            .execute(pool)
            .await;
            Ok("processing".into())
        }
        Err(e) => {
            // Roll back the ledger debit and mark failed.
            let _ = paybank_db::ledger_repo::LedgerRepo::record_payout_reversal(
                pool,
                merchant_id,
                payout_id,
                amount,
            )
            .await;
            let _ = sqlx::query("UPDATE payouts SET status='failed', updated_at=NOW() WHERE id=$1")
                .bind(payout_id)
                .execute(pool)
                .await;
            Err(e)
        }
    }
}

#[derive(Deserialize)]
pub struct CreatePayoutRequest {
    pub payee_name: String,
    pub account_type: String,
    pub routing_number: String,
    pub account_number: String,
    pub amount: i64,
    pub rail: Option<String>,
    pub description: Option<String>,
}

pub async fn create_payout(
    State(state): State<AppState>,
    actor: crate::auth::AuthedMerchantUser,
    Json(req): Json<CreatePayoutRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let payee = req.payee_name.trim();
    let routing = req.routing_number.trim();
    let account = req.account_number.trim();
    let acct_type = req.account_type.trim().to_lowercase();
    if payee.is_empty() {
        return Err(AppError::BadRequest("payee name is required".into()));
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
    if req.amount < 1 {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }

    let spendable = merchant_spendable(&state.db.pool, actor.merchant_id).await?;
    if req.amount > spendable {
        return Err(AppError::BadRequest(format!(
            "amount exceeds available balance (${:.2})",
            spendable as f64 / 100.0
        )));
    }

    let (cp_id, ext_id) =
        paybank_payments::onboard_payee(payee, &acct_type, routing, account).await?;
    let last4 = account[account.len() - 4..].to_string();
    let rail = req
        .rail
        .clone()
        .unwrap_or_else(|| "ach".into())
        .to_lowercase();
    let owner_initiated = actor.role == "owner";
    let id = Uuid::new_v4();

    sqlx::query(
        "INSERT INTO payouts (id, merchant_id, payee_name, payee_counterparty_id, \
         payee_external_account_id, payee_last4, amount_cents, currency, rail, description, \
         status, initiated_by) VALUES ($1,$2,$3,$4,$5,$6,$7,'USD',$8,$9,'pending_approval',$10)",
    )
    .bind(id)
    .bind(actor.merchant_id)
    .bind(payee)
    .bind(&cp_id)
    .bind(&ext_id)
    .bind(&last4)
    .bind(req.amount)
    .bind(&rail)
    .bind(&req.description)
    .bind(actor.user_id)
    .execute(&state.db.pool)
    .await?;

    // Owner self-approves: execute immediately. Otherwise it waits for approval.
    let status = if owner_initiated {
        execute_payout(&state.db.pool, id, actor.user_id).await?
    } else {
        "pending_approval".into()
    };

    Ok(Json(serde_json::json!({
        "id": id,
        "status": status,
        "amount": req.amount,
        "payee_last4": last4,
        "needs_approval": !owner_initiated,
    })))
}

pub async fn list_payouts(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, payee_name, payee_last4, amount_cents, currency, rail, status, \
         created_at FROM payouts WHERE merchant_id = $1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(merchant_id)
    .fetch_all(&state.db.pool)
    .await?;
    let payouts: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.try_get::<Uuid,_>("id").ok(),
                "payee_name": r.try_get::<String,_>("payee_name").unwrap_or_default(),
                "payee_last4": r.try_get::<Option<String>,_>("payee_last4").ok().flatten(),
                "amount": r.try_get::<i64,_>("amount_cents").unwrap_or(0),
                "rail": r.try_get::<String,_>("rail").unwrap_or_default(),
                "status": r.try_get::<String,_>("status").unwrap_or_default(),
                "created_at": r.try_get::<chrono::DateTime<chrono::Utc>,_>("created_at").ok(),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "payouts": payouts })))
}

pub async fn approve_payout(
    State(state): State<AppState>,
    actor: crate::auth::AuthedMerchantUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !actor.can_approve() {
        return Err(AppError::Forbidden(
            "requires admin or owner to approve".into(),
        ));
    }
    // Maker ≠ checker: the approver must not be the initiator.
    let row =
        sqlx::query("SELECT initiated_by, status FROM payouts WHERE id=$1 AND merchant_id=$2")
            .bind(id)
            .bind(actor.merchant_id)
            .fetch_optional(&state.db.pool)
            .await?
            .ok_or(AppError::PaymentNotFound)?;
    let initiated_by: Uuid = row.try_get("initiated_by")?;
    let status: String = row.try_get("status").unwrap_or_default();
    if status != "pending_approval" {
        return Err(AppError::BadRequest(
            "payout is not pending approval".into(),
        ));
    }
    if initiated_by == actor.user_id {
        return Err(AppError::Forbidden(
            "the approver must be different from the initiator".into(),
        ));
    }
    let new_status = execute_payout(&state.db.pool, id, actor.user_id).await?;
    Ok(Json(serde_json::json!({ "id": id, "status": new_status })))
}

pub async fn reject_payout(
    State(state): State<AppState>,
    actor: crate::auth::AuthedMerchantUser,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !actor.can_approve() {
        return Err(AppError::Forbidden(
            "requires admin or owner to reject".into(),
        ));
    }
    let updated = sqlx::query(
        "UPDATE payouts SET status='rejected', approved_by=$3, updated_at=NOW() \
         WHERE id=$1 AND merchant_id=$2 AND status='pending_approval'",
    )
    .bind(id)
    .bind(actor.merchant_id)
    .bind(actor.user_id)
    .execute(&state.db.pool)
    .await?;
    if updated.rows_affected() == 0 {
        return Err(AppError::BadRequest(
            "payout is not pending approval".into(),
        ));
    }
    Ok(Json(serde_json::json!({ "id": id, "status": "rejected" })))
}

// ── Merchant users (Phase 3a): list + invite, role-gated ───────────────────
#[derive(Deserialize)]
pub struct InviteUserRequest {
    pub email: String,
    pub name: String,
    pub password: String,
    pub role: Option<String>,
}

pub async fn list_users(
    State(state): State<AppState>,
    actor: crate::auth::AuthedMerchantUser,
) -> Result<Json<serde_json::Value>, AppError> {
    if !actor.can_approve() {
        return Err(AppError::Forbidden("requires admin or owner".into()));
    }
    let users = sqlx::query_as::<_, paybank_core::MerchantUser>(
        "SELECT * FROM merchant_users WHERE merchant_id = $1 ORDER BY created_at",
    )
    .bind(actor.merchant_id)
    .fetch_all(&state.db.pool)
    .await?;
    let views: Vec<crate::auth::MerchantUserView> = users.into_iter().map(Into::into).collect();
    Ok(Json(serde_json::json!({ "users": views })))
}

pub async fn invite_user(
    State(state): State<AppState>,
    actor: crate::auth::AuthedMerchantUser,
    Json(req): Json<InviteUserRequest>,
) -> Result<Json<crate::auth::MerchantUserView>, AppError> {
    if !actor.can_approve() {
        return Err(AppError::Forbidden("requires admin or owner".into()));
    }
    let email = req.email.trim();
    if !email.contains('@') {
        return Err(AppError::BadRequest("a valid email is required".into()));
    }
    if req.password.len() < 8 {
        return Err(AppError::BadRequest(
            "password must be at least 8 characters".into(),
        ));
    }
    let role = req.role.as_deref().unwrap_or("member");
    if role == "owner" && actor.role != "owner" {
        return Err(AppError::Forbidden(
            "only an owner can create another owner".into(),
        ));
    }
    let user = crate::auth::create_merchant_user(
        &state.db.pool,
        actor.merchant_id,
        email,
        req.name.trim(),
        &req.password,
        role,
    )
    .await?;
    Ok(Json(user.into()))
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

    // Modern Treasury requires a due date; default to 30 days out if the
    // merchant didn't pick one.
    let due_str = req
        .due_date
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            (Utc::now() + Duration::days(30))
                .format("%Y-%m-%d")
                .to_string()
        });

    let created = paybank_payments::create_invoice(
        name,
        email,
        "USD",
        Some(&due_str),
        req.description.as_deref(),
        &items,
    )
    .await?;

    let id = Uuid::new_v4();
    let due = chrono::NaiveDate::parse_from_str(&due_str, "%Y-%m-%d").ok();

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

// ── Recurring invoices (Phase 5) ────────────────────────────────────────────
#[derive(Deserialize)]
pub struct CreateRecurringRequest {
    pub customer_name: String,
    pub customer_email: String,
    pub amount: i64,
    pub description: Option<String>,
    pub interval_days: Option<i32>,
    pub start_date: Option<String>,
}

pub async fn create_recurring(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Json(req): Json<CreateRecurringRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    if !req.customer_email.contains('@') {
        return Err(AppError::BadRequest(
            "a valid customer email is required".into(),
        ));
    }
    if req.amount < 1 {
        return Err(AppError::BadRequest("amount must be positive".into()));
    }
    let interval = req.interval_days.unwrap_or(30).max(1);
    let start = req
        .start_date
        .as_deref()
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| Utc::now().date_naive());
    let id = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO recurring_invoices (id, merchant_id, customer_name, customer_email, \
         amount_cents, description, interval_days, next_run) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(id)
    .bind(merchant_id)
    .bind(req.customer_name.trim())
    .bind(req.customer_email.trim())
    .bind(req.amount)
    .bind(&req.description)
    .bind(interval)
    .bind(start)
    .execute(&state.db.pool)
    .await?;
    Ok(Json(serde_json::json!({
        "id": id, "interval_days": interval, "next_run": start.to_string(), "active": true,
    })))
}

pub async fn list_recurring(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
) -> Result<Json<serde_json::Value>, AppError> {
    let rows = sqlx::query(
        "SELECT id, customer_name, customer_email, amount_cents, interval_days, next_run, active \
         FROM recurring_invoices WHERE merchant_id=$1 ORDER BY created_at DESC LIMIT 100",
    )
    .bind(merchant_id)
    .fetch_all(&state.db.pool)
    .await?;
    let items: Vec<serde_json::Value> = rows
        .iter()
        .map(|r| {
            serde_json::json!({
                "id": r.try_get::<Uuid,_>("id").ok(),
                "customer_name": r.try_get::<String,_>("customer_name").unwrap_or_default(),
                "customer_email": r.try_get::<String,_>("customer_email").unwrap_or_default(),
                "amount": r.try_get::<i64,_>("amount_cents").unwrap_or(0),
                "interval_days": r.try_get::<i32,_>("interval_days").unwrap_or(30),
                "next_run": r.try_get::<chrono::NaiveDate,_>("next_run").map(|d| d.to_string()).ok(),
                "active": r.try_get::<bool,_>("active").unwrap_or(false),
            })
        })
        .collect();
    Ok(Json(serde_json::json!({ "recurring": items })))
}

pub async fn cancel_recurring(
    State(state): State<AppState>,
    Extension(AuthedMerchant(merchant_id)): Extension<AuthedMerchant>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let res = sqlx::query(
        "UPDATE recurring_invoices SET active=false, updated_at=NOW() WHERE id=$1 AND merchant_id=$2",
    )
    .bind(id)
    .bind(merchant_id)
    .execute(&state.db.pool)
    .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::PaymentNotFound);
    }
    Ok(Json(serde_json::json!({ "id": id, "active": false })))
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
