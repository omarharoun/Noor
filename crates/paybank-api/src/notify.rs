//! In-app notifications — a small merchant-level activity feed. Best-effort:
//! a failed insert never blocks the money path.
use sqlx::{PgPool, Row};
use uuid::Uuid;

pub async fn push(pool: &PgPool, merchant_id: Uuid, kind: &str, message: &str) {
    let _ =
        sqlx::query("INSERT INTO notifications (merchant_id, kind, message) VALUES ($1, $2, $3)")
            .bind(merchant_id)
            .bind(kind)
            .bind(message)
            .execute(pool)
            .await;
    // Also email the notification (no-op until SENDGRID_API_KEY is set). It's
    // sent FROM noreply@depost.io TO the merchant's own contact email —
    // noreply is send-only and must never be a recipient. Fire-and-forget so a
    // slow SendGrid call never blocks the money path.
    let to: Option<String> = sqlx::query("SELECT email FROM merchants WHERE id = $1")
        .bind(merchant_id)
        .fetch_optional(pool)
        .await
        .ok()
        .flatten()
        .and_then(|r| r.try_get::<String, _>("email").ok())
        .filter(|e| !e.trim().is_empty());
    if let Some(to) = to {
        let subject = format!("Noor — {kind}");
        let body = message.to_string();
        tokio::spawn(async move {
            paybank_payments::send_email(&to, &subject, &body).await;
        });
    }
}
