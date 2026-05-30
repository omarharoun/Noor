use paybank_core::{JournalEntry, LedgerAccount, LedgerPosting};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub struct LedgerRepo;

impl LedgerRepo {
    pub async fn create_journal_entry<'a>(
        tx: &mut Transaction<'a, Postgres>,
        session_id: Option<Uuid>,
        description: &str,
    ) -> Result<JournalEntry, sqlx::Error> {
        let entry = sqlx::query_as!(
            JournalEntry,
            r#"
            INSERT INTO journal_entries (session_id, description, status)
            VALUES ($1, $2, 'posted')
            RETURNING id, session_id, description, status, created_at
            "#,
            session_id,
            description
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(entry)
    }

    pub async fn create_posting<'a>(
        tx: &mut Transaction<'a, Postgres>,
        journal_entry_id: Uuid,
        account_id: Uuid,
        amount_cents: i64,
        direction: &str,
    ) -> Result<LedgerPosting, sqlx::Error> {
        let posting = sqlx::query_as!(
            LedgerPosting,
            r#"
            INSERT INTO ledger_postings (journal_entry_id, account_id, amount_cents, direction)
            VALUES ($1, $2, $3, $4)
            RETURNING id, journal_entry_id, account_id, amount_cents, direction, created_at
            "#,
            journal_entry_id,
            account_id,
            amount_cents,
            direction
        )
        .fetch_one(&mut **tx)
        .await?;
        Ok(posting)
    }

    pub async fn verify_balance<'c, E>(executor: E, journal_entry_id: Uuid) -> Result<bool, sqlx::Error>
    where
        E: sqlx::Executor<'c, Database = sqlx::Postgres>,
    {
        let records = sqlx::query!(
            r#"
            SELECT direction, SUM(amount_cents)::BIGINT as total
            FROM ledger_postings
            WHERE journal_entry_id = $1
            GROUP BY direction
            "#,
            journal_entry_id
        )
        .fetch_all(executor)
        .await?;

        let mut debits: i64 = 0;
        let mut credits: i64 = 0;

        for record in records {
            let total = record.total.unwrap_or(0);
            if record.direction.as_str() == "debit" {
                debits = total;
            } else if record.direction.as_str() == "credit" {
                credits = total;
            }
        }

        Ok(debits == credits && debits > 0)
    }

    pub async fn get_account<'c, E>(executor: E, name: &str, merchant_id: Uuid) -> Result<LedgerAccount, sqlx::Error>
    where
        E: sqlx::Executor<'c, Database = sqlx::Postgres> + Copy,
    {
        sqlx::query_as!(
            LedgerAccount,
            r#"SELECT id, name, type as "account_type!", merchant_id, currency, created_at
               FROM ledger_accounts WHERE name = $1 AND merchant_id = $2"#,
            name,
            merchant_id
        )
        .fetch_one(executor)
        .await
    }
}