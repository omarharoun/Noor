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
/// (role `owner`) so a fresh database has a working login. The secret is
/// authoritative for this account's password: if it changes, the hash is
/// re-synced on boot (rotate the secret + redeploy to rotate the login).
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

/// Gate `/api/merchant-api/*`. Accepts EITHER a merchant API key (x-api-key, or
/// a bearer that isn't a JWT) for programmatic access, OR a merchant-user JWT
/// (bearer) issued by `merchant_login` for the dashboard. Injects
/// `AuthedMerchant` always, and `AuthedMerchantUser` when a user token is used.
pub async fn merchant_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // 1. Explicit API key wins.
    let api_key = req
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 2. A bearer token may be a merchant-user JWT or (legacy) an API key.
    if api_key.is_none() {
        if let Some(tok) = bearer(&req).map(|s| s.to_string()) {
            if let Ok(claims) = decode_merchant_token(&state.config, &tok) {
                let merchant_id =
                    Uuid::parse_str(&claims.merchant_id).map_err(|_| AppError::Unauthorized)?;
                let user_id = Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?;
                req.extensions_mut().insert(AuthedMerchantUser {
                    user_id,
                    merchant_id,
                    email: claims.email,
                    role: claims.role,
                });
                req.extensions_mut().insert(AuthedMerchant(merchant_id));
                return Ok(next.run(req).await);
            }
            return resolve_api_key(&state, &tok, req, next).await;
        }
        return Err(AppError::Unauthorized);
    }
    resolve_api_key(&state, &api_key.unwrap(), req, next).await
}

async fn resolve_api_key(
    state: &AppState,
    key: &str,
    mut req: Request,
    next: Next,
) -> Result<Response, AppError> {
    // Runtime query (not the query! macro) so adding auth needs no offline-cache regen.
    let merchant: Option<Merchant> =
        sqlx::query_as::<_, Merchant>("SELECT * FROM merchants WHERE api_key = $1")
            .bind(key)
            .fetch_optional(&state.db.pool)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    let merchant = merchant.ok_or(AppError::Unauthorized)?;
    req.extensions_mut().insert(AuthedMerchant(merchant.id));
    Ok(next.run(req).await)
}

// ---- Merchant users (Phase 3a): multi-user accounts with roles ------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantClaims {
    pub sub: String, // merchant_user id
    pub merchant_id: String,
    pub email: String,
    pub name: String,
    pub role: String,
    pub exp: usize,
}

/// Authenticated merchant USER (from a merchant-user JWT), injected by
/// `merchant_auth` when a user token is presented.
#[derive(Clone)]
pub struct AuthedMerchantUser {
    pub user_id: Uuid,
    pub merchant_id: Uuid,
    pub email: String,
    pub role: String,
}

impl AuthedMerchantUser {
    /// Roles allowed to approve payouts / manage users.
    pub fn can_approve(&self) -> bool {
        self.role == "owner" || self.role == "admin"
    }
}

/// Extractor: requires a merchant-USER token (not just an API key). Returns 401
/// when the request authenticated with an API key (no user identity) or not at all.
#[axum::async_trait]
impl<S: Send + Sync> axum::extract::FromRequestParts<S> for AuthedMerchantUser {
    type Rejection = AppError;
    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<AuthedMerchantUser>()
            .cloned()
            .ok_or(AppError::Unauthorized)
    }
}

