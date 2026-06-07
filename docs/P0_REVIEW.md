# Depost P0 Production-Readiness Review

**Date:** 2026-05-31
**Scope:** Adversarial verification of the production-hardening pass on the Depost bank-to-bank payments monolith (`/home/omar/noorpay`). Rust+axum API in `crates/`, React/TS admin in `admin/`.
**Method:** Per-claim refutation against source, plus a full build verification (cargo check, clippy, admin build).

## Headline

The hardening pass closed **3 of 7** reviewed P0 claims end-to-end. The build is green (cargo check clean, clippy no errors, admin builds). But four task-named items (tests, outbound webhook worker, plaintext PII, reversal ledger) plus settlement-at-submit-time, provider idempotency, unauthenticated session/transfer routes, and unrotated secrets remain open or partial. **Not production-ready.**

## Build verification

| Step | Result |
|---|---|
| `SQLX_OFFLINE=true cargo check --workspace` | PASS (1 dependency future-incompat note from sqlx-postgres 0.7.4; no project errors) |
| `cargo clippy --workspace` | PASS (0 errors; 15 warnings in paybank-api, 14 auto-fixable; notable `if_same_then_else` at sessions.rs:281) |
| admin `npm ci && npm run build` | PASS (tsc clean, vite emitted dist; only deprecation notices) |

## Claim verdicts

| # | Claim | Verdict | Key evidence | Why not higher |
|---|---|---|---|---|
| 1 | MT/Column webhook: constant-time HMAC-SHA256 over raw body; settlement unforgeable | **CLOSED** | webhooks.rs:89-110 raw `Bytes`, fail-closed secret read, `x-signature`/`column-signature` required, `verify_hmac` (Mac::verify_slice, constant-time via subtle 2.6.1) runs **before** `from_slice`; the only `update_status` calls are post-verification (webhooks.rs:138-139). Both handlers structurally identical. No body-consuming middleware on public routes. | Residual: no replay protection; hex-encoding assumption unverified vs MT's live format; shared-secret fallback; fail-closed-500 on missing secret. All availability/replay, not forgery. |
| 2 | Atomic CAS money transitions; concurrent initiate cannot double-transfer | **CLOSED** | session_repo.rs:82-106 `try_transition` = single `UPDATE ... WHERE status = ANY(...)` returning rows==1. Both money paths (initiate.rs:48-58, sessions.rs:197-202) gate the single `initiate_transfer` on `claimed`; loser gets InvalidStateTransition. confirm = pending->authorized (no money). | Could not construct a bypass. Webhook `update_status` is unconditional but fires no transfer. |
| 3 | Idempotency: lock committed before external work; replay verbatim; different-body 409; confirm cannot create two MT counterparties | **PARTIAL** | idempotency.rs:51-55 commits lock before Proceed; ON CONFLICT DO NOTHING single-winner; mismatch->409; completed->Replay. | **Header is optional** (confirm.rs `if let Some(key)`); keyless re-confirm of an Authorized session re-runs `create_counterparty` -> **second MT counterparty**. **Crash between side effect and `complete()` wedges the key forever**: `expires_at` written (NOW+24h) but never read; no reaper. |
| 4 | Real double-entry ledger + balances | **PARTIAL** | ledger_repo.rs:148-156 balanced debit/credit with rollback on imbalance; ensure_merchant_accounts idempotent + lazy; merchant_available_cents is real signed sum; ledger-failure-after-transfer logged, response still completed. | **No compensating journal on Returned/Reversed** — balance **overstated** after ACH return (most material). Merchant creation provisions accounts non-atomically. No UNIQUE(session_id) on journal_entries; no DB CHECK on direction/sign. |
| 5 | Every admin/merchant route authed; no data leak | **PARTIAL** | router.rs:103/124 route_layer admin_auth/merchant_auth; merchant_api uses AuthedMerchant id, ignores Path (IDOR closed); redact_merchant on all four admin merchant handlers (no password_hash/api_key). | **`/api/sessions`, `/api/sessions/:id`, `/api/transactions` unauthenticated** and accept attacker-supplied merchant_id/UUID -> leak customer bank account/routing numbers. **`/api/sessions/:id/initiate` moves real money with no caller auth.** No CorsLayer, no login rate limit, no body limit. |
| 6 | Secrets out of source; ops basics in | **PARTIAL** | Working-tree compose/.env.example clean; JWT_SECRET+ADMIN_PASSWORD fail-fast; SIGTERM graceful shutdown; DB-backed /health/ready; env-driven Plaid host. | **Real Column/MT/Plaid/WEBHOOK secrets still in git history at 796372... (commit 796372... -> 796372)... commit 796372** — see below; **not rotated** (highest severity here). PUBLIC_APP_URL bypassed in 2 of 3 link generators with divergent localhost fallbacks; ADMIN_* undocumented; JWT min length 16 not 32; Config.public_app_url WARN-only fallback. |
| 7 | The P0 set is NOT complete (tests, outbound worker, PII plaintext, reversal ledger, + more) | **OPEN** | tests/ empty, zero `#[test]`/`#[tokio::test]`/`#[sqlx::test]`; no INSERT INTO webhook_events anywhere (dead outbox); customer_account_number/routing_number plaintext, echoed by get/list_session; Returned/Reversed status-only, no reversal in ledger_repo; no provider Idempotency-Key sent; settlement posts at submit-time not webhook-time; no error classification; no reconciliation; no OFAC/AML/KYC gate (kyc_status hardcoded "verified" at merchant_api.rs:47); seed ships live api_key. | Confirmed open/partial across the board. |

