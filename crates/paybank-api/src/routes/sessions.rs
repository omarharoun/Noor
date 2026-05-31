use crate::auth::AuthedOperator;
use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Extension, Json,
};
use paybank_core::{AppError, CreateSessionRequest, CreateSessionResponse, PaymentRail, SessionStatus};
use paybank_db::{audit_repo, bank_repo, session_repo};
use serde::Deserialize;
use uuid::Uuid;
use chrono::{Duration, Utc};

pub async fn create_session(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, AppError> {
    // Optional idempotency: a repeat with the same key+body replays the original
    // session instead of minting a duplicate.
    let idem_key = crate::idempotency::key_from(&headers);
    if let Some(key) = &idem_key {
        let request_hash = crate::idempotency::hash(&req);
        match crate::idempotency::begin(&state.db, req.merchant_id, key, &request_hash).await? {
            crate::idempotency::Outcome::Replay(body) => {
                let resp: CreateSessionResponse =
                    serde_json::from_value(body).map_err(|e| AppError::Internal(e.into()))?;
                return Ok(Json(resp));
            }
            crate::idempotency::Outcome::Proceed => {}
        }
    }

    let bank = bank_repo::get_bank(&state.db.pool, &req.bank_id)
        .await?
        .ok_or(AppError::BankNotFound)?;

    let rail = PaymentRail::choose(
        bank.supports_fednow,
        bank.supports_rtp,
        req.amount_cents,
    );

    let expires_at = Utc::now() + Duration::hours(24);
    let id = Uuid::new_v4();

    let session = session_repo::create_session(
        &state.db.pool,
        id,
        req.merchant_id,
        &req.bank_id,
        req.amount_cents,
        req.note.as_deref(),
        &rail,
        expires_at,
    )
    .await?;

    let pay_url = format!("{}/pay/{}", state.config.public_app_url, id);
    let qr_data = generate_qr_data(&pay_url);

    let response = CreateSessionResponse {
        id: session.id,
        status: paybank_core::PaymentStatus::Created,
        session_status: session.status,
        amount_cents: session.amount_cents,
        currency: session.currency,
        expires_at: session.expires_at,
        qr_data: Some(qr_data),
    };

    if let Some(key) = &idem_key {
        let body = serde_json::to_value(&response).map_err(|e| AppError::Internal(e.into()))?;
        crate::idempotency::complete(&state.db, req.merchant_id, key, &body).await?;
    }

    Ok(Json(response))
}

#[derive(Deserialize)]
pub struct ListSessionsQuery {
    pub merchant_id: Uuid,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    Query(q): Query<ListSessionsQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let limit = q.limit.unwrap_or(50).min(200);
    let offset = q.offset.unwrap_or(0);
    let (sessions, total) = session_repo::list_sessions(&state.db.pool, q.merchant_id, limit, offset).await?;
    Ok(Json(serde_json::json!({
        "sessions": sessions,
        "total": total,
    })))
}

/// Public, capability-scoped session view (anyone with the unguessable UUID can
/// read it — the customer pay page does). P0: this MUST NOT expose the raw bank
/// account or routing number. We return only a masked last-4 and a `bank_linked`
/// flag; full details stay on the authenticated admin detail endpoint.
pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let s = session_repo::get_session(&state.db.pool, id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    let bank_linked = s
        .customer_account_number
        .as_deref()
        .map(|n| !n.is_empty())
        .unwrap_or(false);
    let account_last4 = s
        .customer_account_number
        .as_deref()
        .filter(|n| n.len() >= 4)
        .map(|n| n[n.len() - 4..].to_string());

    Ok(Json(serde_json::json!({
        "id": s.id,
        "merchant_id": s.merchant_id,
        "bank_id": s.bank_id,
        "amount_cents": s.amount_cents,
        "currency": s.currency,
        "note": s.note,
        "status": s.status,
        "rail_used": s.rail_used,
        "column_ref": s.column_ref,
        "column_counterparty_id": s.column_counterparty_id,
        "customer_name": s.customer_name,
        "customer_email": s.customer_email,
        "customer_phone": s.customer_phone,
        "customer_address_line_1": s.customer_address_line_1,
        "customer_address_city": s.customer_address_city,
        "customer_address_state": s.customer_address_state,
        "customer_address_postal_code": s.customer_address_postal_code,
        "customer_address_country_code": s.customer_address_country_code,
        "customer_account_type": s.customer_account_type,
        // Redacted: never expose the full account/routing number publicly.
        "customer_account_last4": account_last4,
        "bank_linked": bank_linked,
        "expires_at": s.expires_at,
        "created_at": s.created_at,
        "updated_at": s.updated_at,
    })))
}

pub async fn get_session_qr(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<String, AppError> {
    let _session = session_repo::get_session(&state.db.pool, id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    let public_url = std::env::var("PUBLIC_APP_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let pay_url = format!("{}/pay/{}", public_url, id);
    let qr = paybank_qr::generate_qr_svg(&pay_url)
        .map_err(|e| AppError::Internal(e))?;

    Ok(qr)
}

pub async fn stream_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    use axum::response::sse::{Event, Sse};
    use std::time::Duration;

    let session = session_repo::get_session(&state.db.pool, id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    let pool = state.db.pool.clone();
    let stream = async_stream::stream! {
        let initial_data = serde_json::json!({
            "session_id": session.id,
            "status": session.status,
            "amount_cents": session.amount_cents,
        });
        yield Ok::<_, AppError>(Event::default().data(initial_data.to_string()));

        loop {
            tokio::time::sleep(Duration::from_secs(2)).await;
            match session_repo::get_session(&pool, id).await {
                Ok(Some(s)) => {
                    let data = serde_json::json!({
                        "session_id": s.id,
                        "status": s.status,
                        "amount_cents": s.amount_cents,
                    });
                    yield Ok::<_, AppError>(Event::default().data(data.to_string()));
                    if s.status == SessionStatus::Completed || s.status == SessionStatus::Expired {
                        break;
                    }
                }
                _ => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(
        axum::response::sse::KeepAlive::new()
            .interval(Duration::from_secs(5))
            .text("keep-alive"),
    ))
}

pub async fn initiate_payment(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = session_repo::get_session(&state.db.pool, id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    if session.status == SessionStatus::Expired || Utc::now() > session.expires_at {
        return Err(AppError::SessionExpired);
    }

    let rail = session.rail_used.as_ref().ok_or(AppError::SessionNotFound)?;
    let raw = session
        .column_counterparty_id
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No counterparty linked")))?;
    let ext_account_id = raw.split('|').nth(1).unwrap_or(&raw);
    let actual_recipient_id = if std::env::var("MODERN_TREASURY_API_KEY").is_ok() {
        ext_account_id
    } else {
        raw.split('|').next().unwrap_or(&raw)
    };

    // P0: atomic compare-and-swap guard. Only an `authorized` session can be
    // moved to `processing`, and only one caller wins — this is what stops two
    // concurrent/duplicate initiate calls from both firing a transfer.
    let claimed =
        session_repo::try_transition(&state.db.pool, id, &[SessionStatus::Authorized], &SessionStatus::Processing, None)
            .await?;
    if !claimed {
        return Err(AppError::InvalidStateTransition);
    }

    let result = paybank_payments::initiate_transfer(
        id,
        actual_recipient_id,
        rail,
        session.amount_cents,
        &format!("Session {}", id),
    )
    .await;

    match result {
        Ok(transfer) => {
            // Only the winner of Processing -> Completed records the settlement,
            // so a concurrent webhook/reconcile can't double-post (TOCTOU guard).
            let won = session_repo::try_transition(
                &state.db.pool,
                id,
                &[SessionStatus::Processing],
                &SessionStatus::Completed,
                Some(&transfer.provider_transfer_id),
            )
            .await?;

            let txn_id = Uuid::new_v4();
            if won {
                paybank_db::transaction_repo::create_transaction(
                    &state.db.pool,
                    txn_id,
                    id,
                    session.merchant_id,
                    session.amount_cents,
                    rail,
                    Some(&transfer.provider_transfer_id),
                    &format!("pay_{}", id),
                    Utc::now(),
                )
                .await?;

                // Ledger failure is logged for reconciliation, never surfaced as a
                // failed payment to the caller (the transfer already succeeded).
                if let Err(e) = paybank_db::ledger_repo::LedgerRepo::record_settlement(
                    &state.db.pool,
                    session.merchant_id,
                    id,
                    session.amount_cents,
                )
                .await
                {
                    tracing::error!(
                        session_id = %id,
                        error = %e,
                        "transfer settled but ledger posting failed — needs reconciliation"
                    );
                }

                let _ = paybank_db::webhook_repo::enqueue(
                    &state.db.pool,
                    session.merchant_id,
                    Some(id),
                    "session.completed",
                    serde_json::json!({ "session_id": id, "status": "completed", "amount_cents": session.amount_cents }),
                )
                .await;

                audit_repo::record(
                    &state.db.pool,
                    &claims.sub,
                    Some(&claims.role),
                    "session.initiate",
                    Some("session"),
                    Some(&id.to_string()),
                    serde_json::json!({ "amount_cents": session.amount_cents, "transfer_id": transfer.provider_transfer_id }),
                )
                .await;
            }

            Ok(Json(serde_json::json!({
                "status": "completed",
                "transaction_id": txn_id,
            })))
        }
        Err(e) => {
            // A definitive provider rejection → Failed (distinct from Expired).
            // Ambiguous errors stay Processing for webhook/reconciliation to resolve.
            if matches!(e, AppError::ProviderAmbiguous(_)) {
                tracing::warn!(
                    session_id = %id,
                    error = %e,
                    "ambiguous transfer outcome; leaving session Processing for reconciliation"
                );
            } else {
                session_repo::try_transition(
                    &state.db.pool,
                    id,
                    &[SessionStatus::Processing],
                    &SessionStatus::Failed,
                    None,
                )
                .await?;
            }
            Err(e)
        }
    }
}

fn generate_qr_data(pay_url: &str) -> String {
    let prefix = "https://pay.noor.com/p/";
    if pay_url.starts_with(prefix) {
        pay_url.to_string()
    } else {
        pay_url.to_string()
    }
}