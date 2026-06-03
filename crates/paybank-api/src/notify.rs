//! In-app notifications — a small merchant-level activity feed. Best-effort:
//! a failed insert never blocks the money path.
use sqlx::PgPool;
use uuid::Uuid;

pub async fn push(pool: &PgPool, merchant_id: Uuid, kind: &str, message: &str) {
    let _ =
        sqlx::query("INSERT INTO notifications (merchant_id, kind, message) VALUES ($1, $2, $3)")
            .bind(merchant_id)
            .bind(kind)
            .bind(message)
            .execute(pool)
            .await;
}
