//! Console operator accounts. Runtime queries (no compile-time macros) so this
//! needs no offline sqlx-cache regeneration. Password hashing/verification lives
//! in the API layer (paybank-api::auth); this layer only stores/reads the hash.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

/// Full row including the password hash — used only for login verification.
#[derive(Debug, sqlx::FromRow)]
pub struct Operator {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
}

/// Safe projection returned by list endpoints — never includes the hash.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OperatorSummary {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

/// Look up an active operator by email (case-insensitive).
pub async fn get_by_email(pool: &PgPool, email: &str) -> Result<Option<Operator>> {
    let op = sqlx::query_as::<_, Operator>(
        "SELECT id, email, name, password_hash, role, is_active
         FROM operators WHERE lower(email) = lower($1)",
    )
    .bind(email)
    .fetch_optional(pool)
    .await?;
    Ok(op)
}

/// Create a new operator. Errors (unique violation) bubble up as the DB error.
pub async fn create(
    pool: &PgPool,
    email: &str,
    name: &str,
    password_hash: &str,
    role: &str,
) -> Result<OperatorSummary> {
    let row = sqlx::query_as::<_, OperatorSummary>(
        "INSERT INTO operators (email, name, password_hash, role)
         VALUES ($1, $2, $3, $4)
         RETURNING id, email, name, role, is_active, created_at",
    )
    .bind(email)
    .bind(name)
    .bind(password_hash)
    .bind(role)
    .fetch_one(pool)
    .await?;
    Ok(row)
}

pub async fn list(pool: &PgPool) -> Result<Vec<OperatorSummary>> {
    let rows = sqlx::query_as::<_, OperatorSummary>(
        "SELECT id, email, name, role, is_active, created_at
         FROM operators ORDER BY created_at",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn count(pool: &PgPool) -> Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT COUNT(*)::int8 FROM operators")
        .fetch_one(pool)
        .await?;
    Ok(n)
}

/// Idempotently seed a bootstrap operator (does nothing if the email already
/// exists, so a changed password is never reset on the next boot).
pub async fn upsert_bootstrap(
    pool: &PgPool,
    email: &str,
    name: &str,
    password_hash: &str,
    role: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO operators (email, name, password_hash, role)
         VALUES ($1, $2, $3, $4)
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(email)
    .bind(name)
    .bind(password_hash)
    .bind(role)
    .execute(pool)
    .await?;
    Ok(())
}
