-- Phase 6 (wallet): top-ups. A deposit pulls funds from the merchant's own
-- verified bank (ACH debit) into the platform account; the merchant's balance
-- is credited when the debit settles (status completed). Withdrawals reuse the
-- payouts table (a payout to the merchant's own account).
CREATE TABLE IF NOT EXISTS deposits (
    id                  UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id         UUID NOT NULL REFERENCES merchants(id),
    amount_cents        BIGINT NOT NULL,
    currency            TEXT NOT NULL DEFAULT 'USD',
    -- pending | completed | failed
    status              TEXT NOT NULL DEFAULT 'pending',
    mt_payment_order_id TEXT,
    credited            BOOLEAN NOT NULL DEFAULT FALSE,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_deposits_merchant ON deposits (merchant_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_deposits_pending ON deposits (status) WHERE status = 'pending';
