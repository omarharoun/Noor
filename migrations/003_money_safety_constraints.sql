-- 003: money-safety constraints on the ledger and sessions.
-- NOT VALID so the migration never fails on pre-existing rows, while still
-- enforcing the invariant for all new/updated rows.

ALTER TABLE ledger_postings
    ADD CONSTRAINT ledger_postings_direction_chk
    CHECK (direction IN ('debit', 'credit')) NOT VALID;

ALTER TABLE ledger_postings
    ADD CONSTRAINT ledger_postings_amount_pos_chk
    CHECK (amount_cents > 0) NOT VALID;

ALTER TABLE payment_sessions
    ADD CONSTRAINT payment_sessions_amount_pos_chk
    CHECK (amount_cents > 0) NOT VALID;

-- Hot path: admin/merchant session listing filters by status and orders by created_at.
CREATE INDEX IF NOT EXISTS idx_payment_sessions_status_created
    ON payment_sessions (status, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_payment_sessions_merchant_created
    ON payment_sessions (merchant_id, created_at DESC);
