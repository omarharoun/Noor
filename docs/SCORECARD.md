# Noor Production-Readiness Scorecard

_Assessed 2026-05-31 against the original gap lists in docs/PRODUCTION_READINESS.md, docs/P0_REVIEW.md, docs/PRODUCTION_GATE.md. Status verified against current code; "done" only where confirmed in source._

## Headline

Noor closed every **code-closable** P0 in money-safety, ledger, webhooks, PII/crypto, and auth. **GO for a sandbox pilot.** **NO-GO for real money** until 4 code blockers + the human/vendor residue close.

## Counts

| done | partial | open | human_owned |
|---|---|---|---|
| 41 | 14 | 8 | 7 |

## Go / No-Go

- **Sandbox pilot: GO** — all four money-safety pillars, full route auth, real fail-closed inbound HMAC (MT + Column), AES-256-GCM PII at rest, and a verified fail-closed compliance chokepoint are in place. Run with sandbox keys, no real funds.
- **Real-money production: NO-GO.** Code blockers: (1) webhook Completed posts no settlement ledger entry; (2) public session GET leaks full customer PII; (3) Merchant struct re-leaks password_hash/api_key via Serialize; (4) CORS allow-any. Plus the human-owned residue below (sponsor bank, real KYC/OFAC vendor, secret rotation, KMS, SOC2/pentest) independently gates go-live.

---

