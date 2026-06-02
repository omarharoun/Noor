-- Phase 3b: outbound payouts with dual-control approval.
-- A payout sends money from the platform's internal account to a payee's
-- external account (an MT payment_order, direction=credit) and debits the
-- initiating merchant's Settlement balance. Every payout needs approval; an
-- owner-initiated one is auto-approved (see the handler).
CREATE TABLE IF NOT EXISTS payouts (
    id                       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id              UUID NOT NULL REFERENCES merchants(id),
    payee_name               TEXT NOT NULL,
    payee_counterparty_id    TEXT,
    payee_external_account_id TEXT,
    payee_last4              TEXT,
    amount_cents             BIGINT NOT NULL,
    currency                 TEXT NOT NULL DEFAULT 'USD',
    rail                     TEXT NOT NULL DEFAULT 'ach',
    description              TEXT,
    -- pending_approval | approved | processing | completed | returned | failed | rejected
    status                   TEXT NOT NULL DEFAULT 'pending_approval',
    initiated_by             UUID NOT NULL REFERENCES merchant_users(id),
    approved_by              UUID REFERENCES merchant_users(id),
    mt_payment_order_id      TEXT,
    created_at               TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at               TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_payouts_merchant ON payouts (merchant_id, created_at DESC);
