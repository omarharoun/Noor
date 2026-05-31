use crate::auth::AuthedOperator;
use crate::state::AppState;
use axum::{extract::State, Extension, Json};
use paybank_core::{AppError, SessionStatus};
use paybank_db::{audit_repo, session_repo};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct InitiateRequest {
    pub session_id: Uuid,
}

pub async fn initiate_session(
    State(state): State<AppState>,
    Extension(AuthedOperator(claims)): Extension<AuthedOperator>,
    Json(req): Json<InitiateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let session = session_repo::get_session(&state.db.pool, req.session_id)
        .await?
        .ok_or(AppError::SessionNotFound)?;

    if session.status == SessionStatus::Expired || chrono::Utc::now() > session.expires_at {
        return Err(AppError::SessionExpired);
    }
    if session.status == SessionStatus::Completed {
        return Ok(Json(serde_json::json!({ "status": "completed" })));
    }

    let rail = session
        .rail_used
        .as_ref()
        .ok_or(AppError::SessionNotFound)?;
    let raw = session.column_counterparty_id.ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!(
            "No counterparty linked. Use /confirm instead."
        ))
    })?;
    let counterparty_id = raw.split('|').next().unwrap_or(&raw);
    let ext_account_id = raw.split('|').nth(1).unwrap_or(counterparty_id);

    let actual_recipient_id = if std::env::var("MODERN_TREASURY_API_KEY").is_ok() {
        ext_account_id
    } else {
        counterparty_id
    };

    // P0: atomic claim — only an `authorized` session transitions to `processing`,
    // and only one caller wins, preventing duplicate transfers on retry/double-submit.
    let claimed = session_repo::try_transition(
        &state.db.pool,
        req.session_id,
        &[SessionStatus::Authorized],
        &SessionStatus::Processing,
        None,
    )
    .await?;
    if !claimed {
        return Err(AppError::InvalidStateTransition);
    }

    let result = paybank_payments::initiate_transfer(
        req.session_id,
        actual_recipient_id,
        rail,
        session.amount_cents,
        &format!("Session {}", req.session_id),
    )
    .await;

    match result {
        Ok(transfer) => {
            // Only the caller that wins Processing -> Completed records the
            // settlement, so a webhook/reconcile that already settled this session
            // can't trigger a second transaction/ledger posting (TOCTOU guard).
            let won = session_repo::try_transition(
                &state.db.pool,
                req.session_id,
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
                    req.session_id,
                    session.merchant_id,
                    session.amount_cents,
                    rail,
                    Some(&transfer.provider_transfer_id),
                    &format!("txn_{}", req.session_id),
                    chrono::Utc::now(),
                )
                .await?;

                // Ledger failure must NOT fail the response — the transfer already
                // succeeded; log loudly for reconciliation instead.
                if let Err(e) = paybank_db::ledger_repo::LedgerRepo::record_settlement(
                    &state.db.pool,
                    session.merchant_id,
                    req.session_id,
                    session.amount_cents,
                )
                .await
                {
                    tracing::error!(
                        session_id = %req.session_id,
                        error = %e,
                        "transfer settled but ledger posting failed — needs reconciliation"
                    );
                }

                let _ = paybank_db::webhook_repo::enqueue(
                    &state.db.pool,
                    session.merchant_id,
                    Some(req.session_id),
                    "session.completed",
                    serde_json::json!({ "session_id": req.session_id, "status": "completed", "amount_cents": session.amount_cents }),
                )
                .await;

                audit_repo::record(
                    &state.db.pool,
                    &claims.sub,
                    Some(&claims.role),
                    "session.initiate",
                    Some("session"),
                    Some(&req.session_id.to_string()),
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
            if matches!(e, AppError::ProviderAmbiguous(_)) {
                // Outcome unknown — the transfer may have gone through. Leave the
                // session Processing for a webhook / reconciliation job to settle;
                // marking it Failed here would be a money-state lie.
                tracing::warn!(
                    session_id = %req.session_id,
                    error = %e,
                    "ambiguous transfer outcome; leaving session Processing for reconciliation"
                );
            } else {
                session_repo::try_transition(
                    &state.db.pool,
                    req.session_id,
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