//! Append-only audit log. Runtime queries (no compile-time macros).
//! Recording is best-effort at call sites (a logging failure must never fail the
//! underlying action), so most callers ignore the result and we log internally.

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AuditEntry {
    pub id: uuid::Uuid,
    pub actor: String,
    pub actor_role: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata: Value,
    pub created_at: DateTime<Utc>,
}

/// Record an audit event. Logs (does not propagate) failures so auditing can
/// never break the action it describes.
pub async fn record(
    pool: &PgPool,
    actor: &str,
    actor_role: Option<&str>,
    action: &str,
    target_type: Option<&str>,
    target_id: Option<&str>,
    metadata: Value,
) {
    let res = sqlx::query(
        "INSERT INTO audit_log (actor, actor_role, action, target_type, target_id, metadata)
         VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(actor)
    .bind(actor_role)
    .bind(action)
    .bind(target_type)
    .bind(target_id)
    .bind(metadata)
    .execute(pool)
    .await;
    if let Err(e) = res {
        tracing::error!(action = %action, error = %e, "failed to write audit log entry");
    }
}

pub async fn list(pool: &PgPool, limit: i64) -> Result<Vec<AuditEntry>> {
    let rows = sqlx::query_as::<_, AuditEntry>(
        "SELECT id, actor, actor_role, action, target_type, target_id, metadata, created_at
         FROM audit_log ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
