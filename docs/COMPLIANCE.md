# Depost — Compliance & Go-Live Roadmap

_Owner: Compliance Lead. Status: active. Last revised: 2026-05-31._
_Scope: Depost is a U.S. instant bank-to-bank (ACH / RTP / FedNow / Wire) money-movement platform. It operates as a Rust + axum monolith backed by Postgres, routes real-value payment orders through Modern Treasury (primary) and Column (fallback), and links customer bank accounts via Plaid. This document is the single source of truth for regulatory, legal, data-protection, security, and operational compliance requirements, and their go-live gates._

> **READ FIRST — CURRENT STATE SUMMARY.**
> The platform is a prototype with confirmed critical gaps: (1) every `/api/admin/*` and `/api/merchant-api/*` route is unauthenticated (`router.rs` mounts all ~30 routes with zero middleware); (2) customer bank account and routing numbers are stored and transmitted in plaintext (`payment_sessions.customer_account_number`, `customer_routing_number`); (3) the Modern Treasury webhook at `/api/webhooks/moderntreasury` performs no signature verification — any attacker can forge a settlement; (4) all payment failures, returns, and reversals collapse into a single `Expired` status, destroying the audit trail needed for regulatory recordkeeping; (5) no KYC/KYB gate, no OFAC screening, no AML program is wired into the payment flow. **None of these gaps prevent development. All of them prevent lawful production operation.**

---

## Table of Contents

