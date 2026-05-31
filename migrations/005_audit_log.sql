-- 005: immutable audit log of money-moving and admin actions (who/what/when).
CREATE TABLE IF NOT EXISTS audit_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor       TEXT NOT NULL,           -- operator email, "customer", or "system"
    actor_role  TEXT,                    -- operator role at the time, if applicable
    action      TEXT NOT NULL,           -- e.g. operator.create, session.initiate
    target_type TEXT,                    -- e.g. session, operator, merchant, webhook
    target_id   TEXT,
    metadata    JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log (created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_log_action ON audit_log (action);
CREATE INDEX IF NOT EXISTS idx_audit_log_target ON audit_log (target_type, target_id);
