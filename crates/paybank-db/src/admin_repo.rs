use anyhow::Result;
use paybank_core::{Merchant, PaymentSession, WebhookEvent};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub today_total_transactions: i64,
    pub today_completed: i64,
    pub pending_count: i64,
    pub total_volume_cents: i64,
    pub by_rail: Vec<RailStats>,
    pub last_7_days: Vec<DailyStats>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RailStats {
    pub rail_used: Option<String>,
    pub count: i64,
    pub volume: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct DailyStats {
    pub date: chrono::NaiveDate,
    pub count: i64,
    pub volume: i64,
}

pub async fn get_dashboard_stats(pool: &PgPool) -> Result<DashboardStats> {
    let today_total: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::int8 FROM payment_sessions WHERE created_at >= CURRENT_DATE",
    )
    .fetch_one(pool)
    .await?;

    let today_completed: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)::int8 FROM payment_sessions WHERE status = 'completed' AND created_at >= CURRENT_DATE",
    )
    .fetch_one(pool)
    .await?;

    let pending_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*)::int8 FROM payment_sessions WHERE status = 'pending'")
            .fetch_one(pool)
            .await?;

    let total_volume_cents: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(amount_cents)::int8, 0::int8) FROM payment_sessions WHERE status = 'completed'",
    )
    .fetch_one(pool)
    .await?;

    let by_rail: Vec<RailStats> = sqlx::query_as(
        r#"SELECT rail_used, COUNT(*)::int8 as count, COALESCE(SUM(amount_cents)::int8, 0::int8) as volume
           FROM payment_sessions WHERE rail_used IS NOT NULL
           GROUP BY rail_used"#,
    )
    .fetch_all(pool)
    .await?;

    let last_7_days: Vec<DailyStats> = sqlx::query_as(
        r#"SELECT DATE(created_at) as date, COUNT(*)::int8 as count, COALESCE(SUM(amount_cents)::int8, 0::int8) as volume
           FROM payment_sessions WHERE created_at >= NOW() - INTERVAL '7 days'
           GROUP BY DATE(created_at) ORDER BY date"#,
    )
    .fetch_all(pool)
    .await?;

    Ok(DashboardStats {
        today_total_transactions: today_total,
        today_completed,
        pending_count,
        total_volume_cents,
        by_rail,
        last_7_days,
    })
}

pub async fn list_all_sessions(
    pool: &PgPool,
    limit: i64,
    offset: i64,
    status: Option<&str>,
    rail: Option<&str>,
    date_from: Option<chrono::NaiveDate>,
    date_to: Option<chrono::NaiveDate>,
) -> Result<(Vec<PaymentSession>, i64)> {
    let mut where_clauses: Vec<String> = Vec::new();
    let mut idx = 1u32;

    let status_val = status;
    let rail_val = rail;
    let date_from_val = date_from;
    let date_to_val = date_to;

    if status_val.is_some() {
        where_clauses.push(format!("ps.status = ${}", idx));
        idx += 1;
    }
    if rail_val.is_some() {
        where_clauses.push(format!("ps.rail_used = ${}", idx));
        idx += 1;
    }
    if date_from_val.is_some() {
        where_clauses.push(format!("ps.created_at >= ${}::date", idx));
        idx += 1;
    }
    if date_to_val.is_some() {
        where_clauses.push(format!(
            "ps.created_at <= ${}::date + interval '1 day'",
            idx
        ));
        idx += 1;
    }

    let where_sql = if where_clauses.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", where_clauses.join(" AND "))
    };

    let count_sql = format!(
        "SELECT COUNT(*)::int8 FROM payment_sessions ps {}",
        where_sql
    );

    let limit_offset_sql = format!(" LIMIT ${} OFFSET ${}", idx, idx + 1);
    let query_sql = format!(
        r#"SELECT ps.id, ps.merchant_id, ps.bank_id, ps.amount_cents, ps.currency, ps.note,
                  ps.status, ps.rail_used,
                  ps.column_ref, ps.column_counterparty_id,
                  ps.customer_name, ps.customer_email, ps.customer_phone,
                  ps.customer_address_line_1, ps.customer_address_city, ps.customer_address_state,
                  ps.customer_address_postal_code, ps.customer_address_country_code,
                  ps.customer_account_number, ps.customer_routing_number, ps.customer_account_type,
                  ps.expires_at, ps.created_at, ps.updated_at
           FROM payment_sessions ps {} ORDER BY ps.created_at DESC{}"#,
        where_sql, limit_offset_sql
    );

    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql);
    let mut data_query = sqlx::query_as::<_, PaymentSession>(&query_sql);

    if let Some(v) = status_val {
        count_query = count_query.bind(v);
        data_query = data_query.bind(v);
    }
    if let Some(v) = rail_val {
        count_query = count_query.bind(v);
        data_query = data_query.bind(v);
    }
    if let Some(v) = date_from_val {
        count_query = count_query.bind(v);
        data_query = data_query.bind(v);
    }
    if let Some(v) = date_to_val {
        count_query = count_query.bind(v);
        data_query = data_query.bind(v);
    }

    data_query = data_query.bind(limit);
    data_query = data_query.bind(offset);

    let total = count_query.fetch_one(pool).await?;
    let sessions = data_query.fetch_all(pool).await?;
    Ok((sessions, total))
}

