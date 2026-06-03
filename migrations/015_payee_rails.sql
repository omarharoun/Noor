-- Cache the rails Modern Treasury says a saved payee's routing number supports
-- (comma-separated MT payment types, e.g. "ach,rtp,wire"), looked up at onboard
-- time. Lets payouts auto-route to the fastest supported rail without a live
-- lookup on every send. NULL/empty => fall back to ACH.
ALTER TABLE payees ADD COLUMN IF NOT EXISTS supported_rails TEXT;
