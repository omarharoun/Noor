pub mod admin_repo;
pub mod attempt_repo;
pub mod bank_repo;
pub mod idempotency_repo;
pub mod ledger_repo;
pub mod session_repo;
pub mod transaction_repo;

use sqlx::PgPool;

#[derive(Clone)]
pub struct Db {
    pub pool: PgPool,
}

impl Db {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}