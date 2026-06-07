# Depost Production-Readiness Program

_Status: consolidated audit + remediation roadmap. Owner: Lead Engineering. Date: 2026-05-31._

> Scope reminder: Depost is a regulated instant bank-to-bank (ACH/RTP/FedNow/Wire) money-movement platform. Correctness, security, and money-safety are paramount. This document consolidates the subsystem audits and the per-workstream implementation specs into a single prioritized, dependency-ordered program.

---

## 1. Executive Summary

Depost has strong database and code **scaffolding** but almost no enforced money-safety, security, or compliance **functionality**. Static reading of source confirms the most dangerous defects directly:

- **The entire API is unauthenticated.** `crates/paybank-api/src/router.rs` (112 lines) mounts ~30 routes with zero middleware — no auth, CORS, rate limiting, body limits — and `src/main.rs` serves it on `0.0.0.0`. Every admin, merchant, and customer surface is reachable by any caller.
- **Money can be moved by anyone, twice.** Both `/api/sessions/initiate` and `/api/sessions/:id/initiate` call `initiate_transfer` for any session UUID with no caller auth. `session_repo::update_status` (session_repo.rs:56-75) is an unconditional `UPDATE ... WHERE id=$1` — **not** a compare-and-swap — so concurrent/duplicate POSTs both fire a transfer. No provider `Idempotency-Key` is sent, so the provider cannot de-dupe either.
- **Settlement is forgeable.** `moderntreasury_webhook` (webhooks.rs:89-137), for the **primary** provider, performs **no signature verification** and drives sessions to Completed/Expired purely on attacker-controlled JSON.
- **Secrets leak.** Unauthenticated admin merchant endpoints serialize the full `Merchant` struct including `password_hash` and `api_key`.
- **No ledger, fake balances.** `LedgerRepo` and `IdempotencyRepo` have **zero call sites** (confirmed by grep); `get_balance` returns hardcoded `0/0`. There is no double-entry record of any money movement.
- **PII in plaintext.** Customer bank account + routing numbers are stored as bare `TEXT` and echoed to the browser.
- **Failure state is lossy.** `SessionStatus` has no Failed/Returned/Reversed; every failure (including network-timeout-ambiguous and real clawbacks) collapses to `Expired`, destroying reconciliation.
- **Operational gaps.** No CI, no graceful shutdown, static `/health`, no observability middleware, dead outbound-webhook subsystem, live-format secrets committed in `docker-compose.yml`, and **zero tests** in the workspace.
- **No compliance perimeter.** No KYC/KYB gate, no OFAC/AML screening, no sponsor-bank/licensing model. The platform **cannot lawfully move money in production** until that business gate is satisfied.

**18 P0 blockers** stand between the current state and a safe production posture. The remediation is well-specified across **13 workstreams**. The dominant planning constraint: the five coupled **payment-core** workstreams (auth, idempotency, ledger, failure/returns, webhook-outbox) all converge on the same hot files and must be **sequenced or carefully integrated**, while the **independent** streams (config/ops, secrets, CI-Docker, observability, tests, admin auth-gate frontend, compliance docs) can run **concurrently** from day one.

---

## 2. P0 Blocker Count

**18 P0 blockers**, grouped by theme:

