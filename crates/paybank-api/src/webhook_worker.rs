//! Background outbound-webhook delivery worker.
//!
//! Polls the `webhook_events` outbox, claims due events (SKIP LOCKED), POSTs an
//! HMAC-signed payload to the merchant's `webhook_url`, logs the attempt, and
//! reschedules with exponential backoff (terminal `exhausted` after the cap).
//! At-least-once delivery; merchants must dedupe on the event id.

use hmac::{Hmac, Mac};
use sha2::Sha256;
use std::time::Duration;

use crate::state::AppState;
use paybank_db::webhook_repo::{self, DueWebhook};

const TICK_SECS: u64 = 5;
const BATCH: i64 = 20;
const MAX_ATTEMPTS: i32 = 6;
const DELIVERY_TIMEOUT_SECS: u64 = 10;

fn sign(secret: &str, body: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).expect("hmac key");
    mac.update(body);
    hex::encode(mac.finalize().into_bytes())
}

/// Exponential backoff: 30s, 60s, 120s, ... capped at 1h.
fn backoff_secs(attempts: i32) -> i64 {
    let base: i64 = 30;
    base.saturating_mul(1i64 << attempts.min(7)).min(3600)
}

/// Spawn the delivery loop. Call once at startup.
pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(DELIVERY_TIMEOUT_SECS))
            .build()
            .unwrap_or_default();
        loop {
            if let Err(e) = tick(&state, &client).await {
                tracing::error!(error = %e, "webhook worker tick failed");
            }
            tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        }
    });
}

async fn tick(state: &AppState, client: &reqwest::Client) -> Result<(), sqlx::Error> {
    let due = webhook_repo::claim_due(&state.db.pool, BATCH).await?;
    for ev in due {
        deliver(state, client, ev).await;
    }
    Ok(())
}

async fn deliver(state: &AppState, client: &reqwest::Client, ev: DueWebhook) {
    let pool = &state.db.pool;
    let endpoint = match webhook_repo::merchant_endpoint(pool, ev.merchant_id).await {
        Ok(e) => e,
        Err(e) => {
            tracing::error!(event_id = %ev.id, error = %e, "lookup merchant endpoint failed");
            let _ = webhook_repo::mark_failed(
                pool,
                ev.id,
                ev.attempts,
                MAX_ATTEMPTS,
                backoff_secs(ev.attempts),
            )
            .await;
            return;
        }
    };

    // No URL configured → nothing to deliver; consider it done (not an error).
    let url = match endpoint.and_then(|(u, s)| u.map(|u| (u, s))) {
        Some((u, s)) if !u.is_empty() => (u, s),
        _ => {
            let _ = webhook_repo::mark_delivered(pool, ev.id).await;
            return;
        }
    };
    let (webhook_url, secret_opt) = url;

    let body = serde_json::json!({
        "id": ev.id,
        "type": ev.event_type,
        "data": ev.payload,
    });
    let raw = serde_json::to_vec(&body).unwrap_or_default();

    // Sign with the merchant's secret, falling back to the global one.
    let secret = secret_opt
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("WEBHOOK_SECRET").ok())
        .unwrap_or_default();
    let signature = sign(&secret, &raw);

    let resp = client
        .post(&webhook_url)
        .header("content-type", "application/json")
        .header("x-noor-signature", signature)
        .header("x-noor-event-id", ev.id.to_string())
        .body(raw)
        .send()
        .await;

    match resp {
        Ok(r) => {
            let code = r.status().as_u16() as i32;
            let _ = webhook_repo::log_attempt(pool, ev.id, ev.attempts, Some(code), None).await;
            if r.status().is_success() {
                let _ = webhook_repo::mark_delivered(pool, ev.id).await;
            } else {
                let _ = webhook_repo::mark_failed(
                    pool,
                    ev.id,
                    ev.attempts,
                    MAX_ATTEMPTS,
                    backoff_secs(ev.attempts),
                )
                .await;
            }
        }
        Err(e) => {
            let _ = webhook_repo::log_attempt(pool, ev.id, ev.attempts, None, Some(&e.to_string()))
                .await;
            let _ = webhook_repo::mark_failed(
                pool,
                ev.id,
                ev.attempts,
                MAX_ATTEMPTS,
                backoff_secs(ev.attempts),
            )
            .await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_value_is_thirty() {
        // attempt 0 => base * (1<<0) = 30
        assert_eq!(backoff_secs(0), 30);
    }

    #[test]
    fn known_early_doublings() {
        // 30, 60, 120, 240, 480, 960, 1920 — pure doubling before the cap bites.
        assert_eq!(backoff_secs(1), 60);
        assert_eq!(backoff_secs(2), 120);
        assert_eq!(backoff_secs(3), 240);
        assert_eq!(backoff_secs(4), 480);
        assert_eq!(backoff_secs(5), 960);
        assert_eq!(backoff_secs(6), 1920);
    }

    #[test]
    fn monotonically_non_decreasing_for_0_through_7() {
        // Each step must be >= the previous one across attempts 0..=7 (the range 0..8).
        let mut prev = backoff_secs(0);
        for attempt in 1..8 {
            let cur = backoff_secs(attempt);
            assert!(
                cur >= prev,
                "backoff decreased: attempt {} gave {} which is < previous {}",
                attempt,
                cur,
                prev
            );
            prev = cur;
        }
    }

    #[test]
    fn capped_at_3600() {
        // attempt 7 => 30 * 128 = 3840, which must be clamped to the 1h cap.
        assert_eq!(backoff_secs(7), 3600);
        // Larger attempt counts must also stay at the cap, never exceeding it.
        for attempt in [8, 10, 31, 62, 1000, i32::MAX] {
            assert_eq!(
                backoff_secs(attempt),
                3600,
                "attempt {} should be capped at 3600",
                attempt
            );
        }
    }

    #[test]
    fn no_overflow_panic_at_max_attempts() {
        // The min(7) on the shift and saturating_mul guard against UB/overflow even at i32::MAX.
        let v = backoff_secs(i32::MAX);
        assert!(v > 0 && v <= 3600);
    }

    #[test]
    fn never_exceeds_cap_anywhere_in_range() {
        for attempt in 0..8 {
            let v = backoff_secs(attempt);
            assert!(v >= 30, "attempt {} below minimum 30: {}", attempt, v);
            assert!(v <= 3600, "attempt {} above cap 3600: {}", attempt, v);
        }
    }
}
