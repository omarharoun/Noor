//! Reconciliation poller for sessions stuck in `processing`.
//!
//! Ambiguous transfer outcomes (provider timeout / 5xx) intentionally leave a
//! session `Processing` rather than lying that it `Failed`. This loop resolves
//! them: where we have a provider transfer reference it polls the provider and
//! settles/reverses accordingly; sessions with no reference are logged for
//! manual reconciliation (we never guess at an unknown money outcome).

use std::time::Duration;

use crate::state::AppState;
use paybank_core::SessionStatus;
use paybank_db::{ledger_repo::LedgerRepo, session_repo};

const TICK_SECS: u64 = 60;
const STUCK_THRESHOLD_SECS: i64 = 300;
const BATCH: i64 = 50;

/// String key for the active primary provider, matching `check_transfer_status`.
fn active_provider() -> &'static str {
    match paybank_payments::PaymentProvider::primary() {
        paybank_payments::PaymentProvider::ModernTreasury => "modern_treasury",
        paybank_payments::PaymentProvider::Column => "column",
    }
}

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&state).await {
                tracing::error!(error = %e, "reconciliation tick failed");
            }
            tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        }
    });
}

fn map_provider_status(s: &str) -> Option<SessionStatus> {
    match s {
        "settled" | "completed" => Some(SessionStatus::Completed),
        "failed" | "cancelled" => Some(SessionStatus::Failed),
        "returned" => Some(SessionStatus::Returned),
        "reversed" => Some(SessionStatus::Reversed),
        _ => None, // still in flight — leave Processing
    }
}

async fn tick(state: &AppState) -> Result<(), sqlx::Error> {
    let stuck = session_repo::list_stuck_processing(&state.db.pool, STUCK_THRESHOLD_SECS, BATCH)
        .await
        .map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    if stuck.is_empty() {
        return Ok(());
    }
    tracing::info!(count = stuck.len(), "reconciling stuck processing sessions");

    for (id, merchant_id, amount_cents, provider_ref) in stuck {
        let Some(tref) = provider_ref.filter(|r| !r.is_empty()) else {
            tracing::warn!(session_id = %id, "stuck processing with no provider reference — manual reconciliation required");
            continue;
        };

        match paybank_payments::check_transfer_status(active_provider(), &tref).await {
            Ok(status) => {
                let Some(resolved) = map_provider_status(&status) else {
                    continue; // genuinely still pending at the provider
                };
                let pool = &state.db.pool;
                match resolved {
                    SessionStatus::Completed => {
                        if session_repo::try_transition(pool, id, &[SessionStatus::Processing], &SessionStatus::Completed, Some(&tref)).await.unwrap_or(false) {
                            let _ = LedgerRepo::record_settlement(pool, merchant_id, id, amount_cents).await;
                            tracing::info!(session_id = %id, "reconciled -> completed");
                        }
                    }
                    SessionStatus::Returned | SessionStatus::Reversed => {
                        if session_repo::try_transition(pool, id, &[SessionStatus::Processing], &resolved, Some(&tref)).await.unwrap_or(false) {
                            let _ = LedgerRepo::record_reversal(pool, merchant_id, id, amount_cents).await;
                            tracing::info!(session_id = %id, ?resolved, "reconciled -> reversed/returned");
                        }
                    }
                    SessionStatus::Failed => {
                        let _ = session_repo::try_transition(pool, id, &[SessionStatus::Processing], &SessionStatus::Failed, None).await;
                        tracing::info!(session_id = %id, "reconciled -> failed");
                    }
                    _ => {}
                }
            }
            Err(e) => {
                tracing::warn!(session_id = %id, error = %e, "provider status check failed; will retry next tick");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settled_and_completed_map_to_completed() {
        assert_eq!(map_provider_status("settled"), Some(SessionStatus::Completed));
        assert_eq!(map_provider_status("completed"), Some(SessionStatus::Completed));
    }

    #[test]
    fn failed_and_cancelled_map_to_failed() {
        assert_eq!(map_provider_status("failed"), Some(SessionStatus::Failed));
        assert_eq!(map_provider_status("cancelled"), Some(SessionStatus::Failed));
    }

    #[test]
    fn returned_maps_to_returned() {
        assert_eq!(map_provider_status("returned"), Some(SessionStatus::Returned));
    }

    #[test]
    fn reversed_maps_to_reversed() {
        assert_eq!(map_provider_status("reversed"), Some(SessionStatus::Reversed));
    }

    #[test]
    fn in_flight_statuses_map_to_none() {
        // Genuinely pending at the provider — must leave the session Processing.
        assert_eq!(map_provider_status("pending"), None);
        assert_eq!(map_provider_status("processing"), None);
    }

    #[test]
    fn unknown_and_empty_statuses_map_to_none() {
        // Never guess at an unknown money outcome.
        assert_eq!(map_provider_status(""), None);
        assert_eq!(map_provider_status("garbage"), None);
    }

    #[test]
    fn matching_is_case_sensitive_and_exact() {
        // Provider strings are compared verbatim; case/whitespace variants are not
        // recognized terminals and must remain in flight (None).
        assert_eq!(map_provider_status("Settled"), None);
        assert_eq!(map_provider_status("COMPLETED"), None);
        assert_eq!(map_provider_status(" settled"), None);
        assert_eq!(map_provider_status("settled "), None);
    }
}
