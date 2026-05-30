CREATE TABLE IF NOT EXISTS merchants (
    id                        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name                      TEXT NOT NULL,
    email                     TEXT NOT NULL UNIQUE,
    password_hash             TEXT,
    api_key                   TEXT NOT NULL UNIQUE,
    webhook_url               TEXT,
    webhook_secret            TEXT,
    business_name             TEXT,
    business_type             TEXT,
    registration_number       TEXT,
    tax_id                    TEXT,
    industry_category         TEXT,
    website_url               TEXT,
    status                    TEXT NOT NULL DEFAULT 'pending',
    kyc_status                TEXT NOT NULL DEFAULT 'not_started',
    risk_level                TEXT DEFAULT 'medium',
    column_account_id         TEXT,
    hyperswitch_merchant_id   TEXT,
    onboarding_completed_at   TIMESTAMPTZ,
    created_at                TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_merchants_email ON merchants(email);
CREATE INDEX IF NOT EXISTS idx_merchants_api_key ON merchants(api_key);
CREATE INDEX IF NOT EXISTS idx_merchants_status ON merchants(status);
CREATE INDEX IF NOT EXISTS idx_merchants_kyc_status ON merchants(kyc_status);

CREATE TABLE IF NOT EXISTS banks (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    routing_number  TEXT,
    supports_fednow BOOLEAN NOT NULL DEFAULT FALSE,
    supports_rtp    BOOLEAN NOT NULL DEFAULT FALSE,
    supports_wire   BOOLEAN NOT NULL DEFAULT FALSE,
    logo_url        TEXT,
    display_order   INT NOT NULL DEFAULT 999
);

CREATE TABLE IF NOT EXISTS payment_sessions (
    id                          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id                 UUID NOT NULL REFERENCES merchants(id),
    bank_id                     TEXT NOT NULL REFERENCES banks(id),
    amount_cents                BIGINT NOT NULL,
    currency                    TEXT NOT NULL DEFAULT 'USD',
    note                        TEXT,
    status                      TEXT NOT NULL DEFAULT 'pending',
    rail_used                   TEXT,
    column_ref                  TEXT,
    column_counterparty_id      TEXT,
    customer_name               TEXT,
    customer_email              TEXT,
    customer_phone              TEXT,
    customer_address_line_1     TEXT,
    customer_address_city       TEXT,
    customer_address_state      TEXT,
    customer_address_postal_code TEXT,
    customer_address_country_code TEXT,
    customer_account_number     TEXT,
    customer_routing_number     TEXT,
    customer_account_type       TEXT,
    expires_at                  TIMESTAMPTZ NOT NULL,
    provider                    VARCHAR(50),
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payment_sessions_merchant ON payment_sessions(merchant_id);
CREATE INDEX IF NOT EXISTS idx_payment_sessions_status ON payment_sessions(status);

CREATE TABLE IF NOT EXISTS transactions (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id   UUID NOT NULL REFERENCES payment_sessions(id),
    merchant_id  UUID NOT NULL REFERENCES merchants(id),
    amount_cents BIGINT NOT NULL,
    rail_used    TEXT NOT NULL,
    column_ref   TEXT,
    reference    TEXT UNIQUE NOT NULL,
    settled_at   TIMESTAMPTZ NOT NULL,
    provider     VARCHAR(50),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_transactions_merchant ON transactions(merchant_id);
CREATE INDEX IF NOT EXISTS idx_transactions_session ON transactions(session_id);
CREATE INDEX IF NOT EXISTS idx_transactions_reference ON transactions(reference);

CREATE TABLE IF NOT EXISTS payment_attempts (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id      UUID NOT NULL REFERENCES payment_sessions(id) ON DELETE CASCADE,
    rail            TEXT NOT NULL,
    provider_ref    TEXT,
    request_payload JSONB,
    response_status INT,
    response_body   JSONB,
    error_message   TEXT,
    status          TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_payment_attempts_session ON payment_attempts(session_id);
CREATE INDEX IF NOT EXISTS idx_payment_attempts_status ON payment_attempts(status);

CREATE TABLE IF NOT EXISTS ledger_accounts (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name        TEXT NOT NULL,
    type        TEXT NOT NULL,
    merchant_id UUID REFERENCES merchants(id),
    currency    TEXT NOT NULL DEFAULT 'USD',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ledger_accounts_merchant ON ledger_accounts(merchant_id);
CREATE INDEX IF NOT EXISTS idx_ledger_accounts_type ON ledger_accounts(type);

CREATE TABLE IF NOT EXISTS journal_entries (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id  UUID REFERENCES payment_sessions(id) ON DELETE SET NULL,
    description TEXT NOT NULL,
    status      TEXT NOT NULL DEFAULT 'posted',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_journal_entries_session ON journal_entries(session_id);
CREATE INDEX IF NOT EXISTS idx_journal_entries_status ON journal_entries(status);

CREATE TABLE IF NOT EXISTS ledger_postings (
    id               UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    journal_entry_id UUID NOT NULL REFERENCES journal_entries(id) ON DELETE CASCADE,
    account_id       UUID NOT NULL REFERENCES ledger_accounts(id),
    amount_cents     BIGINT NOT NULL,
    direction        TEXT NOT NULL,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_ledger_postings_account ON ledger_postings(account_id);
CREATE INDEX IF NOT EXISTS idx_ledger_postings_journal ON ledger_postings(journal_entry_id);

CREATE TABLE IF NOT EXISTS idempotency_keys (
    idempotency_key TEXT NOT NULL,
    merchant_id     UUID NOT NULL REFERENCES merchants(id),
    request_hash    TEXT NOT NULL,
    response_status INT,
    response_body   JSONB,
    state           TEXT NOT NULL DEFAULT 'in_progress',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at      TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (merchant_id, idempotency_key)
);

CREATE INDEX IF NOT EXISTS idx_idempotency_keys_expires ON idempotency_keys(expires_at);
CREATE INDEX IF NOT EXISTS idx_idempotency_keys_state ON idempotency_keys(state);

CREATE TABLE IF NOT EXISTS webhook_events (
    id            UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    merchant_id   UUID NOT NULL REFERENCES merchants(id) ON DELETE CASCADE,
    session_id    UUID REFERENCES payment_sessions(id) ON DELETE SET NULL,
    event_type    TEXT NOT NULL,
    payload       JSONB NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempts      INT NOT NULL DEFAULT 0,
    next_retry_at TIMESTAMPTZ,
    sent_at       TIMESTAMPTZ,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_events_merchant ON webhook_events(merchant_id);
CREATE INDEX IF NOT EXISTS idx_webhook_events_session ON webhook_events(session_id);
CREATE INDEX IF NOT EXISTS idx_webhook_events_status ON webhook_events(status);
CREATE INDEX IF NOT EXISTS idx_webhook_events_queue ON webhook_events(status, next_retry_at) WHERE status IN ('pending', 'failed');
CREATE INDEX IF NOT EXISTS idx_webhook_events_pending ON webhook_events(next_retry_at, created_at) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_webhook_events_type_status ON webhook_events(event_type, status);

CREATE TABLE IF NOT EXISTS webhook_delivery_logs (
    id              UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    webhook_event_id UUID NOT NULL REFERENCES webhook_events(id) ON DELETE CASCADE,
    attempt_number   INT NOT NULL DEFAULT 0,
    request_headers  JSONB,
    request_body     JSONB,
    response_status  INT,
    response_body    TEXT,
    error_message    TEXT,
    delivered_at     TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_webhook_delivery_logs_event ON webhook_delivery_logs(webhook_event_id);
