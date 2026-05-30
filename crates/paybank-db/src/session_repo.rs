use anyhow::Result;
use chrono::{DateTime, Utc};
use paybank_core::{PaymentRail, PaymentSession, SessionStatus};
use sqlx::PgPool;
use uuid::Uuid;

pub async fn create_session(
    pool: &PgPool,
    id: Uuid,
    merchant_id: Uuid,
    bank_id: &str,
    amount_cents: i64,
    note: Option<&str>,
    rail: &PaymentRail,
    expires_at: DateTime<Utc>,
) -> Result<PaymentSession> {
    let rail_str = rail.to_string();
    sqlx::query_as!(PaymentSession,
        r#"INSERT INTO payment_sessions
           (id, merchant_id, bank_id, amount_cents, currency, note, status, rail_used, expires_at, created_at, updated_at)
           VALUES ($1, $2, $3, $4, 'USD', $5, 'pending', $6, $7, NOW(), NOW())
           RETURNING
            id, merchant_id, bank_id, amount_cents, currency, note,
            status as "status: SessionStatus",
            rail_used as "rail_used: PaymentRail",
            column_ref, column_counterparty_id,
            customer_name, customer_email, customer_phone,
            customer_address_line_1, customer_address_city, customer_address_state,
            customer_address_postal_code, customer_address_country_code,
            customer_account_number, customer_routing_number, customer_account_type,
            expires_at, created_at, updated_at"#,
        id, merchant_id, bank_id, amount_cents, note, rail_str, expires_at
    ).fetch_one(pool).await.map_err(Into::into)
}

pub async fn get_session(pool: &PgPool, id: Uuid) -> Result<Option<PaymentSession>> {
    sqlx::query_as!(PaymentSession,
        r#"SELECT
            id, merchant_id, bank_id, amount_cents, currency, note,
            status as "status: SessionStatus",
            rail_used as "rail_used: PaymentRail",
            column_ref, column_counterparty_id,
            customer_name, customer_email, customer_phone,
            customer_address_line_1, customer_address_city, customer_address_state,
            customer_address_postal_code, customer_address_country_code,
            customer_account_number, customer_routing_number, customer_account_type,
            expires_at, created_at, updated_at
        FROM payment_sessions WHERE id = $1"#,
        id
    )
    .fetch_optional(pool)
    .await
    .map_err(Into::into)
}

pub async fn update_status(
    pool: &PgPool,
    id: Uuid,
    status: &SessionStatus,
    column_ref: Option<&str>,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE payment_sessions
           SET status = $2,
               column_ref = COALESCE($3, column_ref),
               updated_at = NOW()
           WHERE id = $1"#,
        id,
        status as &SessionStatus,
        column_ref
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn update_counterparty_id(
    pool: &PgPool,
    id: Uuid,
    counterparty_id: &str,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE payment_sessions
           SET column_counterparty_id = $2, updated_at = NOW()
           WHERE id = $1"#,
        id,
        counterparty_id
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_sessions(
    pool: &PgPool,
    merchant_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<PaymentSession>, i64)> {
    let sessions = sqlx::query_as!(PaymentSession,
        r#"SELECT
            id, merchant_id, bank_id, amount_cents, currency, note,
            status as "status: SessionStatus",
            rail_used as "rail_used: PaymentRail",
            column_ref, column_counterparty_id,
            customer_name, customer_email, customer_phone,
            customer_address_line_1, customer_address_city, customer_address_state,
            customer_address_postal_code, customer_address_country_code,
            customer_account_number, customer_routing_number, customer_account_type,
            expires_at, created_at, updated_at
        FROM payment_sessions WHERE merchant_id = $1
        ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
        merchant_id, limit, offset
    )
    .fetch_all(pool)
    .await?;

    let total: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::int8 FROM payment_sessions WHERE merchant_id = $1",
        merchant_id
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

    Ok((sessions, total))
}

pub async fn update_customer_details(
    pool: &PgPool,
    id: Uuid,
    name: &str,
    email: &str,
    phone: &str,
    address_line_1: &str,
    city: &str,
    state: &str,
    postal_code: &str,
    country_code: &str,
    account_number: &str,
    routing_number: &str,
    account_type: &str,
) -> Result<()> {
    sqlx::query!(
        r#"UPDATE payment_sessions SET
            customer_name = $2,
            customer_email = $3,
            customer_phone = $4,
            customer_address_line_1 = $5,
            customer_address_city = $6,
            customer_address_state = $7,
            customer_address_postal_code = $8,
            customer_address_country_code = $9,
            customer_account_number = $10,
            customer_routing_number = $11,
            customer_account_type = $12,
            updated_at = NOW()
        WHERE id = $1"#,
        id, name, email, phone, address_line_1, city, state, postal_code, country_code, account_number, routing_number, account_type
    )
    .execute(pool)
    .await?;
    Ok(())
}