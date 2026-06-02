-- Phase 5: recurring invoices. A template that the recurring worker turns into
-- a real (MT) invoice every `interval_days`, advancing next_run each time.
CREATE TABLE IF NOT EXISTS recurring_invoices (
    id             UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id    UUID NOT NULL REFERENCES merchants(id),
    customer_name  TEXT NOT NULL,
    customer_email TEXT NOT NULL,
    amount_cents   BIGINT NOT NULL,
    description    TEXT,
    interval_days  INT NOT NULL DEFAULT 30,
    next_run       DATE NOT NULL DEFAULT CURRENT_DATE,
    active         BOOLEAN NOT NULL DEFAULT TRUE,
    last_invoice_id UUID,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_recurring_due ON recurring_invoices (next_run) WHERE active;
