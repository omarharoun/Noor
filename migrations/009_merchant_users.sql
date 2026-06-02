-- Phase 3a: multi-user merchant accounts with roles, for dual-control payouts.
-- A merchant (the business/account) now has named users who log in with
-- email + password. Roles form a hierarchy: owner > admin > member.
-- Any active user can initiate a payout; only admin/owner can approve, and an
-- owner may self-approve (see the payout flow in Phase 3b).
CREATE TABLE IF NOT EXISTS merchant_users (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id   UUID NOT NULL REFERENCES merchants(id),
    email         TEXT NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    name          TEXT NOT NULL,
    -- owner | admin | member
    role          TEXT NOT NULL DEFAULT 'member',
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_merchant_users_merchant ON merchant_users (merchant_id);
