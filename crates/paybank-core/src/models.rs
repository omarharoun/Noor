use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[serde(rename_all = "lowercase")]
#[sqlx(type_name = "text", rename_all = "lowercase")]
pub enum PaymentRail {
    FedNow,
    Rtp,
    Ach,
    Wire,
}

impl PaymentRail {
    pub fn choose(supports_fednow: bool, supports_rtp: bool, amount_cents: i64) -> Self {
        if amount_cents > 2_500_000 {
            PaymentRail::Wire
        } else if supports_fednow {
            PaymentRail::FedNow
        } else if supports_rtp {
            PaymentRail::Rtp
        } else {
            PaymentRail::Ach
        }
    }

    pub fn column_endpoint(&self) -> &'static str {
        match self {
            PaymentRail::Ach => "/transfers/ach",
            PaymentRail::Rtp | PaymentRail::FedNow => "/transfers/realtime",
            PaymentRail::Wire => "/transfers/wire",
        }
    }
}

impl std::fmt::Display for PaymentRail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaymentRail::FedNow => write!(f, "fednow"),
            PaymentRail::Rtp => write!(f, "rtp"),
            PaymentRail::Ach => write!(f, "ach"),
            PaymentRail::Wire => write!(f, "wire"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum PaymentStatus {
    Created,
    Pending,
    Processing,
    Submitted,
    Settled,
    Failed,
    Returned,
    Reversed,
    Refunded,
}

impl std::fmt::Display for PaymentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaymentStatus::Created => write!(f, "created"),
            PaymentStatus::Pending => write!(f, "pending"),
            PaymentStatus::Processing => write!(f, "processing"),
            PaymentStatus::Submitted => write!(f, "submitted"),
            PaymentStatus::Settled => write!(f, "settled"),
            PaymentStatus::Failed => write!(f, "failed"),
            PaymentStatus::Returned => write!(f, "returned"),
            PaymentStatus::Reversed => write!(f, "reversed"),
            PaymentStatus::Refunded => write!(f, "refunded"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum PaymentMethod {
    Ach,
    Eft,
    OpenBanking,
    Stablecoin,
}

impl std::fmt::Display for PaymentMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaymentMethod::Ach => write!(f, "ach"),
            PaymentMethod::Eft => write!(f, "eft"),
            PaymentMethod::OpenBanking => write!(f, "open_banking"),
            PaymentMethod::Stablecoin => write!(f, "stablecoin"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum SessionStatus {
    Pending,
    Authorized,
    Processing,
    Completed,
    Expired,
    // Distinct money-failure terminals. Never reuse `Expired` (an unused/timed-out
    // link) for a transfer outcome — operators and reconciliation must tell them apart.
    Failed,
    Returned,
    Reversed,
}

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            SessionStatus::Pending => "pending",
            SessionStatus::Authorized => "authorized",
            SessionStatus::Processing => "processing",
            SessionStatus::Completed => "completed",
            SessionStatus::Expired => "expired",
            SessionStatus::Failed => "failed",
            SessionStatus::Returned => "returned",
            SessionStatus::Reversed => "reversed",
        };
        write!(f, "{}", s)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, sqlx::Type)]
#[serde(rename_all = "snake_case")]
#[sqlx(type_name = "text", rename_all = "snake_case")]
pub enum EntryType {
    DebitPending,
    CreditPending,
    Settle,
    ReversePending,
    Refund,
    Fee,
    Payout,
}

