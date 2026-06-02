pub mod column;
pub mod moderntreasury;
pub mod plaid;
pub mod rail;

use paybank_core::{AppError, PaymentRail};
use serde::Serialize;

pub use plaid::{create_link_token, exchange_public_token, get_auth, BankAccountDetails};
pub use rail::choose_rail;

pub enum PaymentProvider {
    ModernTreasury,
    Column,
}

impl PaymentProvider {
    pub fn primary() -> Self {
        if std::env::var("MODERN_TREASURY_API_KEY").is_ok()
            && std::env::var("MODERN_TREASURY_ORG_ID").is_ok()
            && std::env::var("MT_INTERNAL_ACCOUNT_ID").is_ok()
        {
            PaymentProvider::ModernTreasury
        } else {
            PaymentProvider::Column
        }
    }
}

pub struct CounterpartyResult {
    pub counterparty_id: String,
    pub external_account_id: String,
}

pub async fn create_counterparty(
    details: &BankAccountDetails,
    customer_name: &str,
) -> Result<CounterpartyResult, AppError> {
    match PaymentProvider::primary() {
        PaymentProvider::ModernTreasury => {
            let (cp_id, ext_id) =
                moderntreasury::create_counterparty(details, customer_name).await?;
            Ok(CounterpartyResult {
                counterparty_id: cp_id,
                external_account_id: ext_id,
            })
        }
        PaymentProvider::Column => {
            let address = column::CustomerAddress {
                line_1: "123 Main St".to_string(),
                city: "New York".to_string(),
                state: "NY".to_string(),
                postal_code: "10001".to_string(),
                country: "US".to_string(),
            };
            let cp_id = column::create_counterparty(details, customer_name, &address).await?;
            Ok(CounterpartyResult {
                counterparty_id: cp_id,
                external_account_id: String::new(),
            })
        }
    }
}

/// Onboard a merchant's own bank account via Modern Treasury (Phase 1, no
/// Plaid): create the counterparty + external account from manually-entered
/// details, then kick off ACH prenote verification. Returns
/// (counterparty_id, external_account_id, verification_status).
pub async fn onboard_merchant_bank_account(
    business_name: &str,
    account_holder: &str,
    account_type: &str,
    routing_number: &str,
    account_number: &str,
) -> Result<(String, String, String), AppError> {
    let (cp, ext) = moderntreasury::create_merchant_bank_account(
        business_name,
        account_holder,
        account_type,
        routing_number,
        account_number,
    )
    .await?;
    // Verification is best-effort: the account is created even if the prenote
    // call fails (e.g. internal account not configured) — status stays unverified.
    let status = moderntreasury::verify_external_account_prenote(&ext)
        .await
        .unwrap_or_else(|_| "unverified".to_string());
    Ok((cp, ext, status))
}

/// Poll MT for the current verification status of a merchant's external account.
pub async fn merchant_bank_account_status(external_account_id: &str) -> Result<String, AppError> {
    moderntreasury::get_external_account_status(external_account_id).await
}

pub async fn initiate_transfer(
    session_id: uuid::Uuid,
    counterparty_id: &str,
    rail: &PaymentRail,
    amount_cents: i64,
    description: &str,
) -> Result<TransferResult, AppError> {
    // Deterministic per-session key so a retried transfer reuses the same
    // provider payment order instead of moving money twice.
    let idempotency_key = format!("noor-transfer-{}", session_id);
    let provider = PaymentProvider::primary();
    match provider {
        PaymentProvider::ModernTreasury => {
            let internal_account_id = std::env::var("MT_INTERNAL_ACCOUNT_ID").map_err(|_| {
                AppError::ModernTreasuryError("MT_INTERNAL_ACCOUNT_ID not set".into())
            })?;
            let result = moderntreasury::create_transfer(
                &internal_account_id,
                counterparty_id,
                rail,
                amount_cents,
                description,
                &idempotency_key,
            )
            .await?;
            Ok(TransferResult {
                provider: "modern_treasury".to_string(),
                provider_transfer_id: result.id,
            })
        }
        PaymentProvider::Column => {
            let merchant_account_id = std::env::var("COLUMN_MERCHANT_ACCOUNT_ID")
                .map_err(|_| AppError::ColumnError("COLUMN_MERCHANT_ACCOUNT_ID not set".into()))?;
            let result = column::create_transfer(
                &merchant_account_id,
                counterparty_id,
                rail,
                amount_cents,
                description,
                &idempotency_key,
            )
            .await?;
            Ok(TransferResult {
                provider: "column".to_string(),
                provider_transfer_id: result.id,
            })
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TransferResult {
    pub provider: String,
    pub provider_transfer_id: String,
}

pub async fn check_transfer_status(provider: &str, transfer_id: &str) -> Result<String, AppError> {
    match provider {
        "modern_treasury" => moderntreasury::get_transfer_status(transfer_id).await,
        "column" => column::get_transfer_status(transfer_id).await,
        _ => Err(AppError::Internal(anyhow::anyhow!(
            "unknown provider: {}",
            provider
        ))),
    }
}
