use anyhow::Result;
use serde_json::Value;
use paybank_core::PaymentAttempt;
use sqlx::PgPool;
use uuid::Uuid;

pub struct AttemptRepo;

impl AttemptRepo {
    pub async fn create_attempt(
        pool: &PgPool,
        session_id: Uuid,
        rail: &str,
        request_payload: Option<Value>,
    ) -> Result<PaymentAttempt, sqlx::Error> {
        let attempt = sqlx::query_as!(
            PaymentAttempt,
            r#"
            INSERT INTO payment_attempts (session_id, rail, request_payload, status)
            VALUES ($1, $2, $3, 'pending')
            RETURNING id, session_id, rail as "rail: paybank_core::PaymentRail", provider_ref, request_payload, response_status, response_body, error_message, status, created_at, updated_at
            "#,
            session_id,
            rail,
            request_payload
        )
        .fetch_one(pool)
        .await?;
        Ok(attempt)
    }

    pub async fn update_attempt(
        pool: &PgPool,
        id: Uuid,
        provider_ref: Option<String>,
        response_status: Option<i32>,
        response_body: Option<Value>,
        error_message: Option<String>,
        status: &str,
    ) -> Result<PaymentAttempt, sqlx::Error> {
        let attempt = sqlx::query_as!(
            PaymentAttempt,
            r#"
            UPDATE payment_attempts
            SET provider_ref = COALESCE($2, provider_ref),
                response_status = COALESCE($3, response_status),
                response_body = COALESCE($4, response_body),
                error_message = COALESCE($5, error_message),
                status = $6,
                updated_at = NOW()
            WHERE id = $1
            RETURNING id, session_id, rail as "rail: paybank_core::PaymentRail", provider_ref, request_payload, response_status, response_body, error_message, status, created_at, updated_at
            "#,
            id, provider_ref, response_status, response_body, error_message, status
        )
        .fetch_one(pool)
        .await?;
        Ok(attempt)
    }
}