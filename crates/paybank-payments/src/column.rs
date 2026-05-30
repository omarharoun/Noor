use base64::Engine;
use paybank_core::{AppError, PaymentRail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::plaid::BankAccountDetails;

const COLUMN_BASE_URL: &str = "https://api.column.com";

fn client() -> Result<Client, AppError> {
    let api_key = std::env::var("COLUMN_API_KEY")
        .map_err(|_| AppError::ColumnError("COLUMN_API_KEY not set".into()))?;
    Ok(Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .default_headers({
            let mut h = reqwest::header::HeaderMap::new();
            let mut auth = reqwest::header::HeaderValue::from_str(&format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(format!(":{}", api_key))
            ))
            .map_err(|e| AppError::ColumnError(e.to_string()))?;
            auth.set_sensitive(true);
            h.insert(reqwest::header::AUTHORIZATION, auth);
            h.insert(
                reqwest::header::CONTENT_TYPE,
                reqwest::header::HeaderValue::from_static("application/json"),
            );
            h
        })
        .build()
        .map_err(|e| AppError::ColumnError(e.to_string()))?)
}

#[derive(Debug, Serialize)]
pub struct CustomerAddress {
    pub line_1: String,
    pub city: String,
    pub state: String,
    pub postal_code: String,
    pub country: String,
}

#[derive(Debug, Serialize)]
struct CreateEntityRequest {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    address: Option<CustomerAddress>,
}

#[derive(Debug, Deserialize)]
struct EntityResponse {
    id: String,
    name: String,
}

#[derive(Debug, Serialize)]
struct CreateAccountRequest {
    entity_id: String,
    account_number: String,
    routing_number: String,
    account_type: String,
}

#[derive(Debug, Deserialize)]
struct AccountResponse {
    #[allow(dead_code)]
    id: String,
}

#[derive(Debug, Serialize)]
struct CreateTransferRequest {
    merchant_account_id: String,
    counterparty_id: String,
    amount_cents: i64,
    currency: String,
    description: String,
    #[serde(rename = "type")]
    transfer_type: String,
}

#[derive(Debug, Deserialize)]
pub struct TransferResponse {
    pub id: String,
    pub status: String,
}

pub async fn create_counterparty(
    details: &BankAccountDetails,
    customer_name: &str,
    address: &CustomerAddress,
) -> Result<String, AppError> {
    let c = client()?;

    let create_entity = CreateEntityRequest {
        name: customer_name.to_string(),
        address: Some(CustomerAddress {
            line_1: address.line_1.clone(),
            city: address.city.clone(),
            state: address.state.clone(),
            postal_code: address.postal_code.clone(),
            country: address.country.clone(),
        }),
    };

    let entity_resp = c
        .post(&format!("{}/entities", COLUMN_BASE_URL))
        .json(&create_entity)
        .send()
        .await
        .map_err(|e| AppError::ColumnError(e.to_string()))?;

    let entity: EntityResponse = entity_resp
        .json()
        .await
        .map_err(|e| AppError::ColumnError(format!("Failed to parse entity: {}", e)))?;

    info!("Created Column entity {}: {}", entity.id, entity.name);

    let create_account = CreateAccountRequest {
        entity_id: entity.id.clone(),
        account_number: details.account_number.clone(),
        routing_number: details.routing_number.clone(),
        account_type: details.account_type.clone(),
    };

    let acct_resp = c
        .post(&format!("{}/accounts", COLUMN_BASE_URL))
        .json(&create_account)
        .send()
        .await
        .map_err(|e| AppError::ColumnError(e.to_string()))?;

    let _acct: AccountResponse = acct_resp
        .json()
        .await
        .map_err(|e| AppError::ColumnError(format!("Failed to parse account: {}", e)))?;

    Ok(entity.id)
}

pub async fn create_transfer(
    merchant_account_id: &str,
    counterparty_id: &str,
    rail: &PaymentRail,
    amount_cents: i64,
    description: &str,
) -> Result<TransferResponse, AppError> {
    let c = client()?;

    let request = CreateTransferRequest {
        merchant_account_id: merchant_account_id.to_string(),
        counterparty_id: counterparty_id.to_string(),
        amount_cents,
        currency: "USD".to_string(),
        description: description.to_string(),
        transfer_type: match rail {
            PaymentRail::Ach => "ach".to_string(),
            PaymentRail::Wire => "wire".to_string(),
            PaymentRail::Rtp => "rtp".to_string(),
            PaymentRail::FedNow => "fednow".to_string(),
        },
    };

    let resp = c
        .post(&format!("{}/transfers", COLUMN_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::ColumnError(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!("Column transfer failed: HTTP {} - {}", status, body);
        return Err(AppError::ColumnError(format!(
            "Transfer failed: HTTP {} - {}",
            status, body
        )));
    }

    let transfer: TransferResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ColumnError(format!("Failed to parse transfer: {}", e)))?;

    info!("Created Column transfer {}", transfer.id);
    Ok(transfer)
}

pub async fn get_transfer_status(transfer_id: &str) -> Result<String, AppError> {
    let c = client()?;

    let resp = c
        .get(&format!("{}/transfers/{}", COLUMN_BASE_URL, transfer_id))
        .send()
        .await
        .map_err(|e| AppError::ColumnError(e.to_string()))?;

    #[derive(Deserialize)]
    struct TransferStatusResponse {
        status: String,
    }

    let ts: TransferStatusResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ColumnError(format!("Failed to parse transfer status: {}", e)))?;

    Ok(ts.status)
}