**Authentication / authorization (5)**
1. Entire `/api/admin/*` surface unauthenticated.
2. Admin merchant endpoints leak `password_hash` + `api_key`.
3. Merchant `api_key` never validated; merchant-api hardcodes `DEMO_MERCHANT_ID`.
4. Unauthenticated money movement: both initiate endpoints.
5. Admin SPA has no login/auth gate (frontend half of #1).

**Webhook authenticity (1)**
6. Modern Treasury webhook has no signature verification (settlement forgery on the primary rail).

**Money-movement correctness (4)**
7. No idempotency / double-submit guard on initiate (unconditional `update_status`, no CAS).
8. No `Idempotency-Key` sent to the provider on payment-order creation.
9. Transfer/processing failure maps to `Expired`, losing money-state (no Failed/Returned/Reversed).
10. `idempotency_keys` table exists but is never used (no duplicate-payment protection).

**Ledger / balances (2)**
11. Double-entry postings never written in any payment flow (`LedgerRepo` has zero callers).
12. Balance endpoint returns hardcoded `0/0`.

**Data protection (2)**
13. Raw bank account + routing numbers stored in plaintext (also echoed to browser).
14. Plaid endpoints have no auth/authz (IDOR overwrite of any session's bank details).

**Provider resilience / reconciliation (2)**
15. "Fallback" provider is not a fallback — no failover on MT failure (latent, env-toggle only).
16. Active provider string not persisted — breaks Column status/webhook reconciliation.

**Ops / compliance (2)**
17. Live-format provider secrets committed to git in `docker-compose.yml`.
18. No compliance perimeter (KYC/KYB gate, OFAC/AML screening, sponsor-bank/licensing) — unlicensed money transmission risk.

> The TEST SUITE and Admin auth-gate workstreams are tagged P0 in their specs; their P0 status is **derivative** of the blockers above (they validate/enforce them) and are counted within #5 and as program-critical enablers rather than as separate money-state defects.

---

## 3. Cross-Cutting Hazard: the Coupled Payment-Core Cluster

Five workstreams repeatedly edit the **same load-bearing files**. Running them blindly in parallel will produce merge conflicts and, worse, silently inconsistent money logic.

| Shared file | Auth | Idempotency | Lifecycle/Failure | Ledger | Webhook-outbox | Observability |
|---|---|---|---|---|---|---|
| `routes/sessions.rs` | ● (confirm_token) | ● (CAS, replay) | ● (states, CAS) | ● (settle) | ● (enqueue) | ● (audit/metrics) |
| `routes/initiate.rs` | | ● | ● | ● | ● | ● |
| `routes/confirm.rs` | ● | ● | | | | ● |
| `routes/webhooks.rs` | ● (routing) | | ● (mapping) | ● (settle/reverse) | ● (enqueue) | ● (audit) |
| `session_repo.rs::update_status` | | (executor) | ● (rewrite to CAS) | (executor) | | ● (audit hook) |
| `transaction_repo.rs::create_transaction` | | ● (executor) | | ● (executor) | | |
| `core/models.rs::SessionStatus` | | | ● (add variants) | | (consume) | |
| `router.rs` / `state.rs` | ● (nest, extend) | ● (route) | | | ● (route, http client) | ● (layers, handle) |
| `migrations/003*` | ● | | ● | ● | ● | ● |

**Integration rules adopted by this program:**
- **`session_repo::update_status` is rewritten once** by the Lifecycle/Failure stream into a transition-guarded CAS. Observability's audit hook and the outbox enqueue both attach to that single rewritten chokepoint — they do **not** independently rewrite it.
- **`transaction_repo::create_transaction` is migrated to a `Transaction` executor once** (Idempotency owns the refactor; Ledger and PII build on it).
- **`SessionStatus` variants are added once** (Lifecycle/Failure); all `match` sites (SSE break in sessions.rs:134, webhook mappers) are updated in the same change.
- **Migration `003` is a single coordinated migration file** (or a clearly numbered sequence 003/004/005) owned jointly; do not let each stream invent its own `003`.
- **Webhook HMAC verification body changes stay inside the handler** and are layered after the handler rewrite, not before.

---

## 4. Dependency-Ordered Waves

Each wave is a set of workstreams with **no shared-file and no logical-dependency conflicts** _within the wave_. Waves are gated on the prior wave's integration.

### Wave 0 — Foundations (fully parallel)
- **Secrets Management** — rotate the exposed MT/Column/Plaid/WEBHOOK_SECRET/DB creds **immediately**, typed `AppConfig` + `secrecy`, fail-fast at boot, env-driven base URLs, git-history scrub + force-push.
- **Config / Graceful-Shutdown / Health** — typed `Config` on `AppState`, env-driven `PUBLIC_APP_URL`/Plaid/MT URLs, canonical pay/QR link, graceful shutdown + SIGTERM, pool tuning (`acquire_timeout` etc.), DB-backed `/ready`.
- **Test Suite scaffolding** — the test-enabling base-URL refactor (env-overridable provider URLs), `.sqlx` offline cache, fixtures, and the `#[ignore]` known-gap tests that encode each P0 as a living red checklist.
- **CI/CD & Docker Hardening** — GitHub Actions (fmt/clippy/test/sqlx-prepare-check/admin-build/docker-build/security), non-root container + HEALTHCHECK + WORKDIR, decouple migrations from boot, remove `appsmith/`/`drop_all.py`/`run_migrations.py`.

_Rationale:_ infra/config enablers the rest of the program needs. The typed `Config`/extended `AppState` and the provider-fn signatures are a **shared contract** — agree the `AppState` struct shape and provider signatures at the start of this wave so downstream streams don't re-edit them.

### Wave 1 — Identity & Visibility
- **Authentication & Authorization** — nest router into admin/merchant/public sub-routers, `merchant_auth` (API-key) + `admin_auth` (JWT+roles) middleware, redacted `MerchantResponse`, `confirm_token` session capability, drop `DEMO_MERCHANT_ID`.
- **Observability & Audit Log** — JSON tracing + EnvFilter, request-id propagation + `TraceLayer` (matched-path, **never** raw URI/PII), Prometheus `/metrics`, Sentry with PII scrubbing, immutable append-only `audit_log` table.

_Rationale:_ Auth is the linchpin P0 (unblocks compliance, the admin frontend gate, ledger scoping, the webhook-retry endpoint, IDOR closure) but must follow the Config/AppState foundation. Observability's audit hook attaches to `update_status` — coordinate so its table/middleware land here and its `update_status` audit hook folds into the Wave-2 rewrite (no double-rewrite).

### Wave 2 — Money-State Machine & Provider Truth
- **Payment Session Lifecycle** — add `Failed/Returned/Reversed/Refunded` to `SessionStatus` + CHECK constraint, rewrite `update_status` into transition-guarded CAS, collapse the two divergent initiate endpoints, persist `Expired` + add sweeper.
- **Provider Hardening (MT + Column)** — thread `Idempotency-Key` into `initiate_transfer`/`create_transfer`/`create_counterparty`, error classification (definitive reject vs ambiguous timeout/5xx), HTTP-status checks before parse, FedNow/RTP rail correctness, persist the **active** provider string.

_Rationale:_ the core of the coupled cluster. The CAS + distinct states + collapsed endpoints are prerequisites for idempotency, failure-returns, and ledger. Provider hardening (separate crate) exposes the idempotency key and error discriminant those streams consume.

### Wave 3 — Idempotency & Data Protection
- **Idempotency Wiring** — `Idempotency-Key` header contract, per-endpoint request hash, two-transaction acquire+CAS pattern (don't hold a tx across the 15s provider call), deterministic `pay_{session_id}` reference, propagate provider key, migrate `session_repo`/`transaction_repo` to a generic `Transaction` executor.
- **PII Protection + Plaid Hardening** — tokenize via Plaid `processor_token` / MT `external_account_id`, envelope-encrypt the residual (AES-256-GCM + KMS-wrapped DEK, AAD-bound), drop plaintext columns after verified backfill, redact read paths, close the Plaid exchange IDOR via `confirm_token`, add `PLAID_ENV` base URL + status checks + propagate DB errors.

_Rationale:_ Idempotency depends on Wave-2 states (failed-replay ≠ Expired) and the provider key. PII depends on the Wave-0 KEK config and Wave-1 auth. Their shared surface is `session_repo.rs`/`confirm.rs` — Idempotency owns the executor refactor first; PII layers encrypted columns on top.

### Wave 4 — Ledger & Reversals
- **Ledger & Real Balances** — provision per-merchant chart of accounts at onboarding, write double-entry postings in the settlement path inside one `sqlx` transaction with `verify_balance` rollback, real `get_balance` aggregation (available vs pending by `journal_entries.status`), DB money-safety constraints (direction/amount/balance trigger).
- **Failure / Returns / Reversals ledger entries** — compensating reversal journals on Returned/Reversed/Refunded, idempotent on `transfer_id`.

_Rationale:_ Ledger depends on lifecycle states, the executor refactor, auth scoping, and the idempotent settlement tx. The reversal half mirrors the forward settle entry, so settle lands first within the wave; both finalize the `webhooks.rs` settlement branches together to avoid double-posting.

### Wave 5 — Webhook Authenticity & Delivery
- **MT Webhook Signature Verification** — `HeaderMap` + HMAC over raw body using the configured MT secret, constant-time compare; align Column to constant-time + its documented scheme.
- **Outbound Webhook Outbox + Delivery Worker** — enqueue on real transitions, `FOR UPDATE SKIP LOCKED` claim, HMAC-signed delivery with stable `X-Depost-Webhook-Id`, capped exponential backoff, admin retry endpoint + frontend wiring.

_Rationale:_ Both finalize `webhooks.rs`, so they land after the lifecycle/ledger rewrites of that file. The outbox depends on the CAS (enqueue only on `rowcount=1`), the distinct states, per-provider secrets, and admin auth (retry endpoint). **Sequencing caveat (P0 tension):** MT signature verification is a critical forgery hole that ideally lands earliest; because it edits the heavily-rewritten MT handler, the pragmatic choice is to add it as the **first** edit of this wave, or hoist it into Wave 1 as a standalone `HeaderMap`+verify guard and re-integrate after the handler rewrite. Either way it is the one P0 whose placement is sequencing-driven, not priority-driven.

### Wave 6 — Frontend Gate & Compliance (CODE)
- **Admin console auth gate (frontend)** — `AuthContext`, `Login` screen, bearer/`credentials` on `api.ts`, centralized 401→logout, real operator footer, drop `api_key` from the TS `Merchant` interface.
- **Compliance Roadmap (CODE items)** — `kyc_status` state machine + CHECK, KYB onboarding flow, OFAC screening wired into `confirm` + an initiate gate, AML monitoring worker, audit-log enforcement, retention/purge jobs, and the **fail-closed** compliance gate on money movement.

_Rationale:_ the frontend gate depends only on the Wave-1 backend admin-auth contract and provides no security on its own. Compliance CODE depends on auth (real merchant identity), the ledger (AML data source), PII encryption (GLBA/NACHA), and distinct failure states (SAR evidence) — necessarily last among code work. Both touch disjoint surfaces (`admin/src` vs new compliance crates/migrations + the existing confirm/initiate gate).

> **Parallel policy/business track (not a wave dependency):** the sponsor-bank/BaaS agreement, FinCEN MSB registration + state MTLs, named BSA/AML Officer, board-approved BSA/AML program, SOC 2 Type I→II, and privacy/retention documentation run **continuously from Wave 0**. They are the **true production go-live gate**: the fail-closed flag on `initiate` stays `false` until legal sign-off, regardless of code completion.

---

## 5. Per-Workstream Summary

| # | Workstream | Priority | Effort | Wave | Key dependencies | Primary shared-file risk |
|---|---|---|---|---|---|---|
| 1 | Authentication & Authorization | P0 | L | 1 | Config (JWT_SECRET), AppState shape | router.rs nesting, state.rs, sessions/confirm/merchant_api |
| 2 | Idempotency wiring | P0 | L | 3 | Lifecycle states+CAS, provider key, executor refactor | sessions/initiate/confirm, session_repo, transaction_repo |
| 3 | Ledger & Real Balances | P0 | L | 4 | Lifecycle, executor refactor, auth scoping | webhooks/initiate/sessions, ledger_repo, transaction_repo |
| 4 | PII Protection + Plaid hardening | P0 | L | 3 | Config (KEK), auth (confirm_token) | confirm.rs, session_repo, plaid.rs |
| 5 | Secrets Management | P0 | L | 0 | — | main.rs, payments crate signatures, compose |
| 6 | Outbound Webhook Outbox + Worker | P1 | L | 5 | Lifecycle CAS+states, admin auth, secrets | webhooks.rs, state.rs (http client), router |
| 7 | Failure / Returns / Reversals | P0 | L | 2→4 | DB constraints, ledger settle path, provider error classification | models.rs, session_repo, webhooks, payments |
| 8 | Config / Graceful-Shutdown / Health | P1 | M | 0 | — | main.rs, state.rs, sessions, plaid, Dockerfile |
| 9 | Test Suite | P0 | XL | 0 | base-URL refactor (own) | provider base-URL constants, Cargo manifests, .sqlx |
| 10 | Observability & Audit Log | P1 | L | 1 | auth (actor_id), lifecycle (update_status), config | main.rs, router, state, update_status |
| 11 | CI/CD & Docker Hardening | P1 | M | 0 | — | Dockerfile, compose, main.rs (migration flag), .github |
| 12 | Admin console auth gate (frontend) | P0 | M | 6 | backend admin auth contract, redacted MerchantResponse | admin/src only (no backend overlap) |
| 13 | Compliance Roadmap (KYC/AML/SOC2/etc.) | P0 | XL | 6 + parallel policy | auth, ledger, PII, failure states | confirm/initiate gate, new crates/migrations |

**Workstream notes**

- **Auth (1):** splitting the flat router into nested sub-routers risks silently dropping routes — verify all ~30 mount and compile. Constant-time api_key compare is only meaningful once keys are hashed at rest (follow-up). Webhooks stay on the public router (HMAC-gated) and must not inherit a JSON body extractor that consumes the raw bytes.
- **Idempotency (2):** the make-or-break detail is **not holding a sqlx tx across the 15s provider HTTP call** (only 10 pool connections). Use tx#1 (acquire key `in_progress` + Authorized→Processing CAS, commit) → provider call with deterministic key → tx#2 (finalize + `complete_key`). The `in_progress` row guards the gap.
- **Ledger (3):** available/pending split is derived from `journal_entries.status` (pending vs posted), requiring `create_journal_entry` to accept a status. Settlement moves to **webhook-time** (not submit-time), coordinated with Lifecycle to avoid double-posting.
- **PII (4):** prefer **tokenization** (Plaid processor token / MT external_account_id) to eliminate storage; envelope-encrypt only the manual-entry residual. Encryption does **not** substitute for auth — safe only alongside the IDOR/exchange-overwrite fixes.
- **Secrets (5):** rotation without history scrub leaves keys discoverable; scrub without rotation is useless. Do **both**, and pair the force-push with team coordination.
- **Outbox (6):** at-least-once delivery — merchants dedupe on the stable `X-Depost-Webhook-Id`. Keep the poll batch small (20) and use a dedicated acquire timeout so the worker doesn't starve the request pool. Never log full payload/signature.
- **Failure/Returns (7):** the transition matrix must tolerate provider **out-of-order** delivery (allow Authorized→Completed recovery edge) while forbidding downgrades out of Completed (except to Returned/Reversed). Reversal entries must mirror an existing settle journal or fall back to reconciliation flagging.
- **Config/Health (8):** the SSE stream (sessions.rs:104-148) loops indefinitely and will hold graceful shutdown open — rely on a deployment-level grace timeout. `acquire_timeout` converts hangs into 503s (intended behavior change).
- **Tests (9):** `sqlx` compile-time macros mean nothing builds without a live `DATABASE_URL` or a committed `.sqlx` cache — CI must own this. Env-mutating provider-selection tests must be `#[serial]`. The base-URL refactor must default to the exact current literals (covered by a default-equals-const test).
- **Observability (10):** dominant risk is **PII/secret leakage via logs/Sentry** — never serialize `PaymentSession`/`BankAccountDetails`/`Merchant`; log only ids + last-4. Audit immutability is only as strong as DB grants (INSERT/SELECT-only role + optional hash chain).
- **CI/Docker (11):** decoupling migrations from boot is a deliberate behavior change — wire the one-shot migrate step into CD or a replica boots un-migrated. `-D warnings` may surface a backlog; may need staged enforcement.
- **Admin gate (12):** frontend-only, **no real security alone** — the backend 401 is the control. Prefer an httpOnly same-origin cookie over localStorage (XSS) since the SPA is served same-origin from `/admin`. Scope the admin bearer to `/api/admin/*`; don't leak it to public session routes.
- **Compliance (13):** highest risk is **legal, not technical** — unlicensed money transmission and OFAC strict-liability. The compliance gate must **fail closed** (deny on any screening/verification uncertainty or provider outage). Adding a `kyc_status` CHECK will reject the seed's pre-set `verified` demo merchants — normalize/backfill first. The BSA 5-year retention overrides GDPR/CCPA deletion (legal-hold exception must be enforced in code).

---

## 6. What the Audit / Design MISSED or Under-Specified

1. **Wire-level transfer authorization vs. session capability is partly conflated.** The auth spec adds a `confirm_token` to gate initiate, but the existing public SSE/`/pay`/QR flow and the admin console's own `createSession`/`initiateTransfer` calls (admin `api.ts`) hit the **public** session surface. No workstream fully reconciles which principal (customer session token vs admin bearer) is authoritative for each money action — left as a "coordinate at integration" note. This is a real correctness gap, not just a merge concern, and should get an explicit auth-matrix artifact.
2. **`002_seed_data.sql` runs unconditionally in every environment** and ships **active, KYC-verified merchants with guessable plaintext API keys** (`pb_test_1234567890abcdef`). The Schema and Compliance audits each flag a facet, but **no workstream owns gating/removing the seed from production migrations** end-to-end. If shipped, these are live authenticated credentials in prod. Assign this explicitly (CI/Docker or Auth stream).
3. **Multiple background workers, one runtime, no coordination.** Five separate specs independently propose `tokio::spawn` loops (expiry sweeper, webhook delivery worker, AML monitor, rescreen job) plus the SSE streams — all sharing the **10-connection** pool. No workstream owns a unified worker-supervision/pool-budget design or ties the workers into graceful shutdown via a single `CancellationToken`. This risks pool starvation and workers that don't drain on SIGTERM. Needs a small "background runtime" design owned by Config/Ops.
4. **Migration numbering collision is unmanaged.** At least six workstreams independently propose a `migrations/003_*.sql`. `sqlx` orders by filename; two `003_` files will collide or apply nondeterministically. The program adopts a single coordinated migration sequence, but the audit/specs never call this out — flagged here as a hard coordination requirement.
5. **No reconciliation job against the providers.** Multiple streams say "for ambiguous errors, mark for reconciliation," but **no workstream builds the reconciler** that lists MT/Column payment orders by idempotency key and reconciles `Processing`/needs-reconciliation sessions. Without it, the "leave Processing" guidance creates a class of sessions that never resolve. This is a missing P1 workstream.
6. **`get_transfer_status` / `check_transfer_status` is never invoked for polling.** The provider audit fixes the missing HTTP-status check, but nothing schedules status polling, so settlement relies entirely on webhooks — which are themselves at-least-once and (for MT) currently unverified. A poll-based backstop is implied by "reconcile via status polling" but unassigned.
7. **`payment_attempts` table is defined and seeded-against but never used.** The Schema audit flags the dead `Payment` model and unused `idempotency_keys`, but does not address `payment_attempts` (migration 001) — no workstream populates it. Either wire it into the initiate retry/audit story or remove it; leaving it invites the same "false confidence" the dead `column_endpoint`/`choose_rail` create.
8. **Frontend pagination silently truncates the operations ledger.** The Admin/Merchant audit notes list pages fetch `limit` only with no `offset`, capping at 100-200 rows. For a money product this is a **reconciliation integrity** issue, but it is only tagged P2 and not assigned to the admin auth-gate frontend workstream (the only frontend stream in the plan). It will otherwise be orphaned.
9. **Webhook signature schemes are assumed, not verified against provider docs.** Both the Column (assumed raw-body-hex-HMAC) and MT (scheme TBD) verification designs are inferred. If Column's real scheme is versioned/timestamped, the "fix" still won't validate genuine signatures. No workstream includes a "validate against current provider docs / sandbox" acceptance step — it should.
10. **`error.rs` leaks raw provider error strings to clients** (ColumnError/MT/Plaid/Auth/Template arms). Tagged P2 in the auth/security audit but not owned by any workstream's file list. Fold into Observability (server-side detail, generic client message) explicitly.
11. **No load/capacity or rate-limit numbers.** Rate limiting is mentioned (tower-governor on login/webhooks/initiate) but with no targets, and the pool is 10 connections with no documented expected concurrency. For a money API the throughput envelope and the abuse thresholds (session-UUID brute force, MT object-id enumeration) should be quantified, not just "add a limiter."
12. **Stablecoin path is dead but present.** `PaymentMethod::Stablecoin` exists in models with no backing flow. Not money-safe to leave a half-modeled rail in an enum that drives Display/serialization without a clear "unsupported → reject" guard.

---

## 7. Recommended Sequencing Summary

```
Wave 0 (parallel):  Secrets · Config/Health · Test scaffold · CI/Docker
Wave 1 (parallel):  Auth (lead) · Observability/Audit
Wave 2 (paired):    Lifecycle/State-machine+CAS · Provider hardening (idempotency-key, errors, rails)
Wave 3 (coordinated):  Idempotency wiring · PII/Plaid hardening
Wave 4 (paired):    Ledger settle path · Failure/Returns reversal entries
Wave 5 (paired):    MT webhook signature (first) · Outbound webhook outbox+worker
Wave 6 (parallel):  Admin frontend gate · Compliance CODE  + fail-closed initiate flag
   ‖ continuous policy/business track: sponsor-bank, MSB/MTL, BSA/AML Officer, SOC2, privacy
```

**Go-live is gated on:** all 18 P0 blockers closed, the fail-closed compliance flag enabled only after legal sign-off, and the reconciliation/worker-supervision gaps in §6 assigned and resolved.