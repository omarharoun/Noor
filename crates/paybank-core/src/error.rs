use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("invalid amount")]
    InvalidAmount,

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

    #[error("plaid api error: {0}")]
    PlaidError(String),

    #[error("authentication error: {0}")]
    AuthError(String),

    #[error("unauthorized")]
    Unauthorized,

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
            AppError::PlaidError(_) => (StatusCode::BAD_GATEWAY, self.to_string()),
            AppError::AuthError(_) => (StatusCode::UNAUTHORIZED, self.to_string()),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, self.to_string()),
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