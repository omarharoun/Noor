use paybank_core::AppError;
use serde::{Deserialize, Serialize};
use tracing::info;

const PLAID_BASE_URL: &str = "https://sandbox.plaid.com";

fn client() -> Result<reqwest::Client, AppError> {
    reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Plaid client build error: {}", e)))
}

fn plaid_creds() -> Result<(String, String), AppError> {
    let client_id = std::env::var("PLAID_CLIENT_ID")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("PLAID_CLIENT_ID not set")))?;
    let secret = std::env::var("PLAID_SECRET")
        .map_err(|_| AppError::Internal(anyhow::anyhow!("PLAID_SECRET not set")))?;
    Ok((client_id, secret))
}

#[derive(Debug, Serialize)]
struct LinkTokenCreateRequest {
    client_id: String,
    secret: String,
    client_name: String,
    country_codes: Vec<String>,
    language: String,
    user: LinkUser,
    products: Vec<String>,
}

#[derive(Debug, Serialize)]
struct LinkUser {
    client_user_id: String,
}

#[derive(Debug, Deserialize)]
struct LinkTokenCreateResponse {
    link_token: String,
    expiration: String,
    #[allow(dead_code)]
    request_id: String,
}

#[derive(Debug, Serialize)]
struct ExchangeRequest {
    client_id: String,
    secret: String,
    public_token: String,
}

#[derive(Debug, Deserialize)]
struct ExchangeResponse {
    access_token: String,
    item_id: String,
    #[allow(dead_code)]
    request_id: String,
}

#[derive(Debug, Serialize)]
struct AuthGetRequest {
    client_id: String,
    secret: String,
    access_token: String,
}

#[derive(Debug, Deserialize)]
struct AuthGetResponse {
    accounts: Vec<PlaidAccount>,
    numbers: PlaidNumbers,
    #[allow(dead_code)]
    request_id: String,
}

#[derive(Debug, Deserialize)]
struct PlaidAccount {
    account_id: String,
    name: String,
    #[allow(dead_code)]
    official_name: Option<String>,
    subtype: Option<String>,
    #[serde(rename = "type")]
    #[allow(dead_code)]
    account_type: Option<String>,
    balances: PlaidBalances,
}

#[derive(Debug, Deserialize)]
struct PlaidBalances {
    available: Option<f64>,
    current: Option<f64>,
    iso_currency_code: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlaidNumbers {
    ach: Vec<PlaidAchNumber>,
}

#[derive(Debug, Deserialize)]
struct PlaidAchNumber {
    account_id: String,
    account: String,
    routing: String,
    #[allow(dead_code)]
    wire_routing: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BankAccountDetails {
    pub account_id: String,
    pub account_name: String,
    pub account_number: String,
    pub routing_number: String,
    pub account_type: String,
    pub subtype: String,
    pub available_balance: Option<f64>,
    pub current_balance: Option<f64>,
    pub iso_currency_code: Option<String>,
}

pub async fn create_link_token(session_id: &uuid::Uuid) -> Result<String, AppError> {
    let c = client()?;
    let (client_id, secret) = plaid_creds()?;

    let request = LinkTokenCreateRequest {
        client_id,
        secret,
        client_name: "PayBank".to_string(),
        country_codes: vec!["US".to_string()],
        language: "en".to_string(),
        user: LinkUser {
            client_user_id: session_id.to_string(),
        },
        products: vec!["auth".to_string()],
    };

    let resp = c
        .post(&format!("{}/link/token/create", PLAID_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::PlaidError(format!("link token request failed: {}", e)))?;

    let result: LinkTokenCreateResponse = resp
        .json()
        .await
        .map_err(|e| AppError::PlaidError(format!("failed to parse link token: {}", e)))?;

    info!("Created Plaid link token, expires {}", result.expiration);
    Ok(result.link_token)
}

pub async fn exchange_public_token(public_token: &str) -> Result<String, AppError> {
    let c = client()?;
    let (client_id, secret) = plaid_creds()?;

    let request = ExchangeRequest {
        client_id,
        secret,
        public_token: public_token.to_string(),
    };

    let resp = c
        .post(&format!("{}/item/public_token/exchange", PLAID_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::PlaidError(format!("token exchange failed: {}", e)))?;

    let result: ExchangeResponse = resp
        .json()
        .await
        .map_err(|e| AppError::PlaidError(format!("failed to parse exchange response: {}", e)))?;

    info!(
        "Exchanged public token for item {}",
        result.item_id
    );
    Ok(result.access_token)
}

pub async fn get_auth(access_token: &str) -> Result<Vec<BankAccountDetails>, AppError> {
    let c = client()?;
    let (client_id, secret) = plaid_creds()?;

    let request = AuthGetRequest {
        client_id,
        secret,
        access_token: access_token.to_string(),
    };

    let resp = c
        .post(&format!("{}/auth/get", PLAID_BASE_URL))
        .json(&request)
        .send()
        .await
        .map_err(|e| AppError::PlaidError(format!("auth get failed: {}", e)))?;

    let result: AuthGetResponse = resp
        .json()
        .await
        .map_err(|e| AppError::PlaidError(format!("failed to parse auth response: {}", e)))?;

    let ach_map: std::collections::HashMap<String, &PlaidAchNumber> = result
        .numbers
        .ach
        .iter()
        .map(|a| (a.account_id.clone(), a))
        .collect();

    let accounts = result
        .accounts
        .into_iter()
        .map(|a| {
            let ach = ach_map.get(&a.account_id);
            let subtype_value = a.subtype.clone().unwrap_or_else(|| "checking".to_string());
            BankAccountDetails {
                account_id: a.account_id,
                account_name: a.name,
                account_number: ach.map(|n| n.account.clone()).unwrap_or_default(),
                routing_number: ach.map(|n| n.routing.clone()).unwrap_or_default(),
                account_type: subtype_value.clone(),
                subtype: subtype_value,
                available_balance: a.balances.available,
                current_balance: a.balances.current,
                iso_currency_code: a.balances.iso_currency_code,
            }
        })
        .collect();

    Ok(accounts)
}