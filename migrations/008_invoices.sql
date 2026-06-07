-- Phase 2: invoicing. Depost stores invoice metadata; Modern Treasury issues the
-- invoice + hosts the pay page and moves the money. We keep our own row so the
-- merchant dashboard can list/filter without calling the provider each time.
CREATE TABLE IF NOT EXISTS invoices (
    id                 UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id        UUID NOT NULL REFERENCES merchants(id),
    mt_invoice_id      TEXT,
    mt_counterparty_id TEXT,
    number             TEXT,
    customer_name      TEXT NOT NULL,
    customer_email     TEXT NOT NULL,
    description        TEXT,
    amount_cents       BIGINT NOT NULL,
    currency           TEXT NOT NULL DEFAULT 'USD',
    status             TEXT NOT NULL DEFAULT 'draft',
    hosted_url         TEXT,
    due_date           DATE,
    created_at         TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at         TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_invoices_merchant ON invoices (merchant_id, created_at DESC);
