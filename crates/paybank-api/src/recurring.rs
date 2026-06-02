//! Recurring-invoice worker: turns due `recurring_invoices` templates into real
//! (Modern Treasury) invoices on schedule, advancing `next_run` each cycle.

use crate::state::AppState;
use sqlx::Row;
use std::time::Duration;
use uuid::Uuid;

const TICK_SECS: u64 = 3600; // hourly is plenty for daily/weekly/monthly cadences

pub fn spawn(state: AppState) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = tick(&state).await {
                tracing::warn!(error = %e, "recurring-invoice tick failed");
            }
            tokio::time::sleep(Duration::from_secs(TICK_SECS)).await;
        }
    });
}

async fn tick(state: &AppState) -> Result<(), sqlx::Error> {
    let pool = &state.db.pool;
    // Claim due templates by advancing next_run atomically (date + int = date),
    // so a restart or a second instance can't issue the same cycle twice.
    let due = sqlx::query(
        "UPDATE recurring_invoices SET next_run = next_run + interval_days, updated_at = NOW() \
         WHERE active AND next_run <= CURRENT_DATE \
         RETURNING id, merchant_id, customer_name, customer_email, amount_cents, description",
    )
    .fetch_all(pool)
    .await?;

    for row in due {
        let rec_id: Uuid = row.try_get("id")?;
        let merchant_id: Uuid = row.try_get("merchant_id")?;
        let name: String = row.try_get("customer_name")?;
        let email: String = row.try_get("customer_email")?;
        let amount: i64 = row.try_get("amount_cents")?;
        let desc: Option<String> = row.try_get("description").ok().flatten();

        let due_str = (chrono::Utc::now() + chrono::Duration::days(30))
            .format("%Y-%m-%d")
            .to_string();
        let items = vec![paybank_payments::InvoiceLineItem {
            name: desc.clone().unwrap_or_else(|| "Invoice".into()),
            quantity: 1,
            unit_amount: amount,
        }];
        match paybank_payments::create_invoice(
            &name,
            &email,
            "USD",
            Some(&due_str),
            desc.as_deref(),
            &items,
        )
        .await
        {
            Ok(created) => {
                let inv_id = Uuid::new_v4();
                let _ = sqlx::query(
                    "INSERT INTO invoices (id, merchant_id, mt_invoice_id, mt_counterparty_id, \
                     number, customer_name, customer_email, description, amount_cents, currency, \
                     status, hosted_url) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'USD',$10,$11)",
                )
                .bind(inv_id)
                .bind(merchant_id)
                .bind(&created.mt_invoice_id)
                .bind(&created.mt_counterparty_id)
                .bind(&created.number)
                .bind(&name)
                .bind(&email)
                .bind(&desc)
                .bind(created.total_amount)
                .bind(&created.status)
                .bind(&created.hosted_url)
                .execute(pool)
                .await;
                let _ = sqlx::query("UPDATE recurring_invoices SET last_invoice_id=$1 WHERE id=$2")
                    .bind(inv_id)
                    .bind(rec_id)
                    .execute(pool)
                    .await;
                tracing::info!(recurring = %rec_id, invoice = %inv_id, "issued recurring invoice");
            }
            Err(e) => {
                tracing::warn!(recurring = %rec_id, error = %e, "recurring invoice issue failed; retries next cycle");
            }
        }
    }
    Ok(())
}
