use anyhow::Result;
use sqlx::PgPool;

use paybank_core::Bank;

pub async fn list_banks(pool: &PgPool) -> Result<Vec<Bank>> {
    let banks = sqlx::query_as!(
        Bank,
        r#"
        SELECT id, name, routing_number, supports_fednow, supports_rtp, supports_wire, logo_url, display_order
        FROM banks ORDER BY display_order ASC
        "#
    )
    .fetch_all(pool)
    .await?;
    Ok(banks)
}

pub async fn get_bank(pool: &PgPool, id: &str) -> Result<Option<Bank>> {
    let bank = sqlx::query_as!(
        Bank,
        r#"
        SELECT id, name, routing_number, supports_fednow, supports_rtp, supports_wire, logo_url, display_order
        FROM banks WHERE id = $1
        "#,
        id
    )
    .fetch_optional(pool)
    .await?;
    Ok(bank)
}