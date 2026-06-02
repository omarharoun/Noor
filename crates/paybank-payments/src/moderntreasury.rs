use base64::Engine;
use paybank_core::{AppError, PaymentRail};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::BankAccountDetails;

const MT_BASE_URL: &str = "https://app.moderntreasury.com/api";

fn mt_client() -> Result<Client, AppError> {
    let org_id = std::env::var("MODERN_TREASURY_ORG_ID")
        .map_err(|_| AppError::ModernTreasuryError("MODERN_TREASURY_ORG_ID not set".into()))?;
    let api_key = std::env::var("MODERN_TREASURY_API_KEY")
        .map_err(|_| AppError::ModernTreasuryError("MODERN_TREASURY_API_KEY not set".into()))?;

    Client::builder()
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
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))
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
        .post(format!("{}/counterparties", MT_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!(
            "MT counterparty creation failed: HTTP {} - {}",
            status, body
        );
        return Err(AppError::ModernTreasuryError(format!(
            "Counterparty creation failed: HTTP {} - {}",
            status, body
        )));
    }

    let counterparty: CounterpartyResponse = resp.json().await.map_err(|e| {
        AppError::ModernTreasuryError(format!("Failed to parse counterparty: {}", e))
    })?;

    let external_account_id = counterparty
        .accounts
        .first()
        .map(|a| a.id.clone())
        .unwrap_or_else(|| counterparty.id.clone());

    info!(
        "Created MT counterparty {} ({}) with account {}",
        counterparty.id, counterparty.name, external_account_id
    );

    Ok((counterparty.id, external_account_id))
}

// ── Merchant bank-account onboarding (Phase 1: direct entry, no Plaid) ──────
#[derive(Debug, Serialize)]
struct MtAcctDetail {
    account_number: String,
}
#[derive(Debug, Serialize)]
struct MtRoutingDetail {
    routing_number: String,
    routing_number_type: String,
    payment_type: String,
}
#[derive(Debug, Serialize)]
struct MtMerchantAccount {
    account_type: String,
    party_name: String,
    party_type: String,
    account_details: Vec<MtAcctDetail>,
    routing_details: Vec<MtRoutingDetail>,
}
#[derive(Debug, Serialize)]
struct MtMerchantCounterpartyReq {
    name: String,
    accounts: Vec<MtMerchantAccount>,
}
#[derive(Debug, Deserialize)]
struct ExtAcctStatusResp {
    #[serde(default)]
    verification_status: Option<String>,
}
#[derive(Debug, Serialize)]
struct VerifyReq {
    originating_account_id: String,
    payment_type: String,
}

/// Create an MT counterparty + external account from manually-entered bank
/// details. Returns (counterparty_id, external_account_id).
pub async fn create_merchant_bank_account(
    business_name: &str,
    account_holder: &str,
    account_type: &str,
    routing_number: &str,
    account_number: &str,
) -> Result<(String, String), AppError> {
    let client = mt_client()?;
    let req = MtMerchantCounterpartyReq {
        name: business_name.to_string(),
        accounts: vec![MtMerchantAccount {
            account_type: account_type.to_string(),
            party_name: account_holder.to_string(),
            party_type: "business".to_string(),
            account_details: vec![MtAcctDetail {
                account_number: account_number.to_string(),
            }],
            routing_details: vec![MtRoutingDetail {
                routing_number: routing_number.to_string(),
                routing_number_type: "aba".to_string(),
                payment_type: "ach".to_string(),
            }],
        }],
    };
    let resp = client
        .post(format!("{}/counterparties", MT_BASE_URL))
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "merchant counterparty create failed: HTTP {} - {}",
            status, body
        )));
    }
    let cp: CounterpartyResponse = resp.json().await.map_err(|e| {
        AppError::ModernTreasuryError(format!("parse merchant counterparty: {}", e))
    })?;
    let ext =
        cp.accounts.first().map(|a| a.id.clone()).ok_or_else(|| {
            AppError::ModernTreasuryError("MT returned no external account".into())
        })?;
    info!(
        "MT merchant counterparty {} external account {}",
        cp.id, ext
    );
    Ok((cp.id, ext))
}

/// Kick off ACH prenote verification for an external account. Returns the
/// verification_status MT reports (typically `pending_verification`).
pub async fn verify_external_account_prenote(
    external_account_id: &str,
) -> Result<String, AppError> {
    let client = mt_client()?;
    let originating = std::env::var("MT_INTERNAL_ACCOUNT_ID")
        .map_err(|_| AppError::ModernTreasuryError("MT_INTERNAL_ACCOUNT_ID not set".into()))?;
    let req = VerifyReq {
        originating_account_id: originating,
        payment_type: "ach".into(),
    };
    let resp = client
        .post(format!(
            "{}/external_accounts/{}/verify",
            MT_BASE_URL, external_account_id
        ))
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "external account verify failed: HTTP {} - {}",
            status, body
        )));
    }
    let s: ExtAcctStatusResp = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("parse verify: {}", e)))?;
    Ok(s.verification_status
        .unwrap_or_else(|| "pending_verification".into()))
}

