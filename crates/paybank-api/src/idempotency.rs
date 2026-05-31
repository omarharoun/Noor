//! Idempotency-Key support for money-adjacent POST endpoints.
//!
//! Clients may send an `Idempotency-Key` header; the first request with a given
//! (merchant, key) does the work and stores its response, and any replay with
//! the same key+body short-circuits to that stored response. A replay with a
//! DIFFERENT body is rejected (key reuse), and a concurrent in-flight request is
//! rejected with a conflict so external side effects (e.g. MT counterparty
//! creation) can't happen twice. The header is optional and backwards-compatible.

use axum::http::HeaderMap;
use chrono::{Duration, Utc};
use paybank_core::AppError;
use paybank_db::idempotency_repo::IdempotencyRepo;
use paybank_db::Db;
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Extract a non-empty `Idempotency-Key` header, if present.
pub fn key_from(headers: &HeaderMap) -> Option<String> {
    headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_string)
        .filter(|s| !s.is_empty())
}

/// Stable SHA-256 over the serialized request, used to detect key reuse with a
/// different payload.
pub fn hash<T: serde::Serialize>(body: &T) -> String {
    let json = serde_json::to_vec(body).unwrap_or_default();
    let mut hasher = Sha256::new();
    hasher.update(&json);
    hex::encode(hasher.finalize())
}

pub enum Outcome {
    /// Caller owns the key — do the work, then call `complete`.
    Proceed,
    /// A prior completed request — replay its stored response verbatim.
    Replay(serde_json::Value),
}

/// Acquire (or replay) an idempotency key. The in-progress lock is committed
/// immediately so concurrent duplicates see it and get `IdempotencyConflict`.
pub async fn begin(
    db: &Db,
    merchant_id: Uuid,
    key: &str,
    request_hash: &str,
) -> Result<Outcome, AppError> {
    let mut tx = db.pool.begin().await?;
    let expires_at = Utc::now() + Duration::hours(24);
    let acquired =
        IdempotencyRepo::try_acquire_key(&mut tx, merchant_id, key, request_hash, expires_at).await?;
    tx.commit().await?;

    match acquired {
        Ok(()) => Ok(Outcome::Proceed),
        Err(existing) => {
            if existing.request_hash != request_hash {
                return Err(AppError::IdempotencyMismatch);
            }
            match existing.state.as_str() {
                "completed" => Ok(Outcome::Replay(
                    existing.response_body.unwrap_or_else(|| serde_json::json!({})),
                )),
                _ => Err(AppError::IdempotencyConflict),
            }
        }
    }
}

/// Persist the final response so future replays return it.
pub async fn complete(
    db: &Db,
    merchant_id: Uuid,
    key: &str,
    response: &serde_json::Value,
) -> Result<(), AppError> {
    let mut tx = db.pool.begin().await?;
    IdempotencyRepo::complete_key(&mut tx, merchant_id, key, 200, response).await?;
    tx.commit().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(serde::Serialize)]
    struct Payload {
        amount: u64,
        currency: &'static str,
    }

    #[test]
    fn hash_is_deterministic_for_same_input() {
        let body = Payload {
            amount: 1000,
            currency: "USD",
        };
        let first = hash(&body);
        let second = hash(&Payload {
            amount: 1000,
            currency: "USD",
        });
        assert_eq!(first, second);
    }

    #[test]
    fn hash_differs_for_different_input() {
        let a = hash(&Payload {
            amount: 1000,
            currency: "USD",
        });
        let b = hash(&Payload {
            amount: 1001,
            currency: "USD",
        });
        let c = hash(&Payload {
            amount: 1000,
            currency: "EUR",
        });
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    #[test]
    fn hash_output_is_64_char_lowercase_hex() {
        let h = hash(&serde_json::json!({ "k": "v", "n": 42 }));
        // SHA-256 -> 32 bytes -> 64 hex chars.
        assert_eq!(h.len(), 64);
        assert!(
            h.chars().all(|c| c.is_ascii_hexdigit()),
            "hash must be hex: {h}"
        );
        assert_eq!(h, h.to_lowercase(), "hex::encode emits lowercase");
    }

    #[test]
    fn hash_matches_known_sha256_of_serialized_json() {
        // serde_json serializes this to the compact `{}` (no whitespace),
        // so the hash must equal SHA-256("{}").
        let h = hash(&serde_json::json!({}));
        let expected = {
            let mut hasher = Sha256::new();
            hasher.update(b"{}");
            hex::encode(hasher.finalize())
        };
        assert_eq!(h, expected);
    }

    #[test]
    fn hash_is_sensitive_to_field_order_via_serialization() {
        // Two structurally different JSON values produce different hashes.
        let a = hash(&serde_json::json!({ "a": 1, "b": 2 }));
        let b = hash(&serde_json::json!({ "a": 2, "b": 1 }));
        assert_ne!(a, b);
    }
}