impl std::fmt::Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntryType::DebitPending => write!(f, "debit_pending"),
            EntryType::CreditPending => write!(f, "credit_pending"),
            EntryType::Settle => write!(f, "settle"),
            EntryType::ReversePending => write!(f, "reverse_pending"),
            EntryType::Refund => write!(f, "refund"),
            EntryType::Fee => write!(f, "fee"),
            EntryType::Payout => write!(f, "payout"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Bank {
    pub id: String,
    pub name: String,
    pub routing_number: Option<String>,
    pub supports_fednow: bool,
    pub supports_rtp: bool,
    pub supports_wire: bool,
    pub logo_url: Option<String>,
    pub display_order: i32,
}

/// A named user belonging to a merchant (Phase 3a: multi-user + roles for
/// dual-control payouts). Roles: owner > admin > member.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct MerchantUser {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub name: String,
    pub role: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Merchant {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    // Secrets: never serialized to API responses (loaded via sqlx FromRow, not
    // serde). Defense-in-depth behind the hand-written redact_merchant projection.
    #[serde(skip_serializing)]
    pub password_hash: Option<String>,
    #[serde(skip_serializing)]
    pub api_key: String,
    pub webhook_url: Option<String>,
    pub business_name: Option<String>,
    pub business_type: Option<String>,
    pub registration_number: Option<String>,
    pub tax_id: Option<String>,
    pub industry_category: Option<String>,
    pub website_url: Option<String>,
    pub status: String,
    pub kyc_status: String,
    pub risk_level: Option<String>,
    pub onboarding_completed_at: Option<DateTime<Utc>>,
    // MT-backed bank account (Phase 1: merchant onboards their own account).
    #[serde(default)]
    pub mt_counterparty_id: Option<String>,
    #[serde(default)]
    pub mt_external_account_id: Option<String>,
    #[serde(default)]
    pub bank_account_status: Option<String>,
    #[serde(default)]
    pub bank_account_last4: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterMerchantRequest {
    pub name: String,
    pub email: String,
    pub password: String,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateMerchantRequest {
    pub business_name: Option<String>,
    pub business_type: Option<String>,
    pub registration_number: Option<String>,
    pub tax_id: Option<String>,
    pub industry_category: Option<String>,
    pub website_url: Option<String>,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MerchantResponse {
    pub id: Uuid,
    pub name: String,
    pub email: String,
    pub api_key: String,
    pub webhook_url: Option<String>,
    pub business_name: Option<String>,
    pub business_type: Option<String>,
    pub status: String,
    pub kyc_status: String,
    pub risk_level: Option<String>,
    pub onboarding_completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Payment {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub bank_id: Option<String>,
    pub idempotency_key: Option<String>,
    pub amount_cents: i64,
    pub currency: String,
    pub status: PaymentStatus,
    pub session_status: Option<SessionStatus>,
    pub method: PaymentMethod,
    pub rail: Option<PaymentRail>,
    pub provider: String,
    pub provider_reference: Option<String>,
    pub customer_name: Option<String>,
    pub customer_email: Option<String>,
    pub customer_phone: Option<String>,
    pub customer_address_line_1: Option<String>,
    pub customer_address_city: Option<String>,
    pub customer_address_state: Option<String>,
    pub customer_address_postal_code: Option<String>,
    pub customer_address_country_code: Option<String>,
    pub customer_account_number: Option<String>,
    pub customer_routing_number: Option<String>,
    pub customer_account_type: Option<String>,
    pub description: Option<String>,
    pub note: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub authorization_url: Option<String>,
    pub column_ref: Option<String>,
    pub column_counterparty_id: Option<String>,
    pub expires_at: Option<DateTime<Utc>>,
    pub settled_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentRequest {
    pub amount: i64,
    pub currency: String,
    pub method: PaymentMethod,
    pub customer_email: Option<String>,
    pub customer_name: Option<String>,
    pub customer_phone: Option<String>,
    pub customer_address_line_1: Option<String>,
    pub customer_address_city: Option<String>,
    pub customer_address_state: Option<String>,
    pub customer_address_postal_code: Option<String>,
    pub customer_address_country_code: Option<String>,
    pub customer_account_number: Option<String>,
    pub customer_routing_number: Option<String>,
    pub customer_account_type: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
    pub provider: Option<String>,
    pub bank_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatePaymentResponse {
    pub id: Uuid,
    pub status: PaymentStatus,
    pub amount: i64,
    pub currency: String,
    pub method: PaymentMethod,
    pub rail: PaymentRail,
    pub provider: String,
    pub authorization_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListPaymentsQuery {
    pub status: Option<String>,
    pub method: Option<String>,
    pub rail: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub page: Option<i64>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionRequest {
    pub merchant_id: Uuid,
    pub bank_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub note: Option<String>,
    pub customer_name: Option<String>,
    pub customer_email: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateSessionResponse {
    pub id: Uuid,
    pub status: PaymentStatus,
    pub session_status: SessionStatus,
    pub amount_cents: i64,
    pub currency: String,
    pub expires_at: DateTime<Utc>,
    pub qr_data: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentSession {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub bank_id: String,
    pub amount_cents: i64,
    pub currency: String,
    pub note: Option<String>,
    pub status: SessionStatus,
    pub rail_used: Option<PaymentRail>,
    pub column_ref: Option<String>,
    pub column_counterparty_id: Option<String>,
    pub customer_name: Option<String>,
    pub customer_email: Option<String>,
    pub customer_phone: Option<String>,
    pub customer_address_line_1: Option<String>,
    pub customer_address_city: Option<String>,
    pub customer_address_state: Option<String>,
    pub customer_address_postal_code: Option<String>,
    pub customer_address_country_code: Option<String>,
    // PII: never serialized out (admin/operator list + detail endpoints serialize
    // this struct). Internal DB reads use sqlx FromRow, not serde, so confirm/
    // initiate still see the value; the public pay endpoint builds its own JSON
    // exposing only a masked last-4.
    #[serde(skip_serializing)]
    pub customer_account_number: Option<String>,
    #[serde(skip_serializing)]
    pub customer_routing_number: Option<String>,
    pub customer_account_type: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Transaction {
    pub id: Uuid,
    pub session_id: Uuid,
    pub merchant_id: Uuid,
    pub amount_cents: i64,
    pub rail_used: PaymentRail,
    pub column_ref: Option<String>,
    pub reference: String,
    pub settled_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LedgerAccount {
    pub id: Uuid,
    pub name: String,
    pub account_type: String,
    pub merchant_id: Option<Uuid>,
    pub currency: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct JournalEntry {
    pub id: Uuid,
    pub session_id: Option<Uuid>,
    pub description: String,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct LedgerPosting {
    pub id: Uuid,
    pub journal_entry_id: Uuid,
    pub account_id: Uuid,
    pub amount_cents: i64,
    pub direction: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BalanceResponse {
    pub available: i64,
    pub pending: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebhookEvent {
    pub id: Uuid,
    pub merchant_id: Uuid,
    pub session_id: Option<Uuid>,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub sent_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WebhookDelivery {
    pub id: Uuid,
    pub webhook_event_id: Uuid,
    pub attempt_number: i32,
    pub request_headers: Option<serde_json::Value>,
    pub request_body: Option<serde_json::Value>,
    pub response_status: Option<i32>,
    pub response_body: Option<String>,
    pub error_message: Option<String>,
    pub delivered_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct IdempotencyKey {
    pub merchant_id: Uuid,
    pub idempotency_key: String,
    pub request_hash: String,
    pub response_status: Option<i32>,
    pub response_body: Option<serde_json::Value>,
    pub state: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentAttempt {
    pub id: Uuid,
    pub session_id: Uuid,
    pub rail: PaymentRail,
    pub provider_ref: Option<String>,
    pub request_payload: Option<serde_json::Value>,
    pub response_status: Option<i32>,
    pub response_body: Option<serde_json::Value>,
    pub error_message: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
