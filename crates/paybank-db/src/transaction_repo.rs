use anyhow::Result;
use chrono::{DateTime, Utc};
use paybank_core::{PaymentRail, Transaction};
use sqlx::PgPool;
use uuid::Uuid;

#[allow(clippy::too_many_arguments)] // one parameter per persisted column
pub async fn create_transaction(
    pool: &PgPool,
    id: Uuid,
    session_id: Uuid,
    merchant_id: Uuid,
    amount_cents: i64,
    rail: &PaymentRail,
    column_ref: Option<&str>,
    reference: &str,
    settled_at: DateTime<Utc>,
) -> Result<Transaction> {
    let rail_str = rail.to_string();
    sqlx::query_as!(Transaction,
        r#"INSERT INTO transactions
           (id, session_id, merchant_id, amount_cents, rail_used, column_ref, reference, settled_at, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW())
           RETURNING id, session_id, merchant_id, amount_cents, rail_used as "rail_used: PaymentRail", column_ref, reference, settled_at, created_at"#,
        id, session_id, merchant_id, amount_cents, rail_str, column_ref, reference, settled_at
    ).fetch_one(pool).await.map_err(Into::into)
}

pub async fn list_transactions(
    pool: &PgPool,
    merchant_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Transaction>, i64)> {
    let txns = sqlx::query_as!(Transaction,
        r#"SELECT id, session_id, merchant_id, amount_cents, rail_used as "rail_used: PaymentRail", column_ref, reference, settled_at, created_at
           FROM transactions WHERE merchant_id = $1
           ORDER BY created_at DESC LIMIT $2 OFFSET $3"#,
        merchant_id, limit, offset
    )
    .fetch_all(pool)
    .await?;

    let total: i64 = sqlx::query_scalar!(
        "SELECT COUNT(*)::int8 FROM transactions WHERE merchant_id = $1",
        merchant_id
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0);

    Ok((txns, total))
}
