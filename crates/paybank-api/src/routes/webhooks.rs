use crate::state::AppState;
use axum::{
    body::Bytes,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::IntoResponse,
    Json,
};
use hex;
use hmac::{Hmac, Mac};
use paybank_core::{AppError, SessionStatus};
use paybank_db::session_repo;
use serde_json::Value;
use sha2::Sha256;
use uuid::Uuid;

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
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("Missing signature")))?;

    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid HMAC key")))?;
    mac.update(&body_bytes);

    let expected = hex::encode(mac.finalize().into_bytes());
    if signature != expected {
        return Ok((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid signature"})),
        )
            .into_response());
    }

    let payload: Value = serde_json::from_slice(&body_bytes)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid JSON")))?;

    let transfer_id = payload["data"]["id"].as_str().unwrap_or("");
    let status = payload["data"]["status"].as_str().unwrap_or("unknown");

    if let Some(session_ref) = payload["data"]["description"]
        .as_str()
        .and_then(|d| d.strip_prefix("Session "))
    {
        if let Ok(session_id) = Uuid::parse_str(session_ref) {
            match status {
                "settled" | "completed" => {
                    session_repo::update_status(
                        &state.db.pool,
                        session_id,
                        &SessionStatus::Completed,
                        Some(transfer_id),
                    )
                    .await?;
                }
                "failed" | "returned" => {
                    session_repo::update_status(
                        &state.db.pool,
                        session_id,
                        &SessionStatus::Expired,
                        Some(transfer_id),
                    )
                    .await?;
                }
                _ => {
                    session_repo::update_status(
                        &state.db.pool,
                        session_id,
                        &SessionStatus::Processing,
                        Some(transfer_id),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(Json(serde_json::json!({"received": true})).into_response())
}

pub async fn moderntreasury_webhook(
    State(state): State<AppState>,
    body_bytes: Bytes,
) -> Result<impl IntoResponse, AppError> {
    let payload: Value = serde_json::from_slice(&body_bytes)
        .map_err(|_| AppError::Internal(anyhow::anyhow!("Invalid JSON")))?;

    let event_type = payload["event_type"].as_str().unwrap_or("");
    let resource = &payload["resource"];
    let transfer_id = resource["id"].as_str().unwrap_or("");
    let status = resource["status"].as_str().unwrap_or("");

    tracing::info!("MT webhook: event={} status={} transfer={}", event_type, status, transfer_id);

    let session_id = resource["remittance_information"]
        .as_str()
        .and_then(|d| d.strip_prefix("Session "))
        .and_then(|s| Uuid::parse_str(s).ok());

    let new_status = match status {
        "settled" | "completed" => Some(SessionStatus::Completed),
        "failed" | "returned" | "reversed" => Some(SessionStatus::Expired),
        _ if event_type == "payment_order.settled" => Some(SessionStatus::Completed),
        _ => None,
    };

    if let (Some(sid), Some(ns)) = (session_id, new_status) {
        session_repo::update_status(&state.db.pool, sid, &ns, Some(transfer_id)).await?;
        tracing::info!("Updated session {} to {:?}", sid, ns);
    }

    if !transfer_id.is_empty() {
        if let Some(sid) = session_id {
            let _ = paybank_db::transaction_repo::create_transaction(
                &state.db.pool,
                Uuid::new_v4(),
                sid,
                Uuid::nil(),
                0,
                &paybank_core::PaymentRail::FedNow,
                Some(transfer_id),
                &format!("mt_{}", transfer_id),
                chrono::Utc::now(),
            ).await;
        }
    }

    Ok(Json(serde_json::json!({"received": true})).into_response())
}