## Auth & Security

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Admin JWT gating of /api/admin/* | done | router.rs:89-118 route_layer admin_auth; auth.rs:228-238; only login/me public (router.rs:160-161) | — |
| Operator accounts in DB + Argon2id + bootstrap | done | migrations/004_operators.sql:2-13; auth.rs:51-63 hash/verify, login 129-189, anti-enumeration dummy_hash, bootstrap 194-210 | — |
| Merchant API-key auth + IDOR closed | done | router.rs:135-153 merchant_auth; auth.rs:241-265 injects AuthedMerchant; merchant_api.rs:20-42 ignores path id | — |
| Operator router gates session create/list, initiate, transactions (was PUBLIC) | done | router.rs:124-132 route_layer admin_auth over the formerly-public money routes (P0_REVIEW.md:27) | No per-merchant scoping; operator-supplied merchant_id still trusted, but surface is operator-only now |
| Public session GET — bank account/routing redaction | partial | sessions.rs:116-120,142-143 returns only customer_account_last4 + bank_linked | **Still returns full customer_name/email/phone/address on a UUID-only unauthenticated endpoint (sessions.rs:133-145, verified). PII exposure unaddressed.** |
| password_hash / api_key never serialized | partial | Response paths safe: admin.rs:15-34 redact_merchant; operator_repo Operator is not Serialize | **paybank_core::Merchant (models.rs:171-191, verified) still derives Serialize incl. password_hash + api_key, no serde skip — latent re-leak (P0_REVIEW.md:60); use a DTO or skip the fields** |
| CORS configuration | open | router.rs:189 allow_origin(Any)/methods(Any)/headers(Any); comment 187 says tighten before public | Restrict allow_origin to env-driven operator/merchant origins |
| Request body limit | done | router.rs:197 DefaultBodyLimit::max(256 KiB) on merged router | — |
| Login brute-force throttle | partial | auth.rs:126-147 in-process per-email 10/300s; cleared on success | In-process only (per-replica, resets on restart), keyed by email not IP; needs shared/IP limiter for HA |
| JWT secret strength / fail-fast | done | state.rs:23-27 JWT_SECRET required, rejected if <32 bytes | — |
| Secret rotation of leaked provider/webhook creds | human_owned | docs/SECRETS.md:40-49 only PLAID_SECRET rotated; MT/Column/WEBHOOK_SECRET/PLAID_CLIENT_ID unrotated | Rotate at each vendor; also gate/remove seed pb_test_ keys (migrations/002:8,21) |

## Money-Safety

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Atomic CAS status transitions (try_transition) | done | session_repo.rs:92-116 single guarded UPDATE; used initiate.rs:50-57, sessions.rs:240-245, confirm.rs:61-68 | — |
| Confirm single-winner (Pending->Authorized), no keyless double-counterparty | done | confirm.rs:61-79; counterparty (114) only for winner | — |
| App-level idempotency on create + confirm (lock-before-work, replay, 409 mismatch) | done | idempotency.rs:45-71; sessions.rs:20-31/71-74; confirm.rs:49-56/166-168; idempotency_repo.rs:18-32 ON CONFLICT DO NOTHING | — |
| Provider Idempotency-Key to MT + Column | done | deterministic noor-transfer-{session_id} payments/lib.rs:74; moderntreasury.rs:182; column.rs:168 | — |
| Settlement gated on winning the CAS (no double-post TOCTOU) | done | initiate.rs:76-136 `if won`; sessions.rs:260-320 `if won`; ledger_repo.rs:131-159 inside won branch | — |
| Ambiguous vs definitive error classification | done | transport/5xx/429->ProviderAmbiguous (moderntreasury.rs:188-200, column.rs:173-184)->504 (error.rs:94); 4xx definitive; Processing left on ambiguous | — |
| DB-level UNIQUE(session_id) on journal_entries / transactions | open | Only plain indexes (001:86,126, verified); 003 adds no uniqueness; no in-code dedup | Add UNIQUE(session_id)/settlement-key so a post can't double even if CAS bypassed (CAS is sole defense, PRODUCTION_GATE.md:54) |
| Idempotency-key crash-wedge reaper (enforce expires_at) | open | expires_at written NOW+24h (idempotency.rs:52, verified) but never read/swept; begin() never SELECTs it | Read expires_at in begin() to reacquire stale in_progress, or add a sweep job (P0_REVIEW.md:43) |

## Ledger

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Double-entry settlement postings in payment flow | partial | record_settlement ledger_repo.rs:131-159 gated on CAS win at initiate.rs:86-115, sessions.rs:270-299, reconcile.rs:72 | **GAP (verified): webhooks.rs:66-68 Completed branch transitions status + enqueues outbox but NEVER calls record_settlement — async-webhook-only completion (normal ACH path) posts NO ledger entry, understated balance. Add record_settlement on webhook Completed gated on `changed`.** |
| Real get_balance (not hardcoded) | done | merchant_api.rs:20-42 authed merchant_id; merchant_available_cents ledger_repo.rs:198-211; pending session_repo.rs:159 | — |
| Per-merchant chart-of-accounts provisioning | done | ensure_merchant_accounts ledger_repo.rs:107-125 idempotent; at creation admin_repo.rs:211 + lazily before each post | — |
| Compensating reversals on Returned/Reversed | done | record_reversal ledger_repo.rs:166-194; webhooks.rs:109-124 (verified), reconcile.rs:78 | — |
| verify_balance enforcement | done | ledger_repo.rs:61-90 requires debits==credits>0; rollback on settlement 151-156 + reversal 186-191 | — |
| Health unbalanced-entries integrity check | done | admin_repo.rs:327-360 GROUP BY HAVING sum(debit)<>sum(credit); admin.rs:217 | — |
| Money-safety DB constraints (migration 003) | partial | 003:5-15 direction/amount CHECKs but all NOT VALID (lines 7,11,15) | VALIDATE after backfill; still no UNIQUE(session_id) on journal_entries |

## Webhooks

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Inbound HMAC — Column (constant-time, raw body, fail-closed) | done | webhooks.rs:19-30 verify_slice; 130-145 fail-closed, verify before parse 147; router.rs:169 Bytes extractor | — |
| Inbound HMAC — Modern Treasury (primary rail) | done | webhooks.rs:166-187 verify over raw body before parse (closes PRODUCTION_READINESS.md:39) | Live MT format (base64/structured) unvalidated — see human_owned item |
| Replay-backward protection (CAS-guarded transitions) | done | webhooks.rs:56-82 explicit allowed-from sets; terminal can't move backward | — |
| Webhook replay protection (timestamp/nonce on inbound) | open | webhooks.rs — no timestamp window, no event-id dedupe (P0_REVIEW.md:49) | Add timestamp tolerance + persisted provider-event-id dedupe |
| Per-provider webhook secrets (drop shared fallback) | partial | MT prefers MT-specific then falls back to WEBHOOK_SECRET (174-176); Column uses only WEBHOOK_SECRET (135) | Give Column its own secret; drop shared fallback (P0_REVIEW.md:54) |
| Provider signature scheme validated against live format | human_owned | webhooks.rs:25 assumes hex; PRODUCTION_READINESS.md:190 schemes inferred | Validate against Column + MT sandbox with real signatures |
| Outbound worker — spawned, signed, batched, timeout | done | webhook_worker.rs:33-46 (5s tick, batch 20, 10s timeout), HMAC sign 20-24, headers 91-98; main.rs:58 | — |
| Outbound — capped exponential backoff | done | webhook_worker.rs:27-30 (30s base, x2, cap 3600s, overflow-safe); webhook_repo.rs:99-105; tests 117-185 | — |
| Lease-based stuck-'sending' reaper | done | webhook_repo.rs:43-63 claim_due includes 'sending', 60s lease bump | — |
| Exhausted terminal state | done | webhook_repo.rs:86-107 status='exhausted' at attempts>=6; not reclaimed | — |
| Outbox emission on real transitions (CAS-gated) | done | initiate.rs:117, sessions.rs:301, confirm.rs:140, webhooks.rs:95-106; enqueue webhook_repo.rs:18-37 | — |
| Outbox enqueue is fire-and-forget (errors swallowed) | open | All call sites `let _ = enqueue(...)` (webhooks.rs:95, initiate.rs:117, sessions.rs:301, confirm.rs:140) | Log/metric on enqueue failure or make transactional with state change |
| Admin retry endpoint | done | admin.rs:320-340 auth-gated, audit-logged; retry_now webhook_repo.rs:132-141; router.rs:104-106 | — |
| Admin retry does NOT reset attempts | partial | retry_now updates only status+next_retry_at (verified 132-141); claim_due bumps attempts; retrying exhausted gives exactly 1 more attempt | Reset attempts=0 in retry_now |
| Admin retry UI | done | admin/src/pages/Webhooks.tsx:10-17,51-54; api.ts:200; Health.tsx:68-69 | — |
| DB CHECK on webhook_events.status | open | 001:156-167 plain TEXT, no CHECK; no later migration | Add CHECK (status IN pending/sending/failed/delivered/exhausted) |
| Worker graceful-shutdown integration | open | main.rs:58-60 detached tasks; webhook_worker.rs:39 unconditional loop, no CancellationToken | Thread CancellationToken + await drain on SIGTERM |
| Session-id derivation from free-text description | partial | webhooks.rs:153-156 (Column) / 202-205 (MT) strip 'Session ' prefix; no persisted transfer_id->session map | Persist provider transfer id at initiate and look up by it |

## PII & Crypto

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| AES-256-GCM PII at rest, versioned ciphertext | done | crypto.rs:11-17 enc:v1: prefix; 55-73 random nonce + base64(nonce||ct); 77-95 decrypt; key 64-hex/base64->32 bytes | — |
| Encrypt-on-write / decrypt-on-read at repo boundary | done | session_repo.rs:222-223 encrypt before UPDATE; 54-62 decrypt after get_session | — |
| Public redaction + serde skip (close LEAK#1/#2) | done | models.rs:365-368 skip_serializing on account/routing; admin list/detail (admin.rs:94,104) drop them; public masked last4 | — |
| Fail-closed in production (NOOR_ENV) | done | main.rs:26-29 bail if production && !crypto::is_configured(); crypto.rs:50-52,24-42 | In-module path still fail-OPEN in non-prod (safe behind startup gate) |
| KMS / envelope-encryption upgrade path | human_owned | DEPLOY.md:64-72 plan; crypto.rs:23 static env key; no DEK code | Choose KMS, provision creds, implement per-record DEK + KEK rotation |
| Production secret mgmt + rotation for PII key | human_owned | PRODUCTION_GATE.md:74; DEPLOY.md:34; key in env (crypto.rs:23) | Move to secrets manager, define rotation, rotate any leaked key |

## Reconciliation & Failure Handling

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Reconciliation poller for stuck Processing | done | reconcile.rs:27-95 (60s tick, 300s, batch 50); main.rs:60; session_repo.rs:137-155 | — |
| Provider derived from primary() (not hardcoded) | partial | reconcile.rs:19-25 active_provider via primary(); payment_sessions.provider (001:64) never written/read | Populate + read per-session provider; non-primary session would poll wrong provider |
| CAS + ledger resolution on reconcile | done | reconcile.rs:71-85 settle/reverse gated on try_transition from [Processing] | — |
| Ref-less stuck sessions logged for manual recon (no guessing) | done | reconcile.rs:58-60 warn + skip; map_provider_status None on unknown 38-46 | — |
| Ambiguous-timeout sessions eventually auto-resolve | partial | initiate.rs:144-152 left Processing with NO ref; reconciler always hits ref-less manual branch | Need idempotency-key-based provider lookup to recover transfer id |
| Returned/Reversed/Failed into distinct terminals | done | webhooks.rs:34-43 distinct (never Expired); both handlers verify HMAC then route | — |
| No terminal-backward moves; replayed webhook safe | done | webhooks.rs:65-82 explicit from-sets; side-effects only when changed | — |
| Compensating ledger reversal on Returned/Reversed (webhook) | done | webhooks.rs:108-124 record_reversal once on winning CAS; failure logged | — |
| No false-Failed on ambiguous | done | initiate.rs:143-162 only non-ambiguous->Failed; reconcile.rs:44 unknown->None; tests 101-145 | — |
| Ledger errors surfaced (not swallowed) in reconciler | partial | reconcile.rs:72,78 `let _ =` discard results (vs webhooks.rs:118-122 which logs) | Log/alert on ledger failure; dead-letter/aging cap for manual queue |

## Compliance / KYC

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Fail-closed compliance gate before authorization | done | confirm.rs:39-45 gate before claim; compliance.rs:105-131 KYC + screen + Err=>deny | — |
| Gate is single chokepoint (sole Authorized producer; initiate requires Authorized) | done | confirm.rs:61-68 only Pending->Authorized; initiate.rs:53 requires Authorized | — |
| Gate enforces REAL merchant KYC (not hardcoded) | done | compliance.rs:111-119 reads merchant.kyc_status; admin_repo.rs:301-313 real column | — |
| Screening fails closed on provider error | done | compliance.rs:121-130 Err=>ComplianceRejected; tests 38-98; error.rs:99 ->403 | — |
| Real OFAC/sanctions/KYC vendor (screen_customer is STUB) | open | compliance.rs:18-32 hardcoded DENY_SUBSTRINGS (verified), documented placeholder | Replace with real vendor/MT compliance; OFAC re-screen at initiate; persist screened_at/result; add 'blocked' status + review queue |
| Vendor procurement + sponsor-bank/BSA-AML program | human_owned | PRODUCTION_GATE.md:72; COMPLIANCE.md:40-47; PRODUCTION_READINESS.md:140 | Sign sponsor-bank agreement, register MSB/MTLs, name BSA/AML officer, contract vendor, wire behind compliance.rs |
| get_merchant_profile hardcodes kyc_status "verified" | partial | merchant_api.rs:44-50 hardcoded; NOT used by the gate (display-only, no bypass) | Back with real merchant data |
| KYC CHECK constraint / state machine + seed normalization | open | 001:16 no CHECK; 003 no kyc CHECK; 002:10,24 seeds verified merchants (verified) | Add kyc_status CHECK/enum + onboarding state machine; gate/remove seed verified merchants in prod |
| Webhooks advance past gate without re-screening | partial | webhooks.rs:67-73 Pending->Processing/Completed allowed; HMAC-verified, no outbound transfer (no money bypass) | If real screening + initiate re-screen added, align webhook-driven completions |

## Ops / DB / CI / Tests / Observability

| Item | Status | Evidence | Remaining |
|---|---|---|---|
| Typed Config + fail-fast at boot | done | state.rs:18-39 (JWT_SECRET>=32 bail); main.rs:22,26-29 prod PII fail-closed | — |
| Graceful shutdown (SIGTERM/Ctrl-C) | done | main.rs:69 with_graceful_shutdown; 76-98 ctrl_c + SIGTERM | Workers + SSE not on a shared CancellationToken; SSE loop can hold shutdown open |
| DB readiness probe | done | router.rs:19-27 SELECT 1 ->200/503; GET /health/ready 159 | — |
| Neon Postgres wiring (sqlx tls-rustls) | partial | paybank-db Cargo.toml tls-rustls; least_privilege_role.sql Neon; DEPLOY.md Supabase | DATABASE_URL raw (main.rs:31), no sslmode in code; Neon vs Supabase docs inconsistent; endpoint/sslmode is deploy-time |
| Boot-vs-deploy migrations + NOOR_SKIP_MIGRATE | done | main.rs:45-49; DEPLOY.md:57-59 | — |
| Least-privilege DB role (docs/sql) | done | least_privilege_role.sql:10-24 SELECT/INSERT/UPDATE/DELETE only, no DDL; ALTER DEFAULT PRIVILEGES | Applying it (create role, switch URL, migrate as owner) is a human deploy action; audit_log immutability relies on not granting DELETE |
| CI workflow | done | ci.yml: postgres svc, migrate, fmt --check, clippy -D warnings, sqlx prepare --check, release build, test, admin build, gitleaks | No docker-build job; test job is unit-only |
| .sqlx offline cache committed | done | git ls-files .sqlx returns JSONs; Dockerfile:24 COPY + SQLX_OFFLINE | Harmless: not in .gitignore listing |
| Dockerfile hardening | done | multi-stage, non-root uid/gid 10001, HEALTHCHECK curl /health, minimal runtime, release | — |
| audit_log table + repo + call sites + Health feed | done | 005_audit_log.sql:2-15 append-only; audit_repo.rs:25-60; wired auth.rs:170, initiate.rs:126, confirm.rs:149, sessions.rs:310, admin operators; Health.tsx:155-186 | No DB immutability/hash-chain; append-only relies on role lacking DELETE |
| Unit tests (count) | partial | 31 test fns (crypto 1, compliance 6, reconcile 7, idempotency 5, webhook_worker 6, rail 6); zero #[sqlx::test], no tests/ dir (verified) | NO integration/DB tests; no end-to-end regression guard for CAS double-fire, HMAC valid/invalid/missing/replay, ledger balance+reversal, idempotency replay/mismatch, auth-on-every-route (PRODUCTION_GATE.md Wave F) |
| Structured logging | done | main.rs:13-19 tracing_subscriber json + EnvFilter; audit_repo.rs:47 structured | — |
| Observability: request-id / TraceLayer / metrics / Sentry | open | main.rs:11-12 comment defers TraceLayer; grep found none; no prometheus/Sentry | Add request-id + TraceLayer (matched-path, no PII), Prometheus /metrics, Sentry with PII scrubbing |
| Pool tuning (acquire_timeout etc.) | partial | main.rs:32-40 only max_connections (default 10); no acquire/idle/max_lifetime/min | Add acquire_timeout (hang->503) + budget across background workers sharing 10 conns |
| Secret rotation status | human_owned | SECRETS.md:40-49 only PLAID_SECRET rotated; MT/Column/WEBHOOK_SECRET/PLAID_CLIENT_ID unrotated | Rotate at each vendor (code can't un-leak); KMS path documented (DEPLOY.md:64-72) not implemented |
| Seed credentials gating (002_seed_data.sql) | open | 002:8,21 inserts active pb_test_ api_keys, kyc 'verified' on every migrate (verified) | No workstream owns gating/removing seed from prod; these are live authenticated creds if 002 runs in prod |