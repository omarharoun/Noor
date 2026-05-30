use crate::state::AppState;
use axum::{
    extract::{Path, Query, State},
    Json,
};
use paybank_core::{AppError, CreateSessionRequest, CreateSessionResponse, PaymentRail, SessionStatus};
use paybank_db::{bank_repo, session_repo};
use serde::Deserialize;
use uuid::Uuid;
use chrono::{Duration, Utc};

pub async fn create_session(
    State(state): State<AppState>,
    Json(req): Json<CreateSessionRequest>,
) -> Result<Json<CreateSessionResponse>, AppError> {
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

    let public_url = std::env::var("PUBLIC_APP_URL").unwrap_or_else(|_| "http://localhost:3000".into());
    let pay_url = format!("{}/pay/{}", public_url, id);

    let qr_data = generate_qr_data(&pay_url);

    Ok(Json(CreateSessionResponse {
        id: session.id,
        status: paybank_core::PaymentStatus::Created,
        session_status: session.status,
        amount_cents: session.amount_cents,
        currency: session.currency,
        expires_at: session.expires_at,
        qr_data: Some(qr_data),
    }))
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

pub async fn get_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<paybank_core::PaymentSession>, AppError> {
    let session = session_repo::get_session(&state.db.pool, id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    Ok(Json(session))
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
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = session_repo::get_session(&state.db.pool, id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    if session.status == SessionStatus::Expired || Utc::now() > session.expires_at {
        return Err(AppError::SessionExpired);
    }

    let rail = session.rail_used.as_ref().ok_or(AppError::SessionNotFound)?;
    let raw = session.column_counterparty_id
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("No counterparty linked")))?;
    let ext_account_id = raw.split('|').nth(1).unwrap_or(&raw);
    let actual_recipient_id = if std::env::var("MODERN_TREASURY_API_KEY").is_ok() {
        ext_account_id
    } else {
        raw.split('|').next().unwrap_or(&raw)
    };

    session_repo::update_status(&state.db.pool, id, &SessionStatus::Processing, None).await?;

    let result = paybank_payments::initiate_transfer(
        id,
        &actual_recipient_id,
        rail,
        session.amount_cents,
        &format!("Session {}", id),
    )
    .await;

    match result {
        Ok(transfer) => {
            session_repo::update_status(
                &state.db.pool,
                id,
                &SessionStatus::Completed,
                Some(&transfer.provider_transfer_id),
            )
            .await?;

            let txn_id = Uuid::new_v4();
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

            Ok(Json(serde_json::json!({
                "status": "completed",
                "transaction_id": txn_id,
            })))
        }
        Err(e) => {
            session_repo::update_status(&state.db.pool, id, &SessionStatus::Expired, None).await?;
            Err(e)
        }
    }
}

fn generate_qr_data(pay_url: &str) -> String {
    let prefix = "https://pay.paybank.com/p/";
    if pay_url.starts_with(prefix) {
        pay_url.to_string()
    } else {
        pay_url.to_string()
    }
}