use paybank_core::{JournalEntry, LedgerAccount, LedgerPosting};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

/// The four ledger accounts every merchant gets. Mirrors the demo merchant's
/// seed in migrations/002. (name, type)
const MERCHANT_ACCOUNTS: [(&str, &str); 4] = [
    ("Cash", "asset"),
    ("Receivables", "asset"),
    ("Revenue", "revenue"),
    ("Settlement", "liability"),
];

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

    /// Idempotently ensure a merchant has its standard chart of accounts. Safe to
    /// call repeatedly (e.g. on merchant creation and lazily before posting).
    pub async fn ensure_merchant_accounts(pool: &PgPool, merchant_id: Uuid) -> Result<(), sqlx::Error> {
        for (name, ty) in MERCHANT_ACCOUNTS {
            sqlx::query(
                r#"INSERT INTO ledger_accounts (name, type, merchant_id, currency)
                   SELECT $1, $2, $3, 'USD'
                   WHERE NOT EXISTS (
                       SELECT 1 FROM ledger_accounts WHERE merchant_id = $3 AND name = $1
                   )"#,
            )
            .bind(name)
            .bind(ty)
            .bind(merchant_id)
            .execute(pool)
            .await?;
        }
        Ok(())
    }

    /// Record a balanced double-entry journal for a settled payment IN to the
    /// merchant: debit Cash (asset up, funds received), credit Settlement
    /// (liability up, owed to merchant). Provisions accounts if missing and
    /// rejects an unbalanced entry. The whole journal commits atomically.
    pub async fn record_settlement(
        pool: &PgPool,
        merchant_id: Uuid,
        session_id: Uuid,
        amount_cents: i64,
    ) -> Result<(), sqlx::Error> {
        Self::ensure_merchant_accounts(pool, merchant_id).await?;
        let cash = Self::get_account(pool, "Cash", merchant_id).await?;
        let settlement = Self::get_account(pool, "Settlement", merchant_id).await?;

        let mut tx = pool.begin().await?;
        let entry = Self::create_journal_entry(
            &mut tx,
            Some(session_id),
            &format!("Settlement for session {}", session_id),
        )
        .await?;
        Self::create_posting(&mut tx, entry.id, cash.id, amount_cents, "debit").await?;
        Self::create_posting(&mut tx, entry.id, settlement.id, amount_cents, "credit").await?;

        if !Self::verify_balance(&mut *tx, entry.id).await? {
            tx.rollback().await?;
            return Err(sqlx::Error::Protocol(
                "ledger entry does not balance; refusing to post".into(),
            ));
        }
        tx.commit().await?;
        Ok(())
    }

    /// Compensating reversal of a prior settlement (ACH return / reversal):
    /// debit Settlement (reduce what we owe the merchant), credit Cash (funds
    /// clawed back). Net effect lowers the merchant's available balance by
    /// `amount_cents`. Callers MUST guard this behind a CAS status transition so
    /// a replayed webhook can't reverse twice.
    pub async fn record_reversal(
        pool: &PgPool,
        merchant_id: Uuid,
        session_id: Uuid,
        amount_cents: i64,
    ) -> Result<(), sqlx::Error> {
        Self::ensure_merchant_accounts(pool, merchant_id).await?;
        let cash = Self::get_account(pool, "Cash", merchant_id).await?;
        let settlement = Self::get_account(pool, "Settlement", merchant_id).await?;

        let mut tx = pool.begin().await?;
        let entry = Self::create_journal_entry(
            &mut tx,
            Some(session_id),
            &format!("Reversal for session {}", session_id),
        )
        .await?;
        Self::create_posting(&mut tx, entry.id, settlement.id, amount_cents, "debit").await?;
        Self::create_posting(&mut tx, entry.id, cash.id, amount_cents, "credit").await?;

        if !Self::verify_balance(&mut *tx, entry.id).await? {
            tx.rollback().await?;
            return Err(sqlx::Error::Protocol(
                "reversal entry does not balance; refusing to post".into(),
            ));
        }
        tx.commit().await?;
        Ok(())
    }

    /// A merchant's available balance = net credit on their liability
    /// (Settlement) account — what Noor owes them and can pay out.
    pub async fn merchant_available_cents(pool: &PgPool, merchant_id: Uuid) -> Result<i64, sqlx::Error> {
        let bal: i64 = sqlx::query_scalar(
            r#"SELECT COALESCE(SUM(
                   CASE WHEN lp.direction = 'credit' THEN lp.amount_cents ELSE -lp.amount_cents END
               ), 0)::int8
               FROM ledger_postings lp
               JOIN ledger_accounts la ON la.id = lp.account_id
               WHERE la.merchant_id = $1 AND la.type = 'liability'"#,
        )
        .bind(merchant_id)
        .fetch_one(pool)
        .await?;
        Ok(bal)
    }
}