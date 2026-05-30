use base64::Engine;
use paybank_core::{AppError, PaymentRail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::plaid::BankAccountDetails;

const MT_BASE_URL: &str = "https://app.moderntreasury.com/api";

fn mt_client() -> Result<Client, AppError> {
    let org_id = std::env::var("MODERN_TREASURY_ORG_ID")
        .map_err(|_| AppError::ModernTreasuryError("MODERN_TREASURY_ORG_ID not set".into()))?;
    let api_key = std::env::var("MODERN_TREASURY_API_KEY")
        .map_err(|_| AppError::ModernTreasuryError("MODERN_TREASURY_API_KEY not set".into()))?;

    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            let auth = format!("{}:{}", org_id, api_key);
            let encoded = base64::engine::general_purpose::STANDARD.encode(auth);
            let mut auth_header =
                reqwest::header::HeaderValue::from_str(&format!("Basic {}", encoded))
                    .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
            auth_header.set_sensitive(true);
            h.insert(reqwest::header::AUTHORIZATION, auth_header);
            h.insert(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            );
            h
        })
        .build()
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?)
}

#[derive(Debug, Serialize)]
struct CreateCounterpartyRequest {
    name: String,
    accounts: Vec<CreateExternalAccountRequest>,
}

#[derive(Debug, Serialize)]
struct CreateExternalAccountRequest {
    account_details: Vec<ExternalAccountDetail>,
    routing_details: Vec<RoutingDetail>,
}

#[derive(Debug, Serialize)]
struct ExternalAccountDetail {
    #[serde(rename = "type")]
    detail_type: String,
    account_number: String,
}

#[derive(Debug, Serialize)]
struct RoutingDetail {
    routing_number: String,
    routing_number_type: String,
}

#[derive(Debug, Deserialize)]
struct CounterpartyResponse {
    id: String,
    name: String,
    accounts: Vec<ExternalAccountResponse>,
}

#[derive(Debug, Deserialize)]
struct ExternalAccountResponse {
    id: String,
    #[serde(rename = "account_details")]
    #[allow(dead_code)]
    account_details: Vec<serde_json::Value>,
}

#[derive(Debug, Serialize)]
struct CreatePaymentOrderRequest {
    #[serde(rename = "type")]
    order_type: String,
    amount: i64,
    direction: String,
    originating_account_id: String,
    receiving_account_id: String,
    receiving_account_type: String,
    remittance_information: String,
    currency: String,
}

#[derive(Debug, Deserialize)]
pub struct PaymentOrderResponse {
    pub id: String,
    pub status: String,
}

pub async fn create_counterparty(
    details: &BankAccountDetails,
    customer_name: &str,
) -> Result<(String, String), AppError> {
    let client = mt_client()?;

    let request = CreateCounterpartyRequest {
        name: customer_name.to_string(),
        accounts: vec![CreateExternalAccountRequest {
            account_details: vec![ExternalAccountDetail {
                detail_type: details.account_type.clone(),
                account_number: details.account_number.clone(),
            }],
            routing_details: vec![RoutingDetail {
                routing_number: details.routing_number.clone(),
                routing_number_type: "aba".to_string(),
            }],
        }],
    };

    let resp = client
        .post(&format!("{}/counterparties", MT_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!("MT counterparty creation failed: HTTP {} - {}", status, body);
        return Err(AppError::ModernTreasuryError(format!(
            "Counterparty creation failed: HTTP {} - {}",
            status, body
        )));
    }

    let counterparty: CounterpartyResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("Failed to parse counterparty: {}", e)))?;

    let external_account_id = counterparty.accounts.first()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| counterparty.id.clone());

    info!(
        "Created MT counterparty {} ({}) with account {}",
        counterparty.id, counterparty.name, external_account_id
    );

    Ok((counterparty.id, external_account_id))
}

pub async fn create_transfer(
    internal_account_id: &str,
    counterparty_id: &str,
    rail: &PaymentRail,
    amount_cents: i64,
    description: &str,
) -> Result<PaymentOrderResponse, AppError> {
    let client = mt_client()?;

    let order_type = match rail {
        PaymentRail::Ach => "ach",
        PaymentRail::Wire => "wire",
        PaymentRail::Rtp | PaymentRail::FedNow => "rtp",
    };

    let request = CreatePaymentOrderRequest {
        order_type: order_type.to_string(),
        amount: amount_cents,
        direction: "credit".to_string(),
        originating_account_id: internal_account_id.to_string(),
        receiving_account_id: counterparty_id.to_string(),
        receiving_account_type: "external_account".to_string(),
        remittance_information: description.to_string(),
        currency: "USD".to_string(),
    };

    let resp = client
        .post(&format!("{}/payment_orders", MT_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(
            "MT payment order failed: HTTP {} - {}",
            status, body
        );
        return Err(AppError::ModernTreasuryError(format!(
            "Payment order failed: HTTP {} - {}",
            status, body
        )));
    }

    let order: PaymentOrderResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("Failed to parse payment order: {}", e)))?;

    info!("Created MT payment order {}", order.id);
    Ok(order)
}

pub async fn get_transfer_status(transfer_id: &str) -> Result<String, AppError> {
    let client = mt_client()?;

    let resp = client
        .get(&format!("{}/payment_orders/{}", MT_BASE_URL, transfer_id))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;

    let order: PaymentOrderResponse = resp
        .json()
        .await
        .map_err(|e| {
            AppError::ModernTreasuryError(format!("Failed to parse payment order status: {}", e))
        })?;

    Ok(order.status)
}

pub async fn get_payment_order_details(transfer_id: &str) -> Result<serde_json::Value, AppError> {
    let client = mt_client()?;
    let resp = client
        .get(&format!("{}/payment_orders/{}", MT_BASE_URL, transfer_id))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "Failed to fetch payment order: HTTP {} - {}", status, body
        )));
    }
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("Failed to parse: {}", e)))?;
    Ok(val)
}

pub async fn get_counterparty_details(counterparty_id: &str) -> Result<serde_json::Value, AppError> {
    let client = mt_client()?;
    let resp = client
        .get(&format!("{}/counterparties/{}", MT_BASE_URL, counterparty_id))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "Failed to fetch counterparty: HTTP {} - {}", status, body
        )));
    }
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("Failed to parse: {}", e)))?;
    Ok(val)
}