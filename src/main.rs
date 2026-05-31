use dotenvy::dotenv;
use paybank_api::{build_router, AppState, Config};
use paybank_db::Db;
use sqlx::postgres::PgPoolOptions;
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenv().ok();

    // Structured JSON logs + env-driven level (RUST_LOG). Request-id/span
    // enrichment via tower-http TraceLayer is a follow-up.
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Load any file-backed secrets (Docker/Compose/Swarm secrets, k8s secret
    // volumes) BEFORE anything reads the environment: `FOO_FILE=/run/secrets/x`
    // populates `FOO` from the file. Must precede Config::from_env and the
    // crypto/DB reads below.
    match paybank_api::secrets::load_file_backed_secrets() {
        Ok(loaded) if !loaded.is_empty() => {
            info!(secrets = ?loaded, "loaded file-backed secrets")
        }
        Ok(_) => {}
        Err(e) => anyhow::bail!("failed to load file-backed secret: {e}"),
    }

    // Fail fast on missing/invalid config rather than at first request.
    let config = Config::from_env()?;

    // In production, refuse to start without working PII encryption (fail-closed):
    // never silently store bank account/routing numbers in plaintext.
    let is_prod = std::env::var("NOOR_ENV")
        .map(|v| v == "production")
        .unwrap_or(false);
    if is_prod && !paybank_core::crypto::is_configured() {
        anyhow::bail!("NOOR_ENV=production but PII_ENCRYPTION_KEY is missing/invalid (refusing to store PII unencrypted)");
    }

    let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let max_conns: u32 = std::env::var("DATABASE_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let pool = PgPoolOptions::new()
        .max_connections(max_conns)
        .connect(&database_url)
        .await?;

    // Boot-time migrations run as the connecting role. With a least-privilege app
    // role (no DDL), set NOOR_SKIP_MIGRATE=true and run migrations as the owner
    // role in a separate deploy step.
    if std::env::var("NOOR_SKIP_MIGRATE")
        .map(|v| v == "true")
        .unwrap_or(false)
    {
        info!("NOOR_SKIP_MIGRATE=true — skipping boot-time migrations");
    } else {
        sqlx::migrate!("./migrations").run(&pool).await?;
    }

    // Seed the bootstrap operator (from ADMIN_EMAIL/ADMIN_PASSWORD) if configured.
    paybank_api::auth::ensure_bootstrap_operator(&pool, &config).await?;

    let db = Db::new(pool);
    let state = AppState::new(db, config);

    // Background outbound-webhook delivery worker (drains the webhook_events outbox).
    paybank_api::webhook_worker::spawn(state.clone());
    // Reconciliation poller for sessions stuck in `processing`.
    paybank_api::reconcile::spawn(state.clone());

    let router = build_router(state);

    let addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:8888".into());
    info!("Noor API starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

/// Drain in-flight requests on SIGTERM/Ctrl-C so deploys don't sever live
/// payment requests mid-flight.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };

    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut sig) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            sig.recv().await;
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    info!("shutdown signal received, draining");
}
