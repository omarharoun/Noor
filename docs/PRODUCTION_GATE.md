# Noor — Production Readiness Gate

Date: 2026-05-31
Scope: Bank-to-bank payments monolith (Rust/axum in `crates/`, React admin in `admin/`).
Method: Adversarial source review (read-only, traced end-to-end with file:line evidence) + automated build/test gate.

## Headline

CODE GATE GREEN (31 tests pass, clippy clean, admin build passes). The payment-core architecture is sound and most hardening items are code-complete, but real-money production is gated by: a stubbed sanctions/KYC screen, two authenticated admin endpoints that leak bank PII, a narrow double-settlement window, and a body of human-owned residue (licensing, leaked-key rotation, real KYC vendor, secrets management, real operator user store) that code cannot close.

Verdict: **GO for sandbox pilot. NO-GO for real-money production.**

## Build / Test Gate

| Check | Result |
|---|---|
| cargo test | PASS — 31/31 |
| clippy | clean |
| admin build (React) | PASS |
| errors | none |

Note: the gate ran in the working tree. The `.sqlx` offline cache is untracked in git and absent from `.gitignore`; a clean CI/Docker build (`SQLX_OFFLINE=true cargo build`) from the committed tree would fail until the cache is committed. The green gate above reflects local state, not a clean checkout.

## Capability Status

| Capability | Status | Evidence |
|---|---|---|
| Idempotent retry / no double-pay | CLOSED | Deterministic key `noor-transfer-{session_id}` (paybank-payments/lib.rs:74) sent to MT (moderntreasury.rs:182) + Column (column.rs:168); timeout/5xx/429 → ProviderAmbiguous (504), session stays Processing; only definitive errors → Failed (initiate.rs:127-145). |
| JWT_SECRET >= 32 fail-fast | CLOSED | state.rs:21-25, invoked before bind (main.rs:22). |
| Graceful shutdown (SIGTERM) | CLOSED | main.rs:51-81. |
| DB readiness probe | CLOSED | SELECT 1 on pool, GET /health/ready (router.rs:19-27,149). |
| Request body limit | CLOSED | 256 KiB once (router.rs:187). |
| JWT expiry + constant-time creds | CLOSED | Validation::default validates exp (auth.rs:54-62); ct_eq compare (auth.rs:72-82). |
| Money-safety DB constraints (mig 003) | PARTIAL | CHECKs present but all NOT VALID with no VALIDATE — new rows only (003_money_safety_constraints.sql). |
| PII encryption at rest (AES-256-GCM) | PARTIAL | Encrypt-on-write/decrypt-on-read correct (session_repo.rs:222-223,54-62); but fail-open to plaintext if key unset/errors (crypto.rs:53-55,65); single static env key, no KMS/rotation. |
| Bank-number exposure controls | OPEN | Admin detail returns full plaintext acct+routing (admin.rs:96-104 + models.rs serialize); admin/operator lists leak ciphertext (admin.rs:76-94, session_repo:177-204). |
| Session status state machine race-safety | CLOSED | All writes via atomic CAS try_transition (session_repo.rs:92-116); confirm single-winner (confirm.rs:61-114); no terminal-backward (webhooks.rs:65-82). |
| Money transitions coupled to CAS | PARTIAL | webhook/reconcile gate ledger on CAS bool; initiate/sessions DISCARD it and post unconditionally (initiate.rs:71-97, sessions.rs:256-282) — double-settlement window; no DB uniqueness on journal_entries/transactions.session_id. |
| Compliance gate (sequencing/fail-closed) | PARTIAL | Architecturally correct and fail-closed (compliance.rs:105-131; confirm.rs:39-45); confirm is sole Authorized producer. But the screen itself is a STUB (compliance.rs:21-32) — no real sanctions coverage. |
| Outbound merchant webhooks | PARTIAL | Worker, atomic claim, backoff, signing, retry all real; but stuck-'sending' has no reaper (at-least-once dishonest), enqueue is fire-and-forget, admin retry doesn't reset attempts. |
| Stuck-session reconciliation | PARTIAL | Poller resolves ref-bearing MT sessions correctly; but ambiguous-timeout sessions never get a ref (never auto-resolve), PROVIDER hardcoded MT (reconcile.rs:14) so Column refs fail, ledger errors swallowed. |
| Login throttle | PARTIAL | Real but in-process, per-email (attacker-controlled), not IP/distributed (auth.rs:105-126). |
| CORS | PARTIAL | allow_origin(Any) wildcard; no credentials so not a hole, but not hardened (router.rs:178-181). |
| Customer pay flow (no regression) | CLOSED | pay.html/invoice.html hit only public router, redacted GET intact (sessions.rs:102-148). |
| Operator dashboard (web/dashboard.html) | OPEN (regression) | Sends no auth header but calls now-gated endpoints → 401 (router.rs:173 + handlers). |
| Provider webhooks vs compliance gate | NOTE | HMAC webhooks can move Pending→Processing/Completed un-gated (webhooks.rs:67-79); no outbound transfer fires, so not a money bypass. |
| Operator identity / auth model | OPEN | Single bootstrap ADMIN_EMAIL/PASSWORD from env, plaintext ct_eq (auth.rs:129-130); no user store. |

## Residual Backlog (code-addressable, before real money)

