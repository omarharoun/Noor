-- 006: production hardening.
--
-- (a) VALIDATE the money-safety CHECKs that 003 added NOT VALID. By now all rows
--     satisfy them, so we promote the constraints to fully enforced (they begin
--     rejecting bad rows on read-modify paths too, not just new writes).
--
-- (b) Exactly-once ledger postings. The application already guards every
--     settlement/reversal behind a compare-and-swap status transition, so a
--     replayed webhook never re-posts. These indexes are defense-in-depth at the
--     DB layer: a session may have AT MOST ONE settlement journal entry and AT
--     MOST ONE reversal — but legitimately BOTH (settle, then a later return),
--     so the uniqueness is per (session_id, entry_type), not per session_id.
--
-- (c) Exactly-once transaction row per session (the synchronous settle path
--     writes one row keyed by a deterministic reference; this closes the gap if
--     a future caller picked a different reference for the same session).
--
-- (d) Rotate the published demo/seed API keys. The pb_test_* keys are committed
--     to git in migrations/002 — well-known and unusable in production. Replace
--     them with fresh random keys so the published values can never authenticate.

-- (a) -----------------------------------------------------------------------
ALTER TABLE ledger_postings  VALIDATE CONSTRAINT ledger_postings_direction_chk;
ALTER TABLE ledger_postings  VALIDATE CONSTRAINT ledger_postings_amount_pos_chk;
ALTER TABLE payment_sessions VALIDATE CONSTRAINT payment_sessions_amount_pos_chk;

-- (b) -----------------------------------------------------------------------
ALTER TABLE journal_entries ADD COLUMN IF NOT EXISTS entry_type TEXT;

-- Backfill existing rows so the unique index is consistent with history.
UPDATE journal_entries SET entry_type = 'settlement'
    WHERE entry_type IS NULL AND description LIKE 'Settlement%';
UPDATE journal_entries SET entry_type = 'reversal'
    WHERE entry_type IS NULL AND description LIKE 'Reversal%';

CREATE UNIQUE INDEX IF NOT EXISTS uq_journal_session_entry_type
    ON journal_entries (session_id, entry_type)
    WHERE session_id IS NOT NULL AND entry_type IS NOT NULL;

-- (c) -----------------------------------------------------------------------
CREATE UNIQUE INDEX IF NOT EXISTS uq_transactions_session
    ON transactions (session_id);

-- (d) -----------------------------------------------------------------------
UPDATE merchants
   SET api_key = 'pb_live_' || md5(random()::text || clock_timestamp()::text || id::text)
 WHERE api_key IN ('pb_test_1234567890abcdef', 'pb_test_abcdef1234567890');