pub async fn list_merchants(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<(Vec<Merchant>, i64)> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*)::int8 FROM merchants")
        .fetch_one(pool)
        .await?;

    let merchants: Vec<Merchant> =
        sqlx::query_as(r#"SELECT * FROM merchants ORDER BY created_at DESC LIMIT $1 OFFSET $2"#)
            .bind(limit)
            .bind(offset)
            .fetch_all(pool)
            .await?;

    Ok((merchants, total))
}

pub async fn create_merchant(
    pool: &PgPool,
    name: &str,
    email: &str,
    api_key: &str,
) -> Result<Merchant> {
    let merchant: Merchant = sqlx::query_as(
        r#"INSERT INTO merchants (name, email, api_key)
           VALUES ($1, $2, $3)
           RETURNING *"#,
    )
    .bind(name)
    .bind(email)
    .bind(api_key)
    .fetch_one(pool)
    .await?;

    // Provision the merchant's chart of accounts so ledger postings and balance
    // queries work for them from day one.
    crate::ledger_repo::LedgerRepo::ensure_merchant_accounts(pool, merchant.id).await?;

    Ok(merchant)
}

#[allow(clippy::too_many_arguments)] // one parameter per updatable column
pub async fn update_merchant(
    pool: &PgPool,
    id: Uuid,
    business_name: Option<&str>,
    business_type: Option<&str>,
    tax_id: Option<&str>,
    industry_category: Option<&str>,
    website_url: Option<&str>,
    webhook_url: Option<&str>,
) -> Result<Merchant> {
    let merchant: Merchant = sqlx::query_as(
        r#"UPDATE merchants SET
               business_name = COALESCE($2, business_name),
               business_type = COALESCE($3, business_type),
               tax_id = COALESCE($4, tax_id),
               industry_category = COALESCE($5, industry_category),
               website_url = COALESCE($6, website_url),
               webhook_url = COALESCE($7, webhook_url),
               updated_at = NOW()
           WHERE id = $1
           RETURNING *"#,
    )
    .bind(id)
    .bind(business_name)
    .bind(business_type)
    .bind(tax_id)
    .bind(industry_category)
    .bind(website_url)
    .bind(webhook_url)
    .fetch_one(pool)
    .await?;
    Ok(merchant)
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SettlementRow {
    pub merchant_id: Uuid,
    pub merchant_name: Option<String>,
    pub total_count: i64,
    pub total_volume_cents: i64,
    pub last_payment_date: Option<chrono::DateTime<chrono::Utc>>,
}

pub async fn get_settlements(pool: &PgPool) -> Result<Vec<SettlementRow>> {
    let rows: Vec<SettlementRow> = sqlx::query_as(
        r#"SELECT ps.merchant_id, m.name as merchant_name,
                  COUNT(*)::int8 as total_count,
                  COALESCE(SUM(ps.amount_cents)::int8, 0::int8) as total_volume_cents,
                  MAX(ps.created_at) as last_payment_date
           FROM payment_sessions ps
           JOIN merchants m ON m.id = ps.merchant_id
           WHERE ps.status = 'completed'
           GROUP BY ps.merchant_id, m.name
           ORDER BY total_volume_cents DESC"#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn list_webhook_events(
    pool: &PgPool,
    limit: i64,
    offset: i64,
) -> Result<(Vec<WebhookEvent>, i64)> {
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*)::int8 FROM webhook_events")
        .fetch_one(pool)
        .await?;

    let events: Vec<WebhookEvent> = sqlx::query_as(
        r#"SELECT id, merchant_id, session_id, event_type, payload, status, attempts,
                  next_retry_at, sent_at, created_at
           FROM webhook_events ORDER BY created_at DESC LIMIT $1 OFFSET $2"#,
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?;

    Ok((events, total))
}

pub async fn get_merchant(pool: &PgPool, id: Uuid) -> Result<Option<Merchant>> {
    let merchant: Option<Merchant> = sqlx::query_as(r#"SELECT * FROM merchants WHERE id = $1"#)
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(merchant)
}

/// Aggregate "is anything wrong right now" counters for the internal health view.
#[derive(Debug, Serialize)]
pub struct IssueSummary {
    pub stuck_processing: i64,
    pub failed_sessions: i64,
    pub returned_sessions: i64,
    pub reversed_sessions: i64,
    pub webhooks_failed: i64,
    pub webhooks_exhausted: i64,
    pub unbalanced_journal_entries: i64,
}

pub async fn get_issue_summary(pool: &PgPool) -> Result<IssueSummary> {
    let scalar =
        |sql: &'static str| async move { sqlx::query_scalar::<_, i64>(sql).fetch_one(pool).await };
    let stuck_processing = scalar(
        "SELECT COUNT(*)::int8 FROM payment_sessions WHERE status='processing' AND updated_at < NOW() - interval '5 minutes'",
    ).await?;
    let failed_sessions =
        scalar("SELECT COUNT(*)::int8 FROM payment_sessions WHERE status='failed'").await?;
    let returned_sessions =
        scalar("SELECT COUNT(*)::int8 FROM payment_sessions WHERE status='returned'").await?;
    let reversed_sessions =
        scalar("SELECT COUNT(*)::int8 FROM payment_sessions WHERE status='reversed'").await?;
    let webhooks_failed =
        scalar("SELECT COUNT(*)::int8 FROM webhook_events WHERE status='failed'").await?;
    let webhooks_exhausted =
        scalar("SELECT COUNT(*)::int8 FROM webhook_events WHERE status='exhausted'").await?;
    // Ledger integrity: every journal entry must have debits == credits. Any row
    // here is a real accounting discrepancy that needs investigation.
    let unbalanced_journal_entries = scalar(
        r#"SELECT COUNT(*)::int8 FROM (
               SELECT journal_entry_id
               FROM ledger_postings
               GROUP BY journal_entry_id
               HAVING COALESCE(SUM(CASE WHEN direction='debit' THEN amount_cents ELSE 0 END),0)
                    <> COALESCE(SUM(CASE WHEN direction='credit' THEN amount_cents ELSE 0 END),0)
           ) q"#,
    )
    .await?;

    Ok(IssueSummary {
        stuck_processing,
        failed_sessions,
        returned_sessions,
        reversed_sessions,
        webhooks_failed,
        webhooks_exhausted,
        unbalanced_journal_entries,
    })
}