P0 (blocks real-money production):
1. Redact bank account/routing in admin `get_session_detail` and both list endpoints (admin.rs:96-104, 76-94; session_repo:177-204) — add serde skip / DTO, decrypt only behind explicit privileged reveal with audit.
2. Replace the sanctions/KYC stub (compliance.rs:21-32) with a real vendor integration; add account/beneficiary screening; add tests for the fail-closed provider-down branch.
3. Gate ledger settlement on the CAS result in initiate.rs:71-97 and sessions.rs:256-282 (mirror webhook/reconcile); add a UNIQUE constraint on journal_entries.session_id (or settlement key) for DB-level idempotency.
4. Make crypto fail-CLOSED: abort startup if PII_ENCRYPTION_KEY is unset/invalid; never silently store plaintext (crypto.rs:53-55,65,34-42).

P1:
5. Add a reaper for stale webhook 'sending' rows and a DB CHECK on webhook_events.status; make enqueue failures observable (not fire-and-forget); reset attempts on admin retry.
6. Reconcile: derive PROVIDER per session instead of hardcoded MT (reconcile.rs:14); implement idempotency-key-based provider lookup for ref-less ambiguous-timeout sessions; add alerting/dead-letter/aging cap; stop swallowing ledger errors (reconcile.rs:66,73).
7. Commit `.sqlx` cache (or add to build) so clean CI/Docker builds succeed.
8. Fix or retire web/dashboard.html (router.rs:173).

P2:
9. Move login throttle to a shared/distributed IP-aware store.
10. Tighten CORS to an operator-origin allowlist.
11. Add VALIDATE CONSTRAINT for migration 003 after backfill, or document the residual-bad-row risk.

## Human-Owned Residue (code cannot close)

- Rotate leaked provider keys (Modern Treasury / Column / Plaid) in git history — revoke and reissue; a code fix cannot un-leak a committed secret.
- Sponsor-bank relationship and money-transmitter / MSB licensing — regulatory prerequisite to real money movement.
- Procurement + contract for a real KYC + OFAC/sanctions vendor behind the stub.
- SOC 2 Type II audit and independent penetration test.
- Production secret management (KMS/secrets manager, envelope encryption, key rotation plan) for PII_ENCRYPTION_KEY / JWT_SECRET / provider keys / WEBHOOK_SECRET.
- Real operator user store: hashed credentials, per-operator identity, least-privilege roles, audit logging — replace single bootstrap env credential. Decide whether operators may ever see raw bank numbers.
- Operational runbook + on-call/alerting for the manual-reconciliation queue and ledger-posting-failed errors.
- Business decision on the CORS operator-origin allowlist.

## Go / No-Go

(a) Sandbox pilot (provider sandboxes, no real funds, trusted internal operators): **GO.** Core flows work, state machine is race-safe, idempotency holds, build/test gate is green. Acceptable to pilot provided operators are trusted (mitigates the PII leak) and the stub screen is understood as non-functional. Fix the `.sqlx` deploy issue and the dashboard.html regression for a clean environment.

(b) Real-money production: **NO-GO.** Blocked by P0 code items (PII leak, sanctions stub, double-settlement window, crypto fail-open) AND by the full human-owned residue (licensing, leaked-key rotation, real KYC vendor, SOC2/pen-test, secrets management, real operator auth). None of these may be waived for handling customer funds.
---

## Post-review remediation (applied after this report)

The following findings from the review above were FIXED immediately after it was generated (build green, 31 tests pass, clippy clean, admin builds, runtime-verified):

- **LEAK #1/#2 (PII over admin endpoints) — FIXED.** `customer_account_number` / `customer_routing_number` are now `#[serde(skip_serializing)]` on `PaymentSession`, so the admin detail, admin list, and operator list endpoints no longer emit them (plaintext or ciphertext). Internal reads (confirm/initiate) use sqlx `FromRow`, not serde, so they still work; the public pay endpoint still exposes only a masked last-4. Runtime-verified: admin detail/list responses omit both fields, retain `customer_account_type`.
- **Double-settlement TOCTOU — FIXED.** Both initiate paths now gate `create_transaction` + `record_settlement` + webhook emit on WINNING the `Processing -> Completed` CAS (`if won { ... }`), so a concurrent webhook/reconcile cannot double-post.
- **Webhook stuck-`sending` reaper — FIXED.** `claim_due` now also reclaims `sending` rows whose 60s lease expired (it bumps `next_retry_at` on claim), making at-least-once honest after a mid-delivery crash.
- **Reconcile hardcoded provider — FIXED.** Now derives the provider string from `PaymentProvider::primary()` instead of always `modern_treasury`.
- **`.sqlx` deploy break — FIXED.** The offline query cache is now committed to git so clean-checkout / Docker / CI `SQLX_OFFLINE` builds work.

Still open / human-owned (unchanged from above): the sanctions/KYC **screen is a stub** (no real vendor), `crypto` fail-open if `PII_ENCRYPTION_KEY` unset (dev convenience), login throttle is per-process/per-email (not IP/distributed), CORS `allow_origin(Any)`, migration 003 CHECKs are `NOT VALID` (new rows only), `web/dashboard.html` demo calls now-gated endpoints (superseded by the `/admin` console), fire-and-forget webhook enqueue, and all the regulatory/secret-rotation/vendor items in the residue list.
