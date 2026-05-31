//! Outbound merchant-webhook outbox. All runtime queries (no compile-time
//! macros) so this needs no offline sqlx-cache regeneration.

use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct DueWebhook {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub event_type: String,
    pub payload: Value,
    pub attempts: i32,
}

/// Enqueue an event for delivery (status='pending', due immediately).
pub async fn enqueue(
    pool: &PgPool,
    merchant_id: Uuid,
    session_id: Option<Uuid>,
    event_type: &str,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO webhook_events
               (merchant_id, session_id, event_type, payload, status, attempts, next_retry_at)
           VALUES ($1, $2, $3, $4, 'pending', 0, NOW())"#,
    )
    .bind(merchant_id)
    .bind(session_id)
    .bind(event_type)
    .bind(payload)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically claim up to `limit` due events: marks them 'sending' and bumps
/// attempts in one statement (FOR UPDATE SKIP LOCKED) so multiple worker ticks /
/// instances never grab the same row, and we never hold a row lock across the
/// outbound HTTP call.
pub async fn claim_due(pool: &PgPool, limit: i64) -> Result<Vec<DueWebhook>, sqlx::Error> {
    // A 60s lease (next_retry_at bumped on claim) doubles as a reaper: 'sending'
    // rows whose worker crashed mid-delivery become claimable again once the
    // lease expires, so a crash can't strand an event forever (honest
    // at-least-once). mark_delivered/mark_failed override the lease on completion.
    let rows = sqlx::query_as::<_, (Uuid, Uuid, String, Value, i32)>(
        r#"UPDATE webhook_events
           SET status = 'sending', attempts = attempts + 1, next_retry_at = NOW() + interval '60 seconds'
           WHERE id IN (
               SELECT id FROM webhook_events
               WHERE status IN ('pending', 'failed', 'sending')
                 AND (next_retry_at IS NULL OR next_retry_at <= NOW())
               ORDER BY next_retry_at NULLS FIRST
               LIMIT $1
               FOR UPDATE SKIP LOCKED
           )
           RETURNING id, merchant_id, event_type, payload, attempts"#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|(id, merchant_id, event_type, payload, attempts)| DueWebhook {
            id,
            merchant_id,
            event_type,
            payload,
            attempts,
        })
        .collect())
}

pub async fn mark_delivered(pool: &PgPool, id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE webhook_events SET status = 'delivered', sent_at = NOW() WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Reschedule a failed delivery; once `max_attempts` is exceeded it goes to the
/// terminal 'exhausted' state (no longer retried) so the queue can't loop forever.
pub async fn mark_failed(
    pool: &PgPool,
    id: Uuid,
    attempts: i32,
    max_attempts: i32,
    backoff_secs: i64,
) -> Result<(), sqlx::Error> {
    if attempts >= max_attempts {
        sqlx::query("UPDATE webhook_events SET status = 'exhausted' WHERE id = $1")
            .bind(id)
            .execute(pool)
            .await?;
    } else {
        sqlx::query(
            "UPDATE webhook_events SET status = 'failed', next_retry_at = NOW() + ($2 || ' seconds')::interval WHERE id = $1",
        )
        .bind(id)
        .bind(backoff_secs.to_string())
        .execute(pool)
        .await?;
    }
    Ok(())
}

pub async fn log_attempt(
    pool: &PgPool,
    event_id: Uuid,
    attempt_number: i32,
    response_status: Option<i32>,
    error_message: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO webhook_delivery_logs
               (webhook_event_id, attempt_number, response_status, error_message, delivered_at)
           VALUES ($1, $2, $3, $4, CASE WHEN $3 BETWEEN 200 AND 299 THEN NOW() ELSE NULL END)"#,
    )
    .bind(event_id)
    .bind(attempt_number)
    .bind(response_status)
    .bind(error_message)
    .execute(pool)
    .await?;
    Ok(())
}

/// Admin "retry": make a failed/exhausted event due again now.
pub async fn retry_now(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let n = sqlx::query(
        "UPDATE webhook_events SET status = 'pending', next_retry_at = NOW() WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?
    .rows_affected();
    Ok(n == 1)
}

/// (webhook_url, webhook_secret) for a merchant, if configured.
pub async fn merchant_endpoint(
    pool: &PgPool,
    merchant_id: Uuid,
) -> Result<Option<(Option<String>, Option<String>)>, sqlx::Error> {
    let row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT webhook_url, webhook_secret FROM merchants WHERE id = $1",
    )
    .bind(merchant_id)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}