/// Read the current verification_status of an external account from MT.
pub async fn get_external_account_status(external_account_id: &str) -> Result<String, AppError> {
    let client = mt_client()?;
    let resp = client
        .get(format!(
            "{}/external_accounts/{}",
            MT_BASE_URL, external_account_id
        ))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(AppError::ModernTreasuryError(format!(
            "get external account failed: HTTP {}",
            resp.status()
        )));
    }
    let s: ExtAcctStatusResp = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("parse external account: {}", e)))?;
    Ok(s.verification_status.unwrap_or_else(|| "unverified".into()))
}

// ── Invoicing (Phase 2: MT Invoices issues + hosts the pay page) ────────────
#[derive(Debug, Serialize)]
struct MtInvoiceLineItem {
    name: String,
    quantity: i64,
    unit_amount: i64,
}
#[derive(Debug, Serialize)]
struct MtContactDetail {
    contact_identifier: String,
    contact_identifier_type: String,
}
#[derive(Debug, Serialize)]
struct MtCounterpartyEmailReq {
    name: String,
    email: String,
}
#[derive(Debug, Serialize)]
struct MtInvoiceReq {
    originating_account_id: String,
    counterparty_id: String,
    currency: String,
    auto_advance: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    due_date: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    contact_details: Vec<MtContactDetail>,
    invoice_line_items: Vec<MtInvoiceLineItem>,
}
#[derive(Debug, Deserialize)]
pub struct InvoiceResponse {
    pub id: String,
    pub status: String,
    pub total_amount: i64,
    #[serde(default)]
    pub number: Option<String>,
    #[serde(default)]
    pub hosted_url: Option<String>,
}

/// A single invoice line item (name, quantity, unit amount in cents).
pub struct LineItem {
    pub name: String,
    pub quantity: i64,
    pub unit_amount: i64,
}

/// Result of issuing an invoice: the MT ids + hosted pay URL.
pub struct CreatedInvoice {
    pub mt_invoice_id: String,
    pub mt_counterparty_id: String,
    pub number: Option<String>,
    pub status: String,
    pub total_amount: i64,
    pub hosted_url: Option<String>,
}

/// Create a counterparty for the customer and issue an MT invoice
/// (`auto_advance` posts it immediately so the hosted pay page is live).
pub async fn create_invoice(
    customer_name: &str,
    customer_email: &str,
    currency: &str,
    due_date: Option<&str>,
    description: Option<&str>,
    line_items: &[LineItem],
) -> Result<CreatedInvoice, AppError> {
    let client = mt_client()?;
    let originating = std::env::var("MT_INTERNAL_ACCOUNT_ID")
        .map_err(|_| AppError::ModernTreasuryError("MT_INTERNAL_ACCOUNT_ID not set".into()))?;

    // Customer counterparty (needs an email for the hosted invoice).
    let cp_resp = client
        .post(format!("{}/counterparties", MT_BASE_URL))
        .json(&MtCounterpartyEmailReq {
            name: customer_name.to_string(),
            email: customer_email.to_string(),
        })
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !cp_resp.status().is_success() {
        let st = cp_resp.status();
        let b = cp_resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "invoice counterparty failed: HTTP {} - {}",
            st, b
        )));
    }
    let cp: CounterpartyResponse = cp_resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("parse counterparty: {}", e)))?;

    let req = MtInvoiceReq {
        originating_account_id: originating,
        counterparty_id: cp.id.clone(),
        currency: currency.to_string(),
        auto_advance: true,
        due_date: due_date.map(|s| s.to_string()),
        description: description.map(|s| s.to_string()),
        contact_details: vec![MtContactDetail {
            contact_identifier: customer_email.to_string(),
            contact_identifier_type: "email".to_string(),
        }],
        invoice_line_items: line_items
            .iter()
            .map(|li| MtInvoiceLineItem {
                name: li.name.clone(),
                quantity: li.quantity,
                unit_amount: li.unit_amount,
            })
            .collect(),
    };
    let resp = client
        .post(format!("{}/invoices", MT_BASE_URL))
        .json(&req)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let st = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "invoice create failed: HTTP {} - {}",
            st, b
        )));
    }
    let inv: InvoiceResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("parse invoice: {}", e)))?;
    info!(
        "MT invoice {} ({:?}) status {}",
        inv.id, inv.number, inv.status
    );
    Ok(CreatedInvoice {
        mt_invoice_id: inv.id,
        mt_counterparty_id: cp.id,
        number: inv.number,
        status: inv.status,
        total_amount: inv.total_amount,
        hosted_url: inv.hosted_url,
    })
}

