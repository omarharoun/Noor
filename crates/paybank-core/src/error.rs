use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid amount")]
    InvalidAmount,

    #[error("{0}")]
    BadRequest(String),

    #[error("bank not found")]
    BankNotFound,

    #[error("merchant not found")]
    MerchantNotFound,

    #[error("session not found")]
    SessionNotFound,

    #[error("session expired")]
    SessionExpired,

    #[error("payment failed")]
    PaymentFailed,

    #[error("payment not found")]
    PaymentNotFound,

    #[error("invalid state transition")]
    InvalidStateTransition,

    #[error("missing idempotency-key header")]
    MissingIdempotencyKey,

    #[error("idempotency key hash mismatch")]
    IdempotencyMismatch,

    #[error("idempotency key is currently processing")]
    IdempotencyConflict,

    #[error("column api error: {0}")]
    ColumnError(String),

    #[error("modern treasury api error: {0}")]
    ModernTreasuryError(String),

    /// A provider call whose outcome is UNKNOWN (network timeout, dropped
    /// connection, or 5xx). The transfer may or may not have gone through, so
    /// the caller must NOT mark the payment Failed — leave it Processing for a
    /// webhook or reconciliation job to resolve. Distinct from a definitive 4xx
    /// rejection.
    #[error("provider temporarily unavailable / outcome unknown: {0}")]
    ProviderAmbiguous(String),

    #[error("plaid api error: {0}")]
    PlaidError(String),

    #[error("authentication error: {0}")]
    AuthError(String),

    #[error("unauthorized")]
    Unauthorized,

    #[error("too many requests")]
    TooManyRequests,

    #[error("compliance check failed: {0}")]
    ComplianceRejected(String),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("internal error: {0}")]
    Internal(#[from] anyhow::Error),

    #[error("template error: {0}")]
    TemplateError(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match &self {
            AppError::InvalidAmount => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::BadRequest(_) => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::BankNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::MerchantNotFound => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::SessionNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::SessionExpired => (StatusCode::GONE, self.to_string()),
            AppError::PaymentFailed => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
            AppError::PaymentNotFound => (StatusCode::NOT_FOUND, self.to_string()),
            AppError::InvalidStateTransition => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::MissingIdempotencyKey => (StatusCode::BAD_REQUEST, self.to_string()),
            AppError::IdempotencyMismatch => (StatusCode::CONFLICT, self.to_string()),
            AppError::IdempotencyConflict => (StatusCode::CONFLICT, self.to_string()),
            AppError::ColumnError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::ModernTreasuryError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::ProviderAmbiguous(_) => (StatusCode::GATEWAY_TIMEOUT, self.to_string()),
            AppError::PlaidError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::AuthError(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, self.to_string()),
            AppError::ComplianceRejected(_) => (StatusCode::FORBIDDEN, self.to_string()),
            AppError::Database(e) => {
                tracing::error!("Database error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "database error".into())
            }
            AppError::Internal(e) => {
                tracing::error!("Internal error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "internal error".into())
            }
            AppError::TemplateError(_) => (StatusCode::INTERNAL_SERVER_ERROR, self.to_string()),
        };

        let body = serde_json::json!({ "error": message });
        (status, axum::Json(body)).into_response()
    }
}