1. [Regulatory Model & Licensing](#1-regulatory-model--licensing)
2. [KYC / KYB — Identity & Business Verification](#2-kyc--kyb--identity--business-verification)
3. [BSA / AML Program](#3-bsa--aml-program)
4. [Data Protection & Privacy](#4-data-protection--privacy)
5. [Security & Audit Controls](#5-security--audit-controls)
6. [Operational Controls](#6-operational-controls)
7. [Prioritized Go-Live Checklist](#7-prioritized-go-live-checklist)

---

## 1. Regulatory Model & Licensing

### 1.1 The Central Gate

Moving money in the United States without a legal authorization model is unlicensed money transmission — a federal and state crime. **This is the single gate that supersedes all code work.** The `initiate_transfer` path must remain fail-closed (see §7) until a legal authorization model is signed and in effect.

### 1.2 Option A — Money Services Business (MSB) + State Money Transmitter Licenses

Operating independently as a money transmitter requires:

- **FinCEN MSB Registration** [BUSINESS/LEGAL] — Register as a Money Services Business with FinCEN under 31 C.F.R. § 1022.380. Must be renewed every two years. This is a federal floor, not a ceiling.
- **State Money Transmitter Licenses (MTLs)** [BUSINESS/LEGAL] — 49 U.S. states and D.C. each require a separate MTL to transmit money to or from residents of that state. Montana has no MTL; all others do. License acquisition takes 3–24 months per state, costs $500–$150,000 per state in fees plus surety bonds, and requires audited financials, background checks on principals, and a written AML program as a pre-condition. Multi-state coverage is typically pursued through the NMLS (Nationwide Multistate Licensing System). **Obtaining a full 50-state stack takes 12–36 months and $2–5M+ in combined fees, bonds, and compliance overhead for a startup.**

*Assessment:* This path is impractical for a pilot and should not be the first route chosen.

### 1.3 Option B — Sponsor Bank / BaaS Model (Recommended for Early Stage)

Most payment startups, including those using Modern Treasury and Column, launch under a **sponsor bank** (also called a "banking-as-a-service" or "bank partner" model). Under this structure:

- A chartered bank (e.g., Column N.A. itself, Evolve Bank & Trust, Thread Bank, Lead Bank) holds the MTLs and is the regulated entity moving money.
- Depost operates as a **program manager** or **agent of the bank**, processing payment instructions on behalf of the bank under a written agreement.
- The bank's existing MTLs and FinCEN MSB registration cover Depost's activity, provided Depost implements the bank's BSA/AML program requirements (see §3).
- Column is already integrated as a payment provider — they offer a direct bank API with a BaaS framework. Confirm whether Depost's current Column agreement constitutes a sponsor arrangement or merely an API license.

**Practical recommendation:** Execute a Program Manager / Agent Agreement with a FDIC-insured sponsor bank before any production payments. The sponsor bank will mandate: (a) a written BSA/AML program, (b) a named BSA/AML Officer, (c) KYC/KYB of all merchants and customers above applicable thresholds, (d) OFAC screening, and (e) incident reporting. All of §§2–6 of this document are, effectively, the sponsor bank's pre-conditions.

### 1.4 NACHA Membership and ACH Access

[BUSINESS/LEGAL] Depost uses the ACH rail. Direct ACH origination requires either:
- Being an **ODFI** (Originating Depository Financial Institution) — a chartered bank. Depost is not.
- Contracting with an ODFI that acts as your originating gateway. Modern Treasury and Column each have ODFI relationships that cover Depost's ACH origination when using their APIs. Confirm this is explicitly covered in your agreements with each provider.

NACHA's Operating Rules bind all ACH participants. Key obligations for Depost as a **Third-Party Sender** (TPS):
- Annual volume-based registration with NACHA if ACH credits or debits exceed thresholds.
- Written agreement with ODFI acknowledging Third-Party Sender rules.
- Return rate monitoring: must not exceed 0.5% unauthorized debit return rate or 3% overall debit return rate; breaches trigger NACHA investigation and potential suspension.
- NACHA's data security requirements require encryption of account data (directly maps to §4.2 below).

[CODE] `PaymentRail::Ach` is the default fallback in `rail.rs::choose`. Confirm that the ODFI relationship via Modern Treasury or Column covers Depost's origination volume at scale, not just sandbox.

### 1.5 FedNow and RTP Participant Requirements

[BUSINESS/LEGAL]
- **FedNow:** Only FDIC/NCUA-insured depository institutions may be direct FedNow participants. Depost must originate FedNow payments through a participant bank (Modern Treasury's or Column's banking relationship). Confirm your provider agreement explicitly grants FedNow access.
- **RTP (The Clearing House):** Same constraint — RTP participants must be federally-insured depository institutions. Modern Treasury and Column each have RTP access; this must be covered in the API agreement.

[CODE] `PaymentRail::FedNow` and `PaymentRail::Rtp` are currently selected based on the `banks` table flags (`supports_fednow`, `supports_rtp`). There is no runtime verification that the provider has an active participant relationship for those rails. Add a provider capability check at session creation.

### Licensing Checklist

- [ ] [BUSINESS/LEGAL] Identify and execute a sponsor bank / Program Manager agreement before any production payment.
- [ ] [BUSINESS/LEGAL] Obtain written confirmation from Modern Treasury and Column that their agreements cover: (a) ACH Third-Party Sender, (b) FedNow origination, (c) RTP origination for Depost's use case and expected volume.
- [ ] [BUSINESS/LEGAL] Register as FinCEN MSB upon go-live (even under a sponsor bank, registration may be required depending on structure — confirm with counsel).
- [ ] [BUSINESS/LEGAL] Engage payments counsel to render a legal opinion on the regulatory model and confirm state-by-state licensing obligations.
- [ ] [BUSINESS/LEGAL] Do not onboard customers in states where the sponsor bank's coverage is ambiguous without explicit legal sign-off.
- [ ] [CODE] Implement a `PAYMENT_INITIATION_ENABLED` feature flag (fail-closed `false`) gating the `initiate_transfer` code path. The flag is only set `true` after legal sign-off. (See §7.)

---

## 2. KYC / KYB — Identity & Business Verification

### 2.1 Regulatory Basis

The Bank Secrecy Act (BSA), FinCEN's Customer Identification Program (CIP) rule (31 C.F.R. § 1020.220), and FinCEN's Customer Due Diligence (CDD) rule (31 C.F.R. § 1010.230) require that Depost (or its sponsor bank, with Depost performing verification as the program manager):

- Collect and verify the identity of each customer before opening an account or facilitating a transaction.
- Collect and verify Beneficial Ownership information for legal entity customers (any entity with ≥ 25% ownership and the controlling person).
- Maintain records for five years.

### 2.2 Current Code State

The `merchants` table has the data scaffolding:
- `kyc_status` (default `'not_started'`), `risk_level` (default `'medium'`)
- `business_name`, `business_type`, `registration_number`, `tax_id`, `industry_category`
- `status` (default `'pending'`), `onboarding_completed_at`

**However, no KYC/KYB logic exists.** `kyc_status` is a free-text field with no state machine, no CHECK constraint, and no enforcement at any payment endpoint. The seed data (`002_seed_data.sql`) ships demo merchants with `kyc_status = 'verified'` and a plaintext guessable API key (`pb_test_1234567890abcdef`) — if this migration runs in production, those merchants are live verified credentials. The `confirm` and `initiate` handlers do not check `kyc_status` at all.

### 2.3 Customer (Consumer) KYC

[CODE] For consumer payments via `/pay/:id` and `/api/sessions/:id/confirm`:
- Collect: full legal name, date of birth, SSN (last 4 at minimum for low-risk, full for high-value), address, government ID type. For ACH credits above $3,000 or any suspicious activity, a more complete profile is required.
- Verify via a third-party identity verification vendor (e.g., Socure, Alloy, Persona, Jumio). Plaid Identity (a separate Plaid product from `auth`) can also perform name-on-account verification.
- Store a verification result record linked to the session. Do not store raw SSN in `payment_sessions`; store a tokenized reference.
- For the current flow, `confirm.rs` hardcodes `customer@example.com` and `"5555555555"` as placeholder PII — this is not a CIP-compliant record.

[POLICY] Define risk tiers: standard (name + DOB + address), enhanced (+ government ID), high-risk (+ manual review). Apply enhanced due diligence to transactions above $10,000 or flagged by the risk model.

### 2.4 Merchant (Business) KYB

[CODE] Implement the merchant onboarding state machine:
```
not_started → in_review → pending_documents → verified | rejected
```
Add a PostgreSQL CHECK constraint on `merchants.kyc_status` to enforce valid transitions. Set `onboarding_completed_at` only when status transitions to `verified`.

Required collection fields (already in schema, not yet validated):
- `business_name`, `business_type`, `registration_number` (EIN/DUNS), `tax_id`, `industry_category`, `website_url`
- Beneficial owners: name, DOB, SSN, address, ownership percentage for all individuals owning ≥ 25%.
- Control person: name, DOB, SSN, address.

[CODE] The `merchant_api` route group hardcodes `DEMO_MERCHANT_ID`. Every merchant API call must resolve a real, `kyc_status = 'verified'` merchant via the authenticated API key (see §5.1).

[CODE] Block payment session creation (`POST /api/sessions`) if the merchant's `kyc_status` is not `verified`. Return `403 Merchant account not verified` and log the attempt to the audit log.

[POLICY] Maintain a written KYB policy document specifying: accepted business types, prohibited categories (unlicensed money transmission, gambling without license, adult content, OFAC-sanctioned jurisdictions), required document retention, and periodic review cadence (annual re-verification for standard, semi-annual for elevated-risk).

### 2.5 Beneficial Ownership

[BUSINESS/LEGAL] FinCEN's CDD rule requires collection of beneficial ownership for all legal entities (not sole proprietors). The sponsor bank will mandate this as a pre-condition for the program manager agreement. Add a `beneficial_owners` table linked to `merchants` capturing: name, DOB, SSN (encrypted — see §4), address, ownership percentage, ID document type and number, and verification status.

### KYC/KYB Checklist

- [ ] [CODE] Add `CHECK (kyc_status IN ('not_started','in_review','pending_documents','verified','rejected'))` constraint on `merchants.kyc_status`.
- [ ] [CODE] Add `kyc_status = 'verified'` gate in `POST /api/sessions` handler.
- [ ] [CODE] Add `kyc_status = 'verified'` gate in both `initiate` handlers.
- [ ] [CODE] Remove hardcoded `DEMO_MERCHANT_ID` from `merchant_api.rs`; resolve merchant from authenticated API key.
- [ ] [CODE] Remove the `kyc_status = 'verified'` seed in `002_seed_data.sql` from production migrations or replace with `not_started`.
- [ ] [CODE] Integrate a third-party KYC/KYB provider (Alloy, Persona, or Socure recommended).
- [ ] [CODE] Create `beneficial_owners` table and KYB collection endpoint.
- [ ] [POLICY] Author and board-approve a written CIP/CDD program document.
- [ ] [BUSINESS/LEGAL] Confirm beneficial ownership collection obligations with payments counsel.

---

## 3. BSA / AML Program

### 3.1 Regulatory Basis

The Bank Secrecy Act (31 U.S.C. § 5318) requires any MSB (and any program manager acting on behalf of a bank) to implement a written anti-money laundering program with five pillars: (1) internal controls, (2) a designated BSA/AML compliance officer, (3) ongoing training, (4) independent audit, and (5) customer due diligence (added by the CDD rule). Failure to maintain an adequate AML program is a federal crime with strict-liability civil penalties and criminal exposure for officers.

### 3.2 Written BSA/AML Program

[POLICY] Draft, adopt, and maintain a written BSA/AML program covering at minimum:
- Risk assessment of Depost's business model, customer types, geographies, and rails (FedNow/RTP/ACH/Wire each carry different risk profiles).
- Customer identification and KYC procedures (§2).
- Transaction monitoring rules and thresholds.
- SAR and CTR filing procedures.
- Recordkeeping requirements.
- Employee training plan and schedule.
- Independent audit schedule (annually minimum).

The program must be approved by senior management (or the board if Depost is a standalone entity) and reviewed at least annually.

### 3.3 BSA/AML Compliance Officer

[BUSINESS/LEGAL] Designate a named individual as the BSA/AML Compliance Officer before any production payments. This person must have sufficient authority, independence, and expertise. For a startup under a sponsor bank, the sponsor bank will often require their prior approval of the nominee. The role cannot be vacated without a replacement.

### 3.4 OFAC / Sanctions Screening

OFAC administers U.S. sanctions programs. Facilitating a transaction involving a Specially Designated National (SDN) or a sanctioned country is a strict-liability civil violation regardless of intent. Penalties are per-transaction and can reach $1M+.

[CODE] Implement OFAC screening at two points in the payment flow:

**Point 1: Session Confirm** (`POST /api/sessions/:id/confirm`):
- Screen the customer's full name + address against the OFAC SDN list, OFAC Consolidated Sanctions list, and applicable third-party watchlists (PEPs, adverse media) via a vendor (e.g., Sanctions.io, Dow Jones Risk & Compliance, LexisNexis).
- If the screen result is a match or potential match: block the session (return `403 Payment blocked — compliance review required`), set `payment_sessions.status = 'blocked'` (requires adding this status — see §6.1), enqueue for manual review, and log to the audit log with the match detail.
- If the screening service is unavailable: **fail closed** — block the transaction and alert the compliance officer. Do not allow a "screen unavailable, proceed anyway" path.

**Point 2: Initiate** (`POST /api/sessions/:id/initiate`):
- Re-screen the counterparty identifiers (name, account ownership) immediately before calling `initiate_transfer`. This catches any watchlist updates between confirm and initiate.
- Log the screening result (pass/fail/manual-review) to the audit log with a timestamp.

[CODE] Add a `sanctions_screened_at` timestamp and `sanctions_result` (pass / blocked / review) to `payment_sessions`.

[POLICY] Define the match adjudication workflow: who reviews potential matches, the decision timeline (24 hours maximum for real-time rails), and the escalation path to the BSA/AML Officer.

[POLICY] Retain all screening results for 5 years (BSA recordkeeping requirement).

### 3.5 Transaction Monitoring

[CODE] Implement a transaction monitoring worker that evaluates completed and in-flight transactions for AML red flags. Minimum rules at launch:
- **CTR threshold:** aggregate cash-equivalent transactions by a single customer exceeding $10,000 in a single business day — flag for CTR filing. (Note: bank-to-bank ACH/RTP/Wire are not "cash" but the aggregation threshold is still a monitoring trigger for structuring detection.)
- **Structuring detection:** series of transactions just below $10,000 by the same customer within a rolling 2-day window.
- **Velocity rules:** more than N transactions per customer per day (configurable, starting at 5); total daily volume per merchant above a configurable threshold.
- **High-risk geography:** transactions involving IP addresses or bank routing numbers associated with high-risk jurisdictions.
- **Wire threshold:** all wires above $3,000 require enhanced due diligence under FINCEN's "travel rule" (counterparty name and account number — most of this is already collected by Depost's confirm flow).

[CODE] The `transactions` table and `ledger_postings` are the data sources. Both the `LedgerRepo` and `IdempotencyRepo` currently have zero call sites in the codebase — the ledger must be wired into the settlement path (see `PRODUCTION_READINESS.md` Wave 4) before transaction monitoring can operate on real data.

[POLICY] Establish velocity and threshold configurations as policy-owned parameters, not hardcoded constants. They must be adjustable without a code deployment in response to emerging typologies.

### 3.6 SAR and CTR Filing

[POLICY] Document and implement procedures for:
- **Suspicious Activity Reports (SARs):** File with FinCEN within 30 days of detecting suspicious activity (60 days if no suspect is identified). SAR amounts of $5,000+ involving a known suspect or $25,000+ regardless. Maintain strict confidentiality — do not tip off the subject.
- **Currency Transaction Reports (CTRs):** File with FinCEN for any transaction or series of transactions involving more than $10,000 in cash (or cash equivalents) by a single person in a single business day. Bank-to-bank electronic payments are generally not "cash" but consult with counsel on your specific rails.

[CODE] Add a `compliance_flags` table (or a `compliance_reviews` table) to track flagged transactions, SAR/CTR filing status, decision audit trail, and the reviewing officer. Automate FinCEN E-Filing API integration for CTR submission at scale.

### 3.7 Recordkeeping

The BSA requires retention of:
- All records related to the identity of customers (5 years from the date of account closure or last transaction).
- All records of transactions transmitted through Depost (5 years from the date of the transaction).
- SAR filings and supporting documentation (5 years from the date of filing).
- CTR filings (5 years).

[CODE] Implement a retention enforcement mechanism: a scheduled job that applies legal-hold flags and prevents deletion of records within their retention window. The BSA 5-year retention obligation **overrides** GDPR/CCPA deletion requests for records within retention (see §4.5).

### BSA/AML Checklist

- [ ] [POLICY] Author and board-approve a written BSA/AML program.
- [ ] [BUSINESS/LEGAL] Designate a named BSA/AML Compliance Officer.
- [ ] [CODE] Integrate a real-time OFAC/sanctions screening vendor into the `confirm` handler.
- [ ] [CODE] Add OFAC re-screen at `initiate` with fail-closed behavior on vendor outage.
- [ ] [CODE] Add `sanctions_screened_at` and `sanctions_result` columns to `payment_sessions`.
- [ ] [CODE] Implement transaction monitoring worker (velocity rules, structuring detection, CTR threshold alerts).
- [ ] [CODE] Add `compliance_reviews` table for flagging, SAR/CTR tracking, and officer audit trail.
- [ ] [CODE] Implement BSA 5-year legal-hold on all transaction and identity records.
- [ ] [POLICY] Establish SAR and CTR filing procedures with clear escalation paths.
- [ ] [POLICY] Establish annual independent AML audit schedule.

---

## 4. Data Protection & Privacy

### 4.1 GLBA Safeguards Rule

The Gramm-Leach-Bliley Act Safeguards Rule (16 C.F.R. Part 314, revised effective June 2023) applies to all "financial institutions" — which includes payment processors and MSBs. It requires a written information security program administered by a qualified individual, with:
- A risk assessment covering customer information (NPI — Nonpublic Personal Information).
- Safeguards covering access controls, encryption, multi-factor authentication for systems containing customer data, monitoring, incident response, and vendor oversight.
- Annual reporting to the Board.

[POLICY] Author and maintain a GLBA Information Security Program document. This must be signed off by a qualified security individual and reported to the board annually.

[CODE] The Safeguards Rule's encryption requirement directly mandates that customer bank account numbers be encrypted at rest and in transit. Currently they are stored in plaintext. See §4.2.

### 4.2 NACHA Data Security Requirements

NACHA's Operating Rules require:
- **Encryption of account data at rest and in transit** for all ACH participants and their service providers. This applies to Depost for the account numbers and routing numbers it handles.
- **Tokenization preference:** NACHA encourages substituting account numbers with tokens (e.g., Plaid Processor Tokens, or the external account IDs issued by Modern Treasury/Column) to minimize the exposure surface.

[CODE] **Critical gap:** `payment_sessions.customer_account_number` and `customer_routing_number` are stored as plaintext `TEXT` columns (confirmed in `crates/paybank-core/src/models.rs`, `PaymentSession` struct and the `confirm.rs` handler which reads and stores these values directly). The `Payment` model also carries `customer_account_number` and `customer_routing_number` as plaintext fields.

Remediation path (in order of preference):
1. **Tokenization (preferred):** Use Plaid Processor Tokens (a Plaid product upgrade from `auth`) or Modern Treasury's External Account IDs / Column's entity/account IDs — these replace the raw account number entirely. Modify the Plaid exchange flow in `plaid.rs` to create a processor token instead of returning raw numbers.
2. **Envelope encryption (required for any residual plaintext):** Encrypt residual account numbers with AES-256-GCM. Use a Key Encryption Key (KEK) stored in a hardware-backed KMS (AWS KMS, GCP Cloud KMS, HashiCorp Vault). The Data Encryption Key (DEK) is wrapped by the KEK. Bind the authenticated data (AAD) to the session ID to prevent cross-record decryption attacks.
3. After encryption and verification, drop the plaintext columns in a follow-up migration.

[CODE] The `get_session_detail` admin handler (`admin.rs`) and the `list_sessions` handler serialize the full `PaymentSession` struct including raw account and routing numbers to the browser. These must be redacted from all API responses — return only a masked last-4 representation (e.g., `"****1234"`) and never the full number.

### 4.3 PCI DSS Scope

Depost does not handle payment card data (no card numbers, CVVs, or PAN). Assuming this remains true:
- Depost is likely **out of scope for PCI DSS** and should have minimal PCI obligations.
- Complete a PCI DSS SAQ (Self-Assessment Questionnaire) — likely **SAQ A** or **SAQ D-SP** depending on the sponsor bank's requirements — to formally confirm and document out-of-scope status.
- If Depost ever adds card acceptance in the future, scope expands dramatically and this section must be revisited.

[BUSINESS/LEGAL] Obtain written confirmation from the sponsor bank that Depost's current card-out-of-scope posture is acceptable under their program.

### 4.4 GDPR and CCPA

Depost may process personal data of individuals in the EU (GDPR) or California residents (CCPA) depending on merchant and customer geography.

[POLICY]
- **GDPR:** If any customer or merchant is an EU data subject: publish a privacy notice identifying Depost (or its sponsor bank) as the data controller, document legal bases for processing (contract performance, legal obligation for BSA recordkeeping), implement data subject rights workflows (access, rectification, erasure with legal-hold carve-out, portability), and conduct a DPIA (Data Protection Impact Assessment) for the payment processing activity.
- **CCPA:** If any California resident is a customer or merchant: publish a "Notice at Collection" at point of data collection, maintain a privacy policy, implement opt-out rights for "sale/sharing" (Depost likely does not sell data, but confirm), and honor deletion requests subject to legal-hold carve-outs.
- GDPR/CCPA deletion requests cannot override BSA/AML 5-year retention obligations. Implement a legal-hold flag: deleted from user-visible systems but retained in a restricted-access archive until the retention window expires.

### 4.5 Data Retention and Deletion Schedule

[POLICY] Maintain a formal Data Retention Schedule. Minimum periods:

| Data Category | Minimum Retention | Legal Basis |
|---|---|---|
| Transaction records (all rails) | 5 years from transaction date | BSA § 1010.430 |
| Customer identity / CIP records | 5 years from account closure or last transaction | BSA § 1020.220 |
| SAR filings and supporting documentation | 5 years from filing date | BSA § 1010.430 |
| CTR filings | 5 years from filing date | BSA § 1010.313 |
| Audit logs | 5 years | BSA / internal policy |
| Bank account data (encrypted tokens) | Duration of relationship + 5 years | BSA / NACHA |
| Marketing / non-financial data | 3 years or until consent revoked | CCPA / GDPR |

[CODE] Implement a scheduled purge job that deletes non-legal-hold data after expiry and verifies that legal-hold records are not purged. This job must be idempotent and auditable.

### Data Protection Checklist

- [ ] [CODE] Encrypt all `customer_account_number` and `customer_routing_number` values at rest (AES-256-GCM with KMS-wrapped DEK, or preferably replace with Plaid processor tokens / MT external account IDs).
- [ ] [CODE] Drop plaintext account/routing columns after verified encrypted backfill.
- [ ] [CODE] Redact account numbers in all API responses (return last-4 only: `"****1234"`).
- [ ] [CODE] The Plaid `exchange_public_token` flow returns raw account numbers from `auth/get`; upgrade to processor tokens or immediately encrypt before storage.
- [ ] [CODE] Implement data retention enforcement job with legal-hold support.
- [ ] [POLICY] Author GLBA Safeguards Rule Information Security Program.
- [ ] [POLICY] Author Privacy Notice and CCPA Notice at Collection.
- [ ] [POLICY] Conduct GDPR DPIA for payment processing activity.
- [ ] [POLICY] Publish and maintain a formal Data Retention Schedule.
- [ ] [BUSINESS/LEGAL] Complete PCI DSS SAQ to formally document out-of-scope status.

---

## 5. Security & Audit Controls

### 5.1 Authentication and Access Control

[CODE] **Critical gap — currently zero authentication on the entire API.** `crates/paybank-api/src/router.rs` (lines 71–112) mounts all ~30 routes — including every `/api/admin/*` route that creates/updates merchants, reads all sessions, and views settlement data — with no authentication middleware whatsoever.

Required controls:
- **Admin API** (`/api/admin/*`): JWT-based authentication with role-based access. Admin tokens must be issued only to verified operator accounts, stored with bcrypt-hashed passwords, and scoped to specific roles (read-only analyst vs full-access operator). Tokens must expire (max 24-hour lifetime with refresh). Multi-factor authentication is required for admin accounts handling financial data (GLBA Safeguards Rule requirement).
- **Merchant API** (`/api/merchant-api/*`): HMAC-based API key authentication. API keys must be stored as bcrypt hashes — the current `merchants.api_key` column stores plaintext keys. Constant-time comparison only.
- **Money movement endpoints** (`/api/sessions/:id/confirm`, `/api/sessions/:id/initiate`, `/api/sessions/initiate`): These must require a session-scoped capability token (a `confirm_token` issued during session creation and bound to that session UUID), not just a merchant API key. This prevents IDOR — any caller today can confirm or initiate any session by UUID.
- **Admin console** (`/admin/*` SPA): Must have a login screen with session management before it communicates with any admin API route. The frontend gate alone provides no security (the backend must enforce auth); both must be implemented.

[CODE] The `merchant_api.rs` route group currently resolves merchants via a hardcoded `DEMO_MERCHANT_ID` constant. This must be replaced with extraction from the authenticated API key bearer header.

[CODE] Admin merchant endpoints currently serialize the full `Merchant` struct including `password_hash` and `api_key` to the response (`admin.rs` lines 97–103, 83–93). Create a `MerchantResponse` DTO that omits sensitive fields. (A `MerchantResponse` struct exists in `models.rs` but is not yet used by admin handlers.)

### 5.2 Immutable Audit Log

[CODE] **Critical gap — no audit log exists.** There is no record of which operator performed which admin action, which merchant created which session, or which user triggered which money movement. This is required by BSA recordkeeping, GLBA, and SOC 2.

Implement an append-only `audit_log` table:
```sql
CREATE TABLE audit_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    actor_type  TEXT NOT NULL,          -- 'admin', 'merchant', 'system', 'customer'
    actor_id    TEXT NOT NULL,          -- operator user id, merchant id, or 'system'
    action      TEXT NOT NULL,          -- 'session.created', 'session.initiated', 'merchant.kyc_updated', etc.
    resource_type TEXT NOT NULL,        -- 'payment_session', 'merchant', 'transaction'
    resource_id TEXT NOT NULL,          -- UUID of the affected resource
    metadata    JSONB,                  -- non-PII context (amounts, rails, status transitions)
    ip_address  TEXT,                   -- client IP (for admin actions)
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

Immutability requirements:
- The application DB user must have INSERT and SELECT only on `audit_log` — no UPDATE or DELETE.
- For stronger guarantees: append a hash-chain field (SHA-256 of the previous row's hash + this row's content) and verify the chain periodically.
- Retain for 5 years minimum.

Minimum events to log:
- All session state transitions (with actor, from-state, to-state, timestamp, provider reference).
- All money movement initiations and outcomes.
- All admin CRUD operations on merchants, banks, and sessions.
- All KYC/KYB status changes with the reviewing officer ID.
- All OFAC screening results.
- All authentication events (login success/failure, token issuance, token revocation).
- All webhook events received (MT and Column) with signature verification results.

### 5.3 Encryption

[CODE] Encryption in transit:
- All external traffic must be TLS 1.2+ (TLS 1.3 preferred). The current `src/main.rs` binds to `0.0.0.0:8888` in plaintext. TLS termination must occur at the load balancer or reverse proxy (not in the Rust binary) for production. Document this in the deployment runbook.
- The Modern Treasury and Column HTTP clients in `crates/paybank-payments/` use `reqwest` which defaults to TLS. Verify certificate validation is not disabled and pinning is considered for payment provider endpoints.

[CODE] Encryption at rest:
- PostgreSQL data-at-rest encryption via filesystem-level encryption (e.g., AWS EBS encryption, Azure Disk Encryption, or dm-crypt) is required. Document the encryption configuration in the infrastructure runbook.
- Application-level encryption for bank account data (see §4.2).
- Environment variable secrets (`MODERN_TREASURY_API_KEY`, `COLUMN_API_KEY`, `PLAID_SECRET`, `WEBHOOK_SECRET`, `DATABASE_URL`) must be managed via a secrets manager (AWS Secrets Manager, HashiCorp Vault, GCP Secret Manager) — not stored in `.env` files in the container or in `docker-compose.yml`. Confirmed gap: `docker-compose.yml` ships with live-format secrets committed to git. These must be rotated immediately and the git history scrubbed.

### 5.4 Webhook Security

[CODE] **Critical gap:** `moderntreasury_webhook` (webhooks.rs lines 89–137) performs **no signature verification**. The entire settlement path for the primary payment provider can be triggered by any HTTP POST to `/api/webhooks/moderntreasury` with attacker-controlled JSON. A POST of `{"event_type":"payment_order.settled","resource":{"id":"fake","status":"settled","remittance_information":"Session <real-uuid>"}}` marks any payment as completed with zero authentication.

Remediation:
- Modern Treasury webhooks are signed with an HMAC-SHA256 signature in a request header. Implement verification equivalent to what `column_webhook` does: extract the raw body bytes **before** JSON parsing, compute HMAC-SHA256 over the raw bytes using the MT webhook secret, and compare in constant time.
- The MT webhook secret must be a distinct environment variable from the Column webhook secret.
- Use `subtle::ConstantTimeEq` (or `hmac::Mac::verify_slice`) for the comparison — do not use `==` on the hex-encoded strings (timing side-channel).

[CODE] Column webhook verification (`column_webhook`, webhooks.rs lines 17–87) is implemented but uses string equality (`signature != expected`) rather than constant-time comparison. Replace with `subtle::ConstantTimeEq`.

### 5.5 SOC 2 Type II Readiness

[BUSINESS/LEGAL] Many enterprise merchants and sponsor banks will require a SOC 2 Type II report as a pre-condition for partnership. SOC 2 Type II covers a 6–12 month observation period and audits controls in five Trust Services Criteria: Security, Availability, Processing Integrity, Confidentiality, and Privacy.

[POLICY] Initiate SOC 2 readiness work in parallel with the code remediation program. Key prerequisites:
- Documented information security policies (align with GLBA program in §4.1).
- Formal access control and provisioning procedures.
- Incident response and business continuity plans (see §6).
- Change management procedures (covered partly by CI/CD — see `PRODUCTION_READINESS.md` Wave 0).
- Vendor risk management for Modern Treasury, Column, Plaid, and any cloud provider.

Engage a SOC 2 auditor (CPA firm) for a readiness assessment. The typical path is: readiness gap analysis → gap remediation (3–6 months) → Type I report (point-in-time) → 6–12 month observation → Type II report.

### Security & Audit Checklist

- [ ] [CODE] Implement JWT admin authentication middleware on all `/api/admin/*` routes.
- [ ] [CODE] Implement API key authentication middleware on all `/api/merchant-api/*` routes.
- [ ] [CODE] Implement session capability token (`confirm_token`) on `confirm` and `initiate` endpoints.
- [ ] [CODE] Store admin passwords as bcrypt hashes; store merchant API keys as bcrypt hashes; never return them in API responses.
- [ ] [CODE] Replace `DEMO_MERCHANT_ID` hardcode with authenticated merchant extraction.
- [ ] [CODE] Use `MerchantResponse` DTO in all admin handlers — omit `password_hash` and `api_key`.
- [ ] [CODE] Implement append-only `audit_log` table with INSERT-only DB grant.
- [ ] [CODE] Log all session state transitions, money movements, KYC changes, OFAC results, and admin actions.
- [ ] [CODE] Implement HMAC signature verification on the Modern Treasury webhook.
- [ ] [CODE] Replace string equality with constant-time comparison in Column webhook verification.
- [ ] [CODE] Rotate all secrets currently committed to git (`docker-compose.yml`) and scrub git history.
- [ ] [CODE] Integrate secrets manager for all provider credentials.
- [ ] [CODE] Enforce TLS at the load balancer / reverse proxy for all production traffic.
- [ ] [CODE] Ensure PostgreSQL disk encryption is configured and documented.
- [ ] [POLICY] Enable MFA for all admin console operator accounts.
- [ ] [BUSINESS/LEGAL] Initiate SOC 2 readiness engagement with a CPA firm.

---

## 6. Operational Controls

### 6.1 Payment Status Machine and Reconciliation

[CODE] **Critical gap:** `SessionStatus` in `crates/paybank-core/src/models.rs` (lines 100–109) has only five variants: `Pending`, `Authorized`, `Processing`, `Completed`, `Expired`. There are no `Failed`, `Returned`, `Reversed`, or `Blocked` states.

Consequence: in `initiate.rs` (lines 92–99) and `webhooks.rs` (lines 64–69), any failure — whether a network timeout (ambiguous), a definitive bank rejection, an NSF return, a NACHA return code R10 (Unauthorized), or a fraud reversal — is mapped to `Expired`. These are **legally and financially distinct events** with different regulatory, ledger, and customer notification obligations.

Required additions to `SessionStatus`:
```rust
pub enum SessionStatus {
    Pending,
    Authorized,
    Processing,
    Completed,
    Failed,        // Definitive provider rejection; no funds moved.
    Returned,      // Funds moved, returned by receiving bank (NACHA return code).
    Reversed,      // Funds moved, reversed by the originating bank or compliance.
    Blocked,       // Held for compliance/OFAC review.
    Expired,       // Timed out before initiation; no funds moved.
}
```

Add a PostgreSQL CHECK constraint on `payment_sessions.status` enforcing valid state transitions (a transition table, not just an enum).

### 6.2 NACHA Return Code Handling

[CODE] ACH returns are governed by NACHA return codes (R01–R85). Different return codes have different compliance and operational implications:

| Code | Meaning | Action Required |
|---|---|---|
| R01 | Insufficient Funds | Mark `Returned`; notify merchant; may retry once |
| R02 | Account Closed | Mark `Returned`; flag for KYC update; do not retry |
| R05 | Unauthorized Debit | Mark `Reversed`; mandatory SAR consideration; do not retry |
| R10 | Customer Advises Unauthorized | Mark `Reversed`; possible SAR; monitor return rate |
| R20 | Non-Transaction Account | Mark `Failed`; update bank metadata |
| R29 | Corporate Customer Advises Not Authorized | Mark `Reversed`; SAR consideration |

[CODE] The Column webhook handler (`column_webhook` in `webhooks.rs`) maps all `"failed" | "returned"` statuses to `SessionStatus::Expired`. This must be replaced with logic that:
1. Reads the NACHA return code from the provider webhook payload (`data.return_code` or equivalent).
2. Maps the return code to the appropriate `SessionStatus` (`Returned`, `Reversed`, or `Failed`).
3. Writes an appropriate compensating `journal_entry` if funds were previously credited.
4. Flags return codes associated with unauthorized transactions for AML review.

Track and monitor return rates against NACHA's thresholds: 0.5% for unauthorized debits (R05, R07, R10, R29), 3% for overall debits.

### 6.3 Reconciliation Against Providers

[CODE] Depost must reconcile its internal transaction records against Modern Treasury and Column at least daily. The current codebase has no reconciliation job.

Required reconciliation workflow:
1. At end of each business day, query Modern Treasury's payment orders API and Column's transfers API for all transactions in a time window.
2. Compare each provider record against `transactions` and `payment_sessions` in Depost's database.
3. Flag discrepancies: sessions in `Processing` with no provider record, sessions the provider reports as settled but Depost shows as `Processing`, amounts that do not match.
4. Alert the operations team for any unresolved discrepancy. Escalate to the BSA/AML Officer if the discrepancy involves a completed settlement with a missing Depost record (potential ledger integrity issue).

[CODE] The `payment_sessions.provider` column exists but is documented as sometimes not being persisted. This must be reliably set at session creation and preserved through all state transitions — it is the foreign key to the reconciliation lookup.

### 6.4 Dispute and Return Handling

[POLICY] Establish a written dispute handling procedure covering:
- Consumer disputes under Regulation E (Electronic Fund Transfer Act): consumers have the right to dispute unauthorized electronic transfers. Depost or its sponsor bank must acknowledge within 10 business days and resolve within 45 business days (or 90 for POS or new accounts).
- Merchant disputes: a clear SLA for merchant-reported discrepancies.
- NACHA return window: returns for most consumer debits can be received up to 60 days after settlement for unauthorized claims.

[CODE] The dispute handling workflow requires the `Returned` and `Reversed` session states described in §6.1. Without distinct states, there is no programmatic distinction between an NSF return (no further action, may retry) and a fraud reversal (SAR consideration, account flag, do not retry).

### 6.5 Incident Response

[POLICY] Author and test an Incident Response Plan covering at minimum:
- **Security incidents:** unauthorized access, data breach, credential compromise. GLBA's Safeguards Rule requires notification to the FTC within 30 days of discovering a security event affecting 500+ customers. Sponsor bank notification must occur on a contractually specified timeline (often 24–72 hours for payment-related incidents).
- **Operational incidents:** provider outage (MT or Column unavailable), mass payment failures, reconciliation breaks.
- **Compliance incidents:** OFAC match discovered post-settlement, SAR filing trigger.
- **Roles:** Incident Commander, Communications Lead, Technical Lead, Legal/Compliance contact.
- **Runbook trigger:** any session entering `Reversed` or `Blocked` state in bulk (>5 in an hour) triggers an automatic PagerDuty/alerting notification.

[CODE] Add Prometheus alerting rules (requires the observability workstream from `PRODUCTION_READINESS.md` Wave 1) for: bulk state transitions to `Reversed`/`Failed`, reconciliation break alerts, and OFAC screening service unavailability.

### 6.6 Business Continuity

[POLICY] Document a Business Continuity Plan (BCP) and Disaster Recovery (DR) plan:
- RTO (Recovery Time Objective): target < 4 hours for full payment processing capability.
- RPO (Recovery Point Objective): target < 15 minutes data loss maximum.
- PostgreSQL backup policy: automated daily snapshots + continuous WAL archiving (point-in-time recovery). Backup retention: 30 days minimum.
- Provider failover: Column is defined as the fallback provider in the README but the failover is not automated (it is an environment variable toggle). Implement automatic failover on MT 5xx errors with circuit-breaker logic.
- Geographic redundancy: document primary and secondary region. Ensure the secondary region has access to the same KMS keys (if using envelope encryption from §4.2).

### Operational Checklist

- [ ] [CODE] Add `Failed`, `Returned`, `Reversed`, `Blocked` variants to `SessionStatus`.
- [ ] [CODE] Add PostgreSQL CHECK constraint enforcing valid `SessionStatus` transitions.
- [ ] [CODE] Replace the `"failed" | "returned" => SessionStatus::Expired` mapping in both webhook handlers with NACHA return-code-aware status mapping.
- [ ] [CODE] Implement daily reconciliation job against Modern Treasury and Column APIs.
- [ ] [CODE] Ensure `payment_sessions.provider` is reliably set and preserved for all sessions.
- [ ] [CODE] Implement automatic provider failover (Column as actual fallback on MT 5xx).
- [ ] [POLICY] Author a NACHA return handling procedure with per-return-code actions.
- [ ] [POLICY] Author a Regulation E consumer dispute handling procedure.
- [ ] [POLICY] Author an Incident Response Plan with GLBA-compliant breach notification procedures.
- [ ] [POLICY] Author a Business Continuity / Disaster Recovery plan with documented RTO/RPO.
- [ ] [POLICY] Establish daily reconciliation review process with escalation path.

---

## 7. Prioritized Go-Live Checklist

This section provides the definitive gate structure for Depost's go-live. Items are separated into three tiers: **Immediate safety** (must be done before any production traffic, regardless of pilot scope), **Minimum Viable Pilot** (required before the first real-money transaction with a real user), and **Minimum GA** (required before general availability and public marketing).

### Tier 0 — Immediate Safety (Before Any Production Infrastructure)

These items represent active risk to any environment connected to production providers:

- [ ] [CODE] Rotate all credentials committed to `docker-compose.yml` (Modern Treasury API key, Column API key, Plaid secret, WEBHOOK_SECRET, database password). Scrub git history. **Do this today.**
- [ ] [CODE] Set `PAYMENT_INITIATION_ENABLED=false` environment variable and add a fail-closed guard at the top of both initiate handlers: `if !env::var("PAYMENT_INITIATION_ENABLED").is_ok_or(false) { return Err(AppError::PaymentInitiationDisabled); }`. This flag is the legal gate — it does not enable until legal/regulatory sign-off (§1).
- [ ] [CODE] Remove or gate the seed `002_seed_data.sql` from running in non-development environments. It ships verified merchants with guessable API keys.

### Tier 1 — Minimum Viable Pilot (Required Before First Real-Money Transaction)

These are the minimum controls for a limited pilot with known, manually-vetted merchants and customers:

**Regulatory:**
- [ ] [BUSINESS/LEGAL] Signed sponsor bank / Program Manager Agreement.
- [ ] [BUSINESS/LEGAL] Legal opinion confirming regulatory model is lawful.
- [ ] [BUSINESS/LEGAL] Named BSA/AML Compliance Officer on file.
- [ ] [POLICY] Board-approved written BSA/AML program.

**Authentication:**
- [ ] [CODE] Admin API JWT authentication middleware on all `/api/admin/*` routes.
- [ ] [CODE] Merchant API key authentication middleware on all `/api/merchant-api/*` routes.
- [ ] [CODE] Session capability token (`confirm_token`) gating `confirm` and `initiate` endpoints.

**Webhook security:**
- [ ] [CODE] HMAC signature verification on Modern Treasury webhook (the current forgery hole).
- [ ] [CODE] Constant-time comparison in Column webhook verification.

**Data protection:**
- [ ] [CODE] Account numbers encrypted at rest (tokenization or AES-256-GCM) — NACHA mandatory.
- [ ] [CODE] Account numbers redacted from all API responses.
- [ ] [CODE] Provider credentials loaded from a secrets manager, not environment files.

**KYC/AML:**
- [ ] [CODE] `kyc_status = 'verified'` gate in session creation and initiate handlers.
- [ ] [CODE] OFAC screening integrated into `confirm` handler with fail-closed behavior.

**Audit:**
- [ ] [CODE] Append-only `audit_log` table logging all session transitions and money movements.

**Operational:**
- [ ] [CODE] `Failed`, `Returned`, `Reversed` status variants replacing the `Expired` catch-all.
- [ ] [CODE] NACHA return-code-aware webhook status mapping.

**Flag:**
- [ ] [CODE+BUSINESS/LEGAL] Set `PAYMENT_INITIATION_ENABLED=true` only after all Tier 1 items above are complete **and** legal sign-off is documented in writing.

### Tier 2 — Minimum GA (Required Before General Availability)

These items are required before opening Depost to arbitrary merchants or customers:

**Regulatory:**
- [ ] [BUSINESS/LEGAL] FinCEN MSB registration (if required under the sponsor bank structure — confirm with counsel).
- [ ] [BUSINESS/LEGAL] NACHA Third-Party Sender agreement with ODFI.

**KYC/KYB:**
- [ ] [CODE] Full merchant KYB onboarding flow with beneficial ownership collection.
- [ ] [CODE] Consumer identity verification integration (Socure/Alloy/Persona) in `confirm` flow.
- [ ] [POLICY] Written CIP/CDD program approved by sponsor bank.

**AML:**
- [ ] [CODE] Transaction monitoring worker (velocity rules, structuring detection).
- [ ] [CODE] OFAC re-screen at `initiate` handler.
- [ ] [CODE] `compliance_reviews` table for SAR/CTR tracking.

**Security:**
- [ ] [CODE] MFA enforced for all admin operator accounts.
- [ ] [CODE] Rate limiting on all payment initiation and authentication endpoints.
- [ ] [CODE] Complete audit log coverage (all events in §5.2).

**Ledger and reconciliation:**
- [ ] [CODE] Double-entry ledger postings wired into settlement path.
- [ ] [CODE] Real `get_balance` aggregation.
- [ ] [CODE] Daily automated reconciliation against MT and Column.

**Operational:**
- [ ] [CODE] Automatic provider failover (Column as real fallback).
- [ ] [POLICY] Regulation E dispute handling procedure.
- [ ] [POLICY] Incident Response Plan tested via tabletop exercise.
- [ ] [POLICY] Business Continuity Plan with documented RTO/RPO.

**Privacy:**
- [ ] [POLICY] GLBA Safeguards Rule Information Security Program published.
- [ ] [POLICY] Privacy Notice and CCPA Notice at Collection published.
- [ ] [CODE] Data retention enforcement job with legal-hold support.

**SOC 2:**
- [ ] [BUSINESS/LEGAL] SOC 2 Type I report (or audit engagement initiated).

---

## The Fail-Closed Flag — Explicit Statement

**The `POST /api/sessions/initiate` and `POST /api/sessions/:id/initiate` endpoints must remain behind a fail-closed feature flag until legal and regulatory sign-off is complete.**

Specifically: the Rust code for these handlers must check a `PAYMENT_INITIATION_ENABLED` environment variable (defaulting to `false`) and return an error to the caller if the flag is not explicitly set to `true`. This is not an optional best-practice — it is the programmatic enforcement of the legal gate described in §1. No amount of code completeness in §§2–6 substitutes for the sponsor bank agreement and legal opinion in §1. The flag must only be set in a production environment after:

1. A signed sponsor bank / Program Manager Agreement is in the Depost legal file.
2. Written legal counsel opinion confirming the go-live regulatory model is lawful.
3. A named BSA/AML Compliance Officer is on file with the sponsor bank.
4. The board-approved BSA/AML written program is finalized.
5. All Tier 1 code items above are merged, tested, and deployed.

Setting the flag without these five conditions exposes every officer and director of Depost to potential criminal liability for unlicensed money transmission. This is not a technical risk — it is a legal risk that code cannot mitigate.

---

_This document should be reviewed and updated quarterly, and immediately following any material change to: the payment rails offered, the sponsor bank relationship, the customer/merchant onboarding flow, the provider integrations, or any regulatory guidance issued by FinCEN, NACHA, the Federal Reserve (FedNow), or The Clearing House (RTP)._

_Cross-reference: `docs/PRODUCTION_READINESS.md` tracks the engineering implementation program for all `[CODE]` items in this document. The two documents are maintained in sync — any new code gap identified in the engineering audit that has regulatory implications must be reflected here._
