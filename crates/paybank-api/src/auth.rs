//! Authentication & authorization.
//!
//! - Admin/operator surface (`/api/admin/*`): bearer JWT issued by `POST /login`,
//!   validated against bootstrap credentials in `Config`. A real operator user
//!   store is a follow-up; this closes the wide-open admin surface today.
//! - Merchant surface (`/api/merchant-api/*`): `X-API-Key` (or `Authorization:
//!   Bearer <key>`) validated against `merchants.api_key`; the resolved merchant
//!   id is injected into request extensions so handlers stop using a hardcoded
//!   demo id.

use crate::state::AppState;
use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use paybank_core::{AppError, Merchant};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const TOKEN_TTL_SECS: i64 = 12 * 3600;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String, // operator email
    pub name: String,
    pub role: String,
    pub exp: usize,
}

/// The authenticated merchant, injected into request extensions by `merchant_auth`.
#[derive(Clone, Copy)]
pub struct AuthedMerchant(pub Uuid);

fn issue_token(config: &crate::state::Config, claims_sub: &str, name: &str, role: &str) -> Result<String, AppError> {
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

fn bearer<'a>(req: &'a Request) -> Option<&'a str> {
    req.headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
}

/// Constant-time string comparison to avoid leaking the admin password via timing.
fn ct_eq(a: &str, b: &str) -> bool {
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

    // Both comparisons run regardless of email match to keep timing flat.
    let email_ok = ct_eq(&body.email, &cfg.admin_email);
    let pw_ok = ct_eq(&body.password, &cfg.admin_password);
    if !(email_ok && pw_ok) {
        return Err(AppError::Unauthorized);
    }

    // Successful login clears the counter.
    state.login_throttle.lock().unwrap().remove(&body.email);
    let name = "Operator";
    let role = "operator";
    let token = issue_token(cfg, &cfg.admin_email, name, role)?;
    Ok(Json(LoginResponse {
        token,
        operator: Operator {
            name: name.to_string(),
            email: cfg.admin_email.clone(),
            role: role.to_string(),
        },
    }))
}

pub async fn me(
    State(state): State<AppState>,
    req: Request,
) -> Result<Json<Operator>, AppError> {
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
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let token = bearer(&req).ok_or(AppError::Unauthorized)?;
    decode_token(&state.config, token)?;
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