> **Note on claim 6 evidence:** the compromised secrets are present in commit **796b...** (the initial commit `796b` referenced in the verdict as `796b...`): `git show <initial-commit>:docker-compose.yml` exposes COLUMN_API_KEY, MODERN_TREASURY_API_KEY, PLAID_SECRET, PLAID_CLIENT_ID, and WEBHOOK_SECRET. Removing them from the working tree does **not** invalidate them.

## Residual gaps

### Open / Partial P0s (must close before production)
- **Reversal ledger (claim 4 / 7):** Returned/Reversed flip session status only; the original Settlement-liability credit is never reversed, so `merchant_available_cents` overstates payable after any ACH return — the ledger is not a faithful record of funds owed.
- **Plaintext PII (claim 7):** `customer_account_number` / `customer_routing_number` are bare TEXT columns, written raw and SELECTed back in cleartext by get/list_session. No tokenization, envelope encryption, KMS, or read-path redaction.
- **Outbound webhook worker (claim 7):** `webhook_events` / `webhook_delivery_logs` tables and retry indexes exist but are never written; no spawned tokio worker. Merchants receive no delivery callbacks. Pure dead scaffolding.
- **Zero tests (claim 7):** Workstream 9 produced nothing. None of the closed P0s (CAS, HMAC, ledger balance, idempotency) has a regression guard.
- **Unauthenticated session/transfer/transaction routes + data leak (claim 5):** IDOR enumeration of customer bank details and an unauthenticated money-firing endpoint.
- **Provider Idempotency-Key (claim 7):** `initiate_transfer`/`create_transfer` take no key; a retried create after an ambiguous timeout cannot be deduped by the provider.
- **Settlement at submit-time, not webhook-time (claim 7):** a provider that accepts then later returns/fails leaves a Completed+settled ledger with no reversal path.
- **Idempotency wedge + keyless double-counterparty (claim 3):** make the key mandatory on confirm (or add an Authorized-state CAS guard around counterparty creation); enforce `expires_at` in `begin()` or add a sweep.
- **Secret rotation (claim 6):** rotate all provider/webhook credentials at each provider regardless of test/sandbox status.
- **Compliance gate (claim 7):** no OFAC/AML screening, no KYC CHECK constraint, no fail-closed initiate gate; `kyc_status` hardcoded "verified".
- **Seed credentials (claim 7):** `002_seed_data.sql` inserts live-looking `pb_test_*` api_keys with status active / kyc verified, unconditionally.

### P1 backlog
- Webhook replay protection (timestamp/nonce); verify `session_repo::update_status` guards terminal states (currently appears unconditional — can move Completed back to Processing).
- Validate webhook signature encoding against MT's live format (hex vs base64 vs `t=,v1=` structured header); add an integration test with a real provider signature.
- Provider error classification: distinguish definitive reject from ambiguous timeout before transitioning Processing->Failed.
- Reconciliation / status-polling job for stuck Processing sessions.
- Robust webhook session-id derivation (do not parse `Session <uuid>` from free-text); persist active provider to disambiguate Column vs MT.
- Separate webhook secrets per provider (remove shared WEBHOOK_SECRET fallback).
- Decide behavior when webhook secret unset (currently fail-closed 500 silently drops settlement webhooks).

### P2 backlog
- Admin JWT in localStorage (XSS exfiltration) — consider httpOnly cookie.
- Single static bootstrap admin credential — add a per-operator user store.
- `paybank_core::Merchant` still derives Serialize with password_hash/api_key — latent re-leak; drop from struct or use a DTO.
- Config.public_app_url WARN-only localhost fallback — fail fast in prod.
- Add CorsLayer, login rate limiting (governor), explicit DefaultBodyLimit.

### P3 backlog
- `clippy::if_same_then_else` dead branch at sessions.rs:281; hardcoded `https://pay.noor.com/p/` prefix unrelated to PUBLIC_APP_URL.
- 14 auto-fixable clippy warnings; sqlx-postgres 0.7.4 future-incompat note.
- `complete_key` not constrained to `state='in_progress'`; hardcodes HTTP 200.
- ADMIN_EMAIL/ADMIN_PASSWORD undocumented in `.env.example`; JWT_SECRET min length 16 vs claimed 32.

## Recommended next waves

**Wave A — Money correctness (blocks launch).** Reversal/compensating ledger on Returned/Reversed; move settlement posting to webhook-time; provider Idempotency-Key on create_transfer; provider error classification (ambiguous-timeout != Failed); UNIQUE(session_id) on journal_entries + posting CHECK constraints.

**Wave B — Auth & data leak (blocks launch).** Authenticate `/api/sessions`, `/api/sessions/:id`, `/api/transactions`, and `/api/sessions/:id/initiate`; scope every list to the authenticated principal; add CorsLayer, login rate limit, body limit. Redact bank account/routing numbers on read paths.

**Wave C — PII & secrets (blocks launch).** Envelope-encrypt/tokenize account/routing numbers (KMS); migrate/drop plaintext columns; rotate all leaked provider/webhook credentials; remove seed credentials or gate them behind a dev-only flag.

**Wave D — Idempotency & reliability.** Make confirm idempotency key mandatory (or Authorized-state CAS guard); enforce `expires_at` / add reaper; build the outbound webhook delivery worker against the existing `webhook_events`/`webhook_delivery_logs` tables; add webhook replay protection and reconciliation/polling.

**Wave E — Compliance.** OFAC/AML screening with fail-closed initiate gate; KYC CHECK constraint; stop hardcoding kyc_status "verified".

**Wave F — Tests (cross-cutting, start now).** Regression tests for CAS double-fire, HMAC verify (valid/invalid/missing/malformed/replay), ledger balance + reversal, idempotency replay/mismatch/crash-wedge, and auth on every route. No P0 should be considered closed without one.
