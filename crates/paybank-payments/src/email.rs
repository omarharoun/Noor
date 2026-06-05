//! Best-effort transactional email. The Cloudflare Email binding (`env.EMAIL`)
//! lives on the Worker, and a container can't hold a Worker binding — so the
//! container POSTs an internal endpoint on the Worker (`EMAIL_ENDPOINT`),
//! authenticated with `CRON_SECRET`, and the Worker calls `env.EMAIL.send()`.
//! No-op until both `EMAIL_ENDPOINT` and `CRON_SECRET` are set, so it's safe to
//! ship before email is wired.

/// Send a plaintext email. Infallible by design (notifications must never fail
/// because email is down) — failures are logged, not propagated.
pub async fn send_email(to: &str, subject: &str, body: &str) {
    let endpoint = match std::env::var("EMAIL_ENDPOINT") {
        Ok(e) if !e.is_empty() => e,
        _ => return, // email not wired yet — silently skip
    };
    let secret = std::env::var("CRON_SECRET").unwrap_or_default();
    if secret.is_empty() {
        return;
    }
    let payload = serde_json::json!({ "to": to, "subject": subject, "text": body });
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(error = %e, "email client build failed");
            return;
        }
    };
    match client
        .post(&endpoint)
        .header("x-cron-key", secret)
        .json(&payload)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {}
        Ok(r) => {
            let st = r.status();
            let b = r.text().await.unwrap_or_default();
            tracing::warn!("email send failed: {} {}", st, b);
        }
        Err(e) => tracing::warn!(error = %e, "email request failed"),
    }
}
