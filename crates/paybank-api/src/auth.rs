//! Authentication & authorization.
//!
//! - Admin/operator surface (`/api/admin/*`): bearer JWT issued by `POST /login`,
//!   validated against bootstrap credentials in `Config`. A real operator user
//!   store is a follow-up; this closes the wide-open admin surface today.
//! - Merchant surface (`/api/merchant-api/*`): `X-API-Key` (or `Authorization:
//!   Bearer <key>`) validated against `merchants.api_key`; the resolved merchant
//!   id is injected into request extensions so handlers stop using a hardcoded
//!   demo id.

use crate::state::{AppState, Config};
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use paybank_core::{AppError, Merchant};
use paybank_db::{audit_repo, operator_repo};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::OnceLock;
use uuid::Uuid;

const TOKEN_TTL_SECS: i64 = 12 * 3600;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // operator email
    pub name: String,
    pub role: String,
    pub exp: usize,
}

/// The authenticated merchant, injected into request extensions by `merchant_auth`.
#[derive(Clone, Copy)]
pub struct AuthedMerchant(pub Uuid);

/// The authenticated operator (decoded JWT claims), injected by `admin_auth`.
#[derive(Clone)]
pub struct AuthedOperator(pub Claims);

// ---- password hashing (Argon2) -------------------------------------------

/// Hash a plaintext password with Argon2id. Public so operator-management
/// handlers can hash new operators' passwords.
pub fn hash_password(plaintext: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(plaintext.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("password hashing failed: {e}")))
}

fn verify_password(plaintext: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|parsed| {
            Argon2::default()
                .verify_password(plaintext.as_bytes(), &parsed)
                .is_ok()
        })
        .unwrap_or(false)
}

/// A valid Argon2 hash of a throwaway value, computed once, used to keep the
/// "no such operator" path's timing close to the real verify (anti-enumeration).
fn dummy_hash() -> &'static str {
    static H: OnceLock<String> = OnceLock::new();
    H.get_or_init(|| hash_password("noor-timing-dummy").unwrap_or_default())
}

fn issue_token(
    config: &crate::state::Config,
    claims_sub: &str,
    name: &str,
    role: &str,
) -> Result<String, AppError> {
    let exp = (chrono::Utc::now().timestamp() + TOKEN_TTL_SECS) as usize;
    let claims = Claims {
        sub: claims_sub.to_string(),
        name: name.to_string(),
        role: role.to_string(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::AuthError(format!("token signing failed: {e}")))
}

fn decode_token(config: &crate::state::Config, token: &str) -> Result<Claims, AppError> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|_| AppError::Unauthorized)
}

fn bearer(req: &Request) -> Option<&str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

// ---- handlers -------------------------------------------------------------

#[derive(Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct Operator {
    pub name: String,
    pub email: String,
    pub role: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
    pub operator: Operator,
}

const LOGIN_WINDOW_SECS: u64 = 300;
const LOGIN_MAX_ATTEMPTS: u32 = 10;

pub async fn login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<LoginResponse>, AppError> {
    let cfg = &state.config;

    // Fixed-window brute-force throttle (per email, in-process).
    {
        let mut map = state.login_throttle.lock().unwrap();
        let now = std::time::Instant::now();
        let entry = map.entry(body.email.clone()).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() > LOGIN_WINDOW_SECS {
            *entry = (0, now);
        }
        if entry.0 >= LOGIN_MAX_ATTEMPTS {
            return Err(AppError::TooManyRequests);
        }
        entry.0 += 1;
    }

    // DB-backed operator accounts (Argon2). Look up by email, verify the hash.
    let operator = operator_repo::get_by_email(&state.db.pool, &body.email)
        .await
        .map_err(AppError::Internal)?;

    let valid = match &operator {
        Some(op) if op.is_active => verify_password(&body.password, &op.password_hash),
        // No such (active) operator → still run a verify against a dummy hash so
        // the response time doesn't reveal whether the email exists.
        _ => {
            let _ = verify_password(&body.password, dummy_hash());
            false
        }
    };
    if !valid {
        return Err(AppError::Unauthorized);
    }
    let op = operator.expect("valid implies Some");

    // Successful login clears the counter.
    state.login_throttle.lock().unwrap().remove(&body.email);
    audit_repo::record(
        &state.db.pool,
        &op.email,
        Some(&op.role),
        "operator.login",
        None,
        None,
        serde_json::json!({}),
    )
    .await;
    let token = issue_token(cfg, &op.email, &op.name, &op.role)?;
    Ok(Json(LoginResponse {
        token,
        operator: Operator {
            name: op.name,
            email: op.email,
            role: op.role,
        },
    }))
}

/// Idempotently seed the bootstrap operator from `ADMIN_EMAIL`/`ADMIN_PASSWORD`
/// (role `owner`) so a fresh database has a working login. Does nothing if that
/// email already exists (a changed password is never reset on reboot).
pub async fn ensure_bootstrap_operator(pool: &PgPool, config: &Config) -> anyhow::Result<()> {
    match (&config.admin_email, &config.admin_password) {
        (Some(email), Some(password)) => {
            let hash = hash_password(password).map_err(|e| anyhow::anyhow!("{e}"))?;
            operator_repo::upsert_bootstrap(pool, email, "Operator", &hash, "owner").await?;
            tracing::info!(email = %email, "bootstrap operator ensured");
        }
        _ => {
            if operator_repo::count(pool).await? == 0 {
                tracing::warn!(
                    "no operators exist and ADMIN_EMAIL/ADMIN_PASSWORD not set — the admin console has NO login"
                );
            }
        }
    }
    Ok(())
}

pub async fn me(State(state): State<AppState>, req: Request) -> Result<Json<Operator>, AppError> {
    let token = bearer(&req).ok_or(AppError::Unauthorized)?;
    let claims = decode_token(&state.config, token)?;
    Ok(Json(Operator {
        name: claims.name,
        email: claims.sub,
        role: claims.role,
    }))
}

// ---- middleware -----------------------------------------------------------

/// Gate `/api/admin/*` (except `/login`) on a valid operator JWT.
pub async fn admin_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = bearer(&req).ok_or(AppError::Unauthorized)?;
    let claims = decode_token(&state.config, token)?;
    // Expose the operator (incl. role) to handlers for role-gated actions.
    req.extensions_mut().insert(AuthedOperator(claims));
    Ok(next.run(req).await)
}

/// Gate `/api/merchant-api/*` on a valid merchant API key and inject the merchant id.
pub async fn merchant_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .or_else(|| bearer(&req).map(|s| s.to_string()))
        .ok_or(AppError::Unauthorized)?;

    // Runtime query (not the query! macro) so adding auth needs no offline-cache regen.
    let merchant: Option<Merchant> =
        sqlx::query_as::<_, Merchant>("SELECT * FROM merchants WHERE api_key = $1")
            .bind(&key)
            .fetch_optional(&state.db.pool)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let merchant = merchant.ok_or(AppError::Unauthorized)?;
    req.extensions_mut().insert(AuthedMerchant(merchant.id));
    Ok(next.run(req).await)
}
