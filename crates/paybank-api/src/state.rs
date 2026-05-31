use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Typed, validated startup configuration. Loaded once at boot (fail-fast) so we
/// never scatter `std::env::var` reads through request handlers for security-
/// sensitive values like the JWT secret or admin bootstrap credentials.
#[derive(Clone)]
pub struct Config {
    pub public_app_url: String,
    pub jwt_secret: String,
    // Optional bootstrap operator seed (ADMIN_EMAIL/ADMIN_PASSWORD). Real accounts
    // live in the `operators` table; these only seed the first login on a fresh DB.
    pub admin_email: Option<String>,
    pub admin_password: Option<String>,
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let public_app_url =
            std::env::var("PUBLIC_APP_URL").unwrap_or_else(|_| "http://localhost:8888".into());
        // Required. Fail fast rather than signing tokens with a default.
        let jwt_secret = std::env::var("JWT_SECRET")
            .map_err(|_| anyhow::anyhow!("JWT_SECRET must be set (signs admin session tokens)"))?;
        if jwt_secret.len() < 32 {
            anyhow::bail!("JWT_SECRET is too short; use a 32+ byte random secret");
        }
        // Optional bootstrap operator seed. If set, an operator row is upserted on
        // startup (role `owner`); otherwise the DB `operators` table is the only
        // source of logins.
        let admin_email = std::env::var("ADMIN_EMAIL").ok().filter(|s| !s.is_empty());
        let admin_password = std::env::var("ADMIN_PASSWORD").ok().filter(|s| !s.is_empty());
        Ok(Self {
            public_app_url,
            jwt_secret,
            admin_email,
            admin_password,
        })
    }
}

/// Fixed-window failed-login counter keyed by email. Cheap brute-force defense
/// for the bootstrap operator credential until a real user store + IP-based
/// limiter lands.
pub type LoginThrottle = Arc<Mutex<HashMap<String, (u32, Instant)>>>;

#[derive(Clone)]
pub struct AppState {
    pub db: paybank_db::Db,
    pub config: Arc<Config>,
    pub login_throttle: LoginThrottle,
}

impl AppState {
    pub fn new(db: paybank_db::Db, config: Config) -> Self {
        Self {
            db,
            config: Arc::new(config),
            login_throttle: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}