/// Fetch the current status of an MT invoice (e.g. unpaid → paid).
pub async fn get_invoice_status(invoice_id: &str) -> Result<String, AppError> {
    let client = mt_client()?;
    let resp = client
        .get(format!("{}/invoices/{}", MT_BASE_URL, invoice_id))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        return Err(AppError::ModernTreasuryError(format!(
            "get invoice failed: HTTP {}",
            resp.status()
        )));
    }
    let inv: InvoiceResponse = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("parse invoice: {}", e)))?;
    Ok(inv.status)
}

/// Send a payout: a payment_order (direction=credit) from the platform internal
/// account to a payee's external account, over the chosen rail.
pub async fn create_payout(
    external_account_id: &str,
    rail: &PaymentRail,
    amount_cents: i64,
    description: &str,
    idempotency_key: &str,
) -> Result<PaymentOrderResponse, AppError> {
    let client = mt_client()?;
    let originating = std::env::var("MT_INTERNAL_ACCOUNT_ID")
        .map_err(|_| AppError::ModernTreasuryError("MT_INTERNAL_ACCOUNT_ID not set".into()))?;
    let order_type = match rail {
        PaymentRail::Ach => "ach",
        PaymentRail::Wire => "wire",
        PaymentRail::Rtp | PaymentRail::FedNow => "rtp",
    };
    let request = CreatePaymentOrderRequest {
        order_type: order_type.to_string(),
        amount: amount_cents,
        direction: "credit".to_string(),
        originating_account_id: originating,
        receiving_account_id: external_account_id.to_string(),
        receiving_account_type: "external_account".to_string(),
        remittance_information: description.to_string(),
        currency: "USD".to_string(),
    };
    let resp = client
        .post(format!("{}/payment_orders", MT_BASE_URL))
        .header("Idempotency-Key", idempotency_key)
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let st = resp.status();
        let b = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "payout failed: HTTP {} - {}",
            st, b
        )));
    }
    resp.json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("parse payout: {}", e)))
}

pub async fn create_transfer(
    internal_account_id: &str,
    counterparty_id: &str,
    rail: &PaymentRail,
    amount_cents: i64,
    description: &str,
    idempotency_key: &str,
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

    // Idempotency-Key makes a retried POST reuse the same payment order instead
    // of creating a duplicate transfer (MT honors this header).
    let resp = client
        .post(format!("{}/payment_orders", MT_BASE_URL))
        .header("Idempotency-Key", idempotency_key)
        .json(&request)
        .send()
        .await
        // A transport error (timeout/dropped connection) is AMBIGUOUS: the order
        // may have been created server-side. Never treat it as a definitive fail.
        .map_err(|e| AppError::ProviderAmbiguous(format!("MT payment order send failed: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        warn!("MT payment order failed: HTTP {} - {}", status, body);
        let msg = format!("Payment order failed: HTTP {} - {}", status, body);
        // 5xx (and 429) are ambiguous/retryable; 4xx is a definitive rejection.
        return Err(if status.is_server_error() || status.as_u16() == 429 {
            AppError::ProviderAmbiguous(msg)
        } else {
            AppError::ModernTreasuryError(msg)
        });
    }

    let order: PaymentOrderResponse = resp.json().await.map_err(|e| {
        AppError::ModernTreasuryError(format!("Failed to parse payment order: {}", e))
    })?;

    info!("Created MT payment order {}", order.id);
    Ok(order)
}

pub async fn get_transfer_status(transfer_id: &str) -> Result<String, AppError> {
    let client = mt_client()?;

    let resp = client
        .get(format!("{}/payment_orders/{}", MT_BASE_URL, transfer_id))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;

    let order: PaymentOrderResponse = resp.json().await.map_err(|e| {
        AppError::ModernTreasuryError(format!("Failed to parse payment order status: {}", e))
    })?;

    Ok(order.status)
}

pub async fn get_payment_order_details(transfer_id: &str) -> Result<serde_json::Value, AppError> {
    let client = mt_client()?;
    let resp = client
        .get(format!("{}/payment_orders/{}", MT_BASE_URL, transfer_id))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "Failed to fetch payment order: HTTP {} - {}",
            status, body
        )));
    }
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("Failed to parse: {}", e)))?;
    Ok(val)
}

pub async fn get_counterparty_details(
    counterparty_id: &str,
) -> Result<serde_json::Value, AppError> {
    let client = mt_client()?;
    let resp = client
        .get(format!(
            "{}/counterparties/{}",
            MT_BASE_URL, counterparty_id
        ))
        .send()
        .await
        .map_err(|e| AppError::ModernTreasuryError(e.to_string()))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::ModernTreasuryError(format!(
            "Failed to fetch counterparty: HTTP {} - {}",
            status, body
        )));
    }
    let val: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AppError::ModernTreasuryError(format!("Failed to parse: {}", e)))?;
    Ok(val)
}