fn issue_merchant_token(
    config: &crate::state::Config,
    user: &paybank_core::MerchantUser,
) -> Result<String, AppError> {
    let exp = (chrono::Utc::now().timestamp() + TOKEN_TTL_SECS) as usize;
    let claims = MerchantClaims {
        sub: user.id.to_string(),
        merchant_id: user.merchant_id.to_string(),
        email: user.email.clone(),
        name: user.name.clone(),
        role: user.role.clone(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(config.jwt_secret.as_bytes()),
    )
    .map_err(|e| AppError::AuthError(format!("token signing failed: {e}")))
}

fn decode_merchant_token(
    config: &crate::state::Config,
    token: &str,
) -> Result<MerchantClaims, AppError> {
    decode::<MerchantClaims>(
        token,
        &DecodingKey::from_secret(config.jwt_secret.as_bytes()),
        &Validation::default(),
    )
    .map(|d| d.claims)
    .map_err(|_| AppError::Unauthorized)
}

#[derive(Serialize)]
pub struct MerchantUserView {
    pub id: Uuid,
    pub email: String,
    pub name: String,
    pub role: String,
    pub is_active: bool,
}

impl From<paybank_core::MerchantUser> for MerchantUserView {
    fn from(u: paybank_core::MerchantUser) -> Self {
        MerchantUserView {
            id: u.id,
            email: u.email,
            name: u.name,
            role: u.role,
            is_active: u.is_active,
        }
    }
}

#[derive(Serialize)]
pub struct MerchantLoginResponse {
    pub token: String,
    pub user: MerchantUserView,
}

/// Merchant-user login (email + password) → JWT for the merchant dashboard.
pub async fn merchant_login(
    State(state): State<AppState>,
    Json(body): Json<LoginBody>,
) -> Result<Json<MerchantLoginResponse>, AppError> {
    {
        let mut map = state.login_throttle.lock().unwrap();
        let now = std::time::Instant::now();
        let entry = map.entry(format!("mu:{}", body.email)).or_insert((0, now));
        if now.duration_since(entry.1).as_secs() > LOGIN_WINDOW_SECS {
            *entry = (0, now);
        }
        if entry.0 >= LOGIN_MAX_ATTEMPTS {
            return Err(AppError::TooManyRequests);
        }
        entry.0 += 1;
    }

    let user: Option<paybank_core::MerchantUser> = sqlx::query_as::<_, paybank_core::MerchantUser>(
        "SELECT * FROM merchant_users WHERE email = $1",
    )
    .bind(&body.email)
    .fetch_optional(&state.db.pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;

    let valid = match &user {
        Some(u) if u.is_active => verify_password(&body.password, &u.password_hash),
        _ => {
            let _ = verify_password(&body.password, dummy_hash());
            false
        }
    };
    if !valid {
        return Err(AppError::Unauthorized);
    }
    let u = user.expect("valid implies Some");
    state
        .login_throttle
        .lock()
        .unwrap()
        .remove(&format!("mu:{}", body.email));
    let token = issue_merchant_token(&state.config, &u)?;
    Ok(Json(MerchantLoginResponse {
        token,
        user: u.into(),
    }))
}

#[derive(serde::Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: String,
    pub new_password: String,
}

/// Merchant-user self-service password change. Requires the current password
/// (so a stolen session token alone can't take over the account) and a
/// merchant-user JWT — API keys can't change passwords.
pub async fn merchant_change_password(
    State(state): State<AppState>,
    actor: AuthedMerchantUser,
    Json(body): Json<ChangePasswordBody>,
) -> Result<Json<serde_json::Value>, AppError> {
    if body.new_password.len() < 8 {
        return Err(AppError::BadRequest(
            "new password must be at least 8 characters".into(),
        ));
    }
    let current_hash: String =
        sqlx::query_scalar("SELECT password_hash FROM merchant_users WHERE id = $1")
            .bind(actor.user_id)
            .fetch_one(&state.db.pool)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    if !verify_password(&body.current_password, &current_hash) {
        return Err(AppError::Unauthorized);
    }
    let new_hash = hash_password(&body.new_password)?;
    sqlx::query("UPDATE merchant_users SET password_hash = $1, updated_at = NOW() WHERE id = $2")
        .bind(&new_hash)
        .bind(actor.user_id)
        .execute(&state.db.pool)
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))?;
    Ok(Json(serde_json::json!({ "changed": true })))
}

pub async fn merchant_me(
    State(state): State<AppState>,
    req: Request,
) -> Result<Json<MerchantUserView>, AppError> {
    let token = bearer(&req).ok_or(AppError::Unauthorized)?;
    let claims = decode_merchant_token(&state.config, token)?;
    Ok(Json(MerchantUserView {
        id: Uuid::parse_str(&claims.sub).map_err(|_| AppError::Unauthorized)?,
        email: claims.email,
        name: claims.name,
        role: claims.role,
        is_active: true,
    }))
}

/// Create a merchant user (Argon2). Shared by the operator bootstrap path and
/// the merchant-side invite. `role` ∈ {owner, admin, member}.
pub async fn create_merchant_user(
    pool: &PgPool,
    merchant_id: Uuid,
    email: &str,
    name: &str,
    password: &str,
    role: &str,
) -> Result<paybank_core::MerchantUser, AppError> {
    let role = match role {
        "owner" | "admin" | "member" => role,
        _ => "member",
    };
    let hash = hash_password(password)?;
    sqlx::query_as::<_, paybank_core::MerchantUser>(
        "INSERT INTO merchant_users (merchant_id, email, password_hash, name, role) \
         VALUES ($1,$2,$3,$4,$5) RETURNING *",
    )
    .bind(merchant_id)
    .bind(email)
    .bind(&hash)
    .bind(name)
    .bind(role)
    .fetch_one(pool)
    .await
    .map_err(|e| AppError::Internal(anyhow::anyhow!(e)))
}
