use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use hex;
use hmac::{Hmac, Mac};
use paybank_core::{AppError, SessionStatus};
use paybank_db::session_repo;
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

/// Constant-time HMAC-SHA256 verification of a raw webhook body against a
/// hex-encoded signature header. Returns true iff the signature is valid.
fn verify_hmac(secret: &str, body: &[u8], signature_hex: &str) -> bool {
    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body);
    match hex::decode(signature_hex) {
        // `verify_slice` is constant-time.
        Ok(sig) => mac.verify_slice(&sig).is_ok(),
        Err(_) => false,
    }
}

/// Map a provider's transfer status to a session status. Failures get their own
/// distinct terminals — never `Expired` (which means "unused link").
fn map_status(status: &str) -> Option<SessionStatus> {
    match status {
        "settled" | "completed" => Some(SessionStatus::Completed),
        "failed" => Some(SessionStatus::Failed),
        "returned" => Some(SessionStatus::Returned),
        "reversed" => Some(SessionStatus::Reversed),
        "pending" | "processing" | "submitted" => Some(SessionStatus::Processing),
        _ => None,
    }
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(serde_json::json!({"error": "invalid signature"})),
    )
        .into_response()
}

/// Apply a provider-driven status change with CAS guards (so a replayed webhook
/// can't move a terminal session backward) and, on a genuine return/reversal,
/// post the compensating ledger entry exactly once.
async fn apply_webhook_status(
    state: &AppState,
    session_id: Uuid,
    new_status: SessionStatus,
    transfer_id: &str,
) -> Result<(), AppError> {
    use SessionStatus::*;
    let pool = &state.db.pool;
    let tref = (!transfer_id.is_empty()).then_some(transfer_id);
    let changed = match new_status {
        Completed => {
            session_repo::try_transition(pool, session_id, &[Pending, Authorized, Processing], &Completed, tref).await?
        }
        Failed => {
            session_repo::try_transition(pool, session_id, &[Pending, Authorized, Processing], &Failed, tref).await?
        }
        Processing => {
            session_repo::try_transition(pool, session_id, &[Pending, Authorized], &Processing, tref).await?
        }
        Returned => {
            session_repo::try_transition(pool, session_id, &[Completed, Processing], &Returned, tref).await?
        }
        Reversed => {
            session_repo::try_transition(pool, session_id, &[Completed, Processing], &Reversed, tref).await?
        }
        _ => false,
    };

    if changed {
        if let Some(s) = session_repo::get_session(pool, session_id).await? {
            // Notify the merchant (outbox; the worker delivers it).
            let evt = match &new_status {
                Completed => "session.completed",
                Failed => "session.failed",
                Returned => "session.returned",
                Reversed => "session.reversed",
                Processing => "session.processing",
                _ => "session.updated",
            };
            let _ = paybank_db::webhook_repo::enqueue(
                pool,
                s.merchant_id,
                Some(session_id),
                evt,
                serde_json::json!({
                    "session_id": session_id,
                    "status": new_status,
                    "amount_cents": s.amount_cents,
                }),
            )
            .await;

            // Compensating ledger reversal on a real return/reversal.
            if matches!(new_status, Returned | Reversed) {
                if let Err(e) = paybank_db::ledger_repo::LedgerRepo::record_reversal(
                    pool,
                    s.merchant_id,
                    session_id,
                    s.amount_cents,
                )
                .await
                {
                    tracing::error!(
                        session_id = %session_id,
                        error = %e,
                        "session reversed but ledger reversal failed — needs reconciliation"
                    );
                }
            }
        }
    }
    Ok(())
}

pub async fn column_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let secret = std::env::var("WEBHOOK_SECRET")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("WEBHOOK_SECRET missing")))?;

    let signature = headers
        .get("column-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    if !verify_hmac(&secret, &body_bytes, signature) {
        return Ok(unauthorized());
    }

    let payload: Value = serde_json::from_slice(&body_bytes)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid JSON")))?;

    let transfer_id = payload["data"]["id"].as_str().unwrap_or("");
    let status = payload["data"]["status"].as_str().unwrap_or("unknown");

    if let Some(session_id) = payload["data"]["description"]
        .as_str()
        .and_then(|d| d.strip_prefix("Session "))
        .and_then(|s| Uuid::parse_str(s).ok())
    {
        if let Some(ns) = map_status(status) {
            apply_webhook_status(&state, session_id, ns, transfer_id).await?;
        }
    }

    Ok(Json(serde_json::json!({"received": true})).into_response())
}

pub async fn moderntreasury_webhook(
    State(state): State<AppState>,
    headers: HeaderMap,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // P0 fix: the primary provider's webhook was previously UNAUTHENTICATED,
    // letting anyone forge "Session <uuid> completed". Verify the HMAC signature
    // over the raw body before trusting anything in it.
    let secret = std::env::var("MODERN_TREASURY_WEBHOOK_SECRET")
        .or_else(|_| std::env::var("WEBHOOK_SECRET"))
        .map_err(|_| AppError::Internal(anyhow::anyhow!("MT webhook secret missing")))?;

    let signature = headers
        .get("x-signature")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;

    if !verify_hmac(&secret, &body_bytes, signature) {
        return Ok(unauthorized());
    }

    let payload: Value = serde_json::from_slice(&body_bytes)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid JSON")))?;

    let event_type = payload["event_type"].as_str().unwrap_or("");
    let resource = &payload["resource"];
    let transfer_id = resource["id"].as_str().unwrap_or("");
    let status = resource["status"].as_str().unwrap_or("");

    tracing::info!(
        "MT webhook: event={} status={} transfer={}",
        event_type,
        status,
        transfer_id
    );

    let session_id = resource["remittance_information"]
        .as_str()
        .and_then(|d| d.strip_prefix("Session "))
        .and_then(|s| Uuid::parse_str(s).ok());

    let new_status = map_status(status).or_else(|| {
        if event_type == "payment_order.settled" {
            Some(SessionStatus::Completed)
        } else {
            None
        }
    });

    if let (Some(sid), Some(ns)) = (session_id, new_status) {
        apply_webhook_status(&state, sid, ns.clone(), transfer_id).await?;
        tracing::info!("Updated session {} to {:?}", sid, ns);
        // NOTE: the previous bogus transactions insert (merchant_id=nil, amount=0,
        // rail=FedNow) was removed — settlement transactions/ledger postings are
        // written on the authoritative initiate/settle path, not fabricated here.
    }

    Ok(Json(serde_json::json!({"received": true})).into_response())
}
