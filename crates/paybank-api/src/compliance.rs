//! Compliance gate: merchant KYC + customer sanctions screening, FAIL-CLOSED.
//!
//! `screen_customer` is a STUB — wire a real provider (OFAC SDN list,
//! ComplyAdvantage, etc.) there. The contract that matters for correctness: any
//! screening error must DENY (never allow), and a session must pass this gate
//! before it can be authorized (and therefore before any transfer can fire).

use paybank_core::AppError;
use paybank_db::admin_repo;
use sqlx::PgPool;
use uuid::Uuid;

pub struct Screen {
    pub cleared: bool,
    pub reason: Option<String>,
}

/// Placeholder sanctions screen. Replace the body with a real vendor call.
/// Returns `Err` only when the provider is unreachable — callers MUST treat
/// `Err` as "deny" (fail closed), which `gate` does.
pub async fn screen_customer(name: &str, _address: &str) -> Result<Screen, AppError> {
    // Stand-in for an OFAC SDN match until a real provider is integrated.
    const DENY_SUBSTRINGS: [&str; 3] = ["ofac test", "sanctioned", "blocked person"];
    let lname = name.to_lowercase();
    if DENY_SUBSTRINGS.iter().any(|d| lname.contains(d)) {
        return Ok(Screen {
            cleared: false,
            reason: Some("name matched a sanctions list entry".into()),
        });
    }
    Ok(Screen {
        cleared: true,
        reason: None,
    })
}

/// Fail-closed gate run before a session is authorized:
///   1. the merchant must be KYC-verified, and
///   2. the customer must clear sanctions screening.
///
/// Any screening provider error denies.
pub async fn gate(
    pool: &PgPool,
    merchant_id: Uuid,
    customer_name: &str,
    customer_address: &str,
) -> Result<(), AppError> {
    let merchant = admin_repo::get_merchant(pool, merchant_id)
        .await?
        .ok_or(AppError::MerchantNotFound)?;
    if merchant.kyc_status != "verified" {
        return Err(AppError::ComplianceRejected(format!(
            "merchant KYC not verified (status: {})",
            merchant.kyc_status
        )));
    }

    match screen_customer(customer_name, customer_address).await {
        Ok(s) if s.cleared => Ok(()),
        Ok(s) => Err(AppError::ComplianceRejected(
            s.reason
                .unwrap_or_else(|| "sanctions screening failed".into()),
        )),
        // Fail closed: never authorize money movement when screening is down.
        Err(_) => Err(AppError::ComplianceRejected(
            "sanctions screening temporarily unavailable".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn clean_name_is_cleared() {
        let s = screen_customer("Jane Q. Public", "1 Main St")
            .await
            .expect("clean screen must not error");
        assert!(s.cleared, "a clean name must clear screening");
        assert!(s.reason.is_none(), "cleared screen carries no reason");
    }

    #[tokio::test]
    async fn ofac_test_substring_is_denied_with_reason() {
        // Exact deny term cased differently to exercise case-insensitive match.
        let s = screen_customer("OFAC Test", "1 Main St")
            .await
            .expect("deny match returns Ok(cleared=false), not Err");
        assert!(!s.cleared, "an OFAC Test name must not clear");
        let reason = s.reason.expect("a denied screen must carry a reason");
        assert!(
            reason.contains("sanctions"),
            "reason should reference the sanctions match, got: {reason}"
        );
    }

    #[tokio::test]
    async fn deny_term_embedded_in_larger_name_is_denied() {
        // Substring (not exact) match: term appears inside a longer string.
        let s = screen_customer("Acme Blocked Person LLC", "")
            .await
            .expect("deny match returns Ok, not Err");
        assert!(!s.cleared, "embedded deny substring must be caught");
        assert!(s.reason.is_some());
    }

    #[tokio::test]
    async fn matching_is_case_insensitive() {
        let upper = screen_customer("THIS IS A SANCTIONED ENTITY", "")
            .await
            .unwrap();
        assert!(!upper.cleared, "uppercase deny term must still match");

        let mixed = screen_customer("SaNcTiOnEd Co", "").await.unwrap();
        assert!(!mixed.cleared, "mixed-case deny term must still match");
    }

    #[tokio::test]
    async fn near_miss_does_not_match() {
        // "ofac" alone is not a deny substring ("ofac test" is); must clear.
        let s = screen_customer("OFAC Liaison Office", "").await.unwrap();
        assert!(
            s.cleared,
            "a partial-but-incomplete term must not trip the deny list"
        );
        assert!(s.reason.is_none());
    }

    #[tokio::test]
    async fn empty_name_is_cleared() {
        let s = screen_customer("", "").await.unwrap();
        assert!(s.cleared, "an empty name matches no deny term");
        assert!(s.reason.is_none());
    }
}
