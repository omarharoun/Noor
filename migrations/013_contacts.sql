-- Phase 7: contacts. Saved payees (vendors) reuse their MT counterparty +
-- external account so payouts don't re-onboard a bank every time; saved
-- customers are quick-pick for invoices.
CREATE TABLE IF NOT EXISTS payees (
    id                        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id               UUID NOT NULL REFERENCES merchants(id),
    name                      TEXT NOT NULL,
    email                     TEXT,
    mt_counterparty_id        TEXT,
    mt_external_account_id    TEXT,
    account_last4             TEXT,
    account_type              TEXT,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_payees_merchant ON payees (merchant_id, created_at DESC);

CREATE TABLE IF NOT EXISTS customers (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id  UUID NOT NULL REFERENCES merchants(id),
    name         TEXT NOT NULL,
    email        TEXT NOT NULL,
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (merchant_id, email)
);
CREATE INDEX IF NOT EXISTS idx_customers_merchant ON customers (merchant_id, created_at DESC);
