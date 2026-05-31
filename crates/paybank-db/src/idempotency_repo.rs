use anyhow::Result;
use chrono::{DateTime, Utc};
use serde_json::Value;
use paybank_core::IdempotencyKey;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

pub struct IdempotencyRepo;

impl IdempotencyRepo {
    pub async fn try_acquire_key<'a>(
        tx: &mut Transaction<'a, Postgres>,
        merchant_id: Uuid,
        idempotency_key: &str,
        request_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<Result<(), IdempotencyKey>, sqlx::Error> {
        let result = sqlx::query_as!(
            IdempotencyKey,
            r#"
            INSERT INTO idempotency_keys (idempotency_key, merchant_id, request_hash, state, expires_at)
            VALUES ($1, $2, $3, 'in_progress', $4)
            ON CONFLICT (merchant_id, idempotency_key) DO NOTHING
            RETURNING idempotency_key, merchant_id, request_hash, response_status, response_body as "response_body: serde_json::Value", state, created_at, expires_at
            "#,
            idempotency_key,
            merchant_id,
            request_hash,
            expires_at
        )
        .fetch_optional(&mut **tx)
        .await?;

        if result.is_some() {
            return Ok(Ok(()));
        }

        let existing = sqlx::query_as!(
            IdempotencyKey,
            r#"
            SELECT idempotency_key, merchant_id, request_hash, response_status, response_body as "response_body: serde_json::Value", state, created_at, expires_at
            FROM idempotency_keys
            WHERE merchant_id = $1 AND idempotency_key = $2
            "#,
            merchant_id,
            idempotency_key
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(Err(existing))
    }

    pub async fn complete_key<'a>(
        tx: &mut Transaction<'a, Postgres>,
        merchant_id: Uuid,
        idempotency_key: &str,
        response_status: i32,
        response_body: &Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"UPDATE idempotency_keys
               SET state = 'completed', response_status = $3, response_body = $4
               WHERE merchant_id = $1 AND idempotency_key = $2"#,
            merchant_id,
            idempotency_key,
            response_status,
            response_body
        )
        .execute(&mut **tx)
        .await?;
        Ok(())
    }

    /// Delete idempotency keys past their TTL. Keys exist only to dedupe retries
    /// within a short window; once expired they are dead weight, and the
    /// (merchant_id, idempotency_key) primary key means a never-pruned table grows
    /// without bound. Returns the number of rows reaped. Runs on a background tick.
    pub async fn reap_expired(pool: &PgPool) -> Result<u64, sqlx::Error> {
        let result = sqlx::query!("DELETE FROM idempotency_keys WHERE expires_at < NOW()")
            .execute(pool)
            .await?;
        Ok(result.rows_affected())
    }
}