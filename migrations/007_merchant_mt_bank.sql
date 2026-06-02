-- Phase 1: merchants onboard their own bank account via Modern Treasury (no Plaid).
-- We store the MT counterparty + external_account ids and the verification state
-- so the merchant dashboard can show status and so payouts/collections can target
-- the merchant's account. Columns are nullable/additive — safe to apply online.
ALTER TABLE merchants ADD COLUMN IF NOT EXISTS mt_counterparty_id      TEXT;
ALTER TABLE merchants ADD COLUMN IF NOT EXISTS mt_external_account_id  TEXT;
-- none | pending_verification | verified | denied  (mirrors MT verification_status)
ALTER TABLE merchants ADD COLUMN IF NOT EXISTS bank_account_status     TEXT NOT NULL DEFAULT 'none';
-- Masked last 4 of the account number for display (raw number lives only in MT).
ALTER TABLE merchants ADD COLUMN IF NOT EXISTS bank_account_last4      TEXT;
