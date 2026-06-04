//! Internal cron endpoint. When the deploy can't run always-on in-process
//! workers (e.g. Cloudflare Containers that scale to zero), a scheduled caller
//! hits this to run one pass of each background job. Gated by a shared secret
//! (`CRON_SECRET`); if that's unset the endpoint is disabled, so it can never be
//! triggered unauthenticated.

use crate::state::AppState;
use axum::{extract::State, http::HeaderMap, Json};
use paybank_core::AppError;

/// Constant-time-ish equality so the secret check doesn't leak length/contents
/// via timing. The secret is high-entropy, so this is belt-and-suspenders.
fn secret_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

/// Run one pass of every background job: outbound webhooks, reconciliation, and
/// recurring invoices. Each job is idempotent, so re-runs are safe.
pub async fn cron_tick(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, AppError> {
    let expected = std::env::var("CRON_SECRET").unwrap_or_default();
    if expected.is_empty() {
        return Err(AppError::Forbidden("cron endpoint disabled".into()));
    }
    let provided = headers
        .get("x-cron-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !secret_eq(provided, &expected) {
        return Err(AppError::Forbidden("invalid cron key".into()));
    }

    let mut errors: Vec<String> = Vec::new();
    if let Err(e) = crate::webhook_worker::run_once(&state).await {
        errors.push(format!("webhooks: {e}"));
    }
    if let Err(e) = crate::reconcile::tick(&state).await {
        errors.push(format!("reconcile: {e}"));
    }
    if let Err(e) = crate::recurring::tick(&state).await {
        errors.push(format!("recurring: {e}"));
    }
    Ok(Json(serde_json::json!({
        "ok": errors.is_empty(),
        "errors": errors,
    })))
}
