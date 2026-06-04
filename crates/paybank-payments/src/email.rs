//! Best-effort transactional email via SendGrid. No-op until SENDGRID_API_KEY
//! is set, so it's safe to ship before the key lands in the environment.
//! `EMAIL_FROM` / fallback recipient default to noreply@repost.io.

/// Send a plaintext email. Infallible by design (notifications must never fail
/// because email is down) — failures are logged, not propagated.
pub async fn send_email(to: &str, subject: &str, body: &str) {
    let key = match std::env::var("SENDGRID_API_KEY") {
        Ok(k) if !k.is_empty() => k,
        _ => return, // not configured yet — silently skip
    };
    let from = std::env::var("EMAIL_FROM").unwrap_or_else(|_| "noreply@repost.io".to_string());
    let payload = serde_json::json!({
        "personalizations": [{ "to": [{ "email": to }] }],
        "from": { "email": from },
        "subject": subject,
        "content": [{ "type": "text/plain", "value": body }],
    });
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
        .post("https://api.sendgrid.com/v3/mail/send")
        .bearer_auth(key)
        .json(&payload)
        .send()
        .await
    {
        Ok(r) if r.status().is_success() => {}
        Ok(r) => {
            let st = r.status();
            let b = r.text().await.unwrap_or_default();
            tracing::warn!("sendgrid send failed: {} {}", st, b);
        }
        Err(e) => tracing::warn!(error = %e, "sendgrid request failed"),
    }
}
