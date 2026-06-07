# Depost — Go-Live Runbook (Modern Treasury)

Everything in Depost is built and verified against **MT Sandbox**. Going live is a
**configuration switch, not a code change**: point the app at a live MT
organization backed by a real bank account, register the live webhook, and flip
a few Railway variables.

---

## 0. The one prerequisite that isn't a config flag: MT full-stack onboarding

Going "all MT" is a real single-provider path. Modern Treasury's **Full-Stack
PSP** product covers what a payments platform needs end-to-end:

- **Bank relationships** — MT provides/manages the partner bank + accounts, so
  you do **not** separately procure a sponsor bank.
- **Compliance** — KYC/KYB, sanctions/OFAC screening, transaction monitoring
  (this can power Depost's `compliance::gate`, replacing the stub).
- **Payment orchestration** — the rails (ACH/wire/RTP/FedNow) Depost already calls.
- **Ledgering** — MT offers it; Depost keeps its own Postgres double-entry ledger
  as source of truth and reconciles against MT webhooks (no need to adopt MT
  Ledgers).

**What's still on you (not a config flag):** complete MT's **onboarding /
underwriting** — your business KYB, your use case (incl. whether you hold
*merchants'* funds and pay them out, which MT's compliance + partner-bank setup
is designed to support), and expected volumes. MT approves the program and
provisions the live organization + internal (bank) account. This is an
application/review process with MT, not a flag you toggle — start it early.

Once MT has approved and provisioned your live org + account, everything below
is the config switch. The app code is identical to sandbox.

---

## 1. Set up the MT **live** organization

1. In Modern Treasury, switch from Sandbox to your **live** organization.
2. **Connect the bank account** (your business account, or a bank partner) and
   wait for it to be approved/active. Note its **internal account ID**.
3. Enable the **rails** you need on that account (ACH, wire, RTP, FedNow). RTP
   *Request-for-Payment* is a per-bank capability — enable it if you want that
   (it's why RfP isn't in the sandbox build).
4. Create a **live API key** (starts with `live-`, not `test-`) and note the
   **organization ID**.

## 2. Register the live webhook

1. MT dashboard → Developers → Webhooks → add endpoint:
   `https://api.norhadi.com/api/webhooks/moderntreasury`
2. Copy the **signing secret** MT generates → that's `MODERN_TREASURY_WEBHOOK_SECRET`.

## 3. Flip the Railway variables (sandbox → live)

In Railway → the **Depost** service → Variables:

| Variable | Set to |
|---|---|
| `MODERN_TREASURY_API_KEY` | your **live** key (`live-…`) |
| `MODERN_TREASURY_ORG_ID` | your **live** organization ID |
| `MT_INTERNAL_ACCOUNT_ID` | the **live** internal account (the connected bank account) |
| `MODERN_TREASURY_WEBHOOK_SECRET` | the secret from step 2 |
| `PUBLIC_APP_URL` | `https://api.norhadi.com` *(fixes QR codes & pay links — confirm it isn't still `pay.norhadi.com`)* |
| `NOOR_ENV` | `production` (already) |

Railway redeploys on a variable change — no code change, no new build needed
beyond that.

## 4. Pre-go-live checklist

- [ ] `PII_ENCRYPTION_KEY` and `JWT_SECRET` are set to strong values (already).
- [ ] **Remove `ADMIN_PASSWORD`** after the first operator login (it's a one-time seed).
- [ ] **Rotate** the Railway + Cloudflare API tokens that were shared in chat.
- [ ] Move secrets to a manager / Docker secrets (see `docs/KEY_MANAGEMENT.md`).
- [ ] Each merchant has a **verified** bank account (prenote cleared) before payouts.
- [ ] Confirm the live MT webhook **signature format** matches what we verify
      (`x-signature`, hex HMAC-SHA256 of the raw body) — see §6.
- [ ] MT full-stack onboarding approved (bank + compliance provisioned by MT, §0).
- [ ] Wire MT's compliance/KYB result into `compliance::gate` (replace the stub)
      so the live authorize chokepoint reflects MT screening.

## 5. Verify after the switch

```sh
curl -fsS https://api.norhadi.com/health         # OK
curl -fsS https://api.norhadi.com/health/ready    # 200 (DB reachable)
```
Then, in the dashboard, run one of each against the **live** org with **small
real amounts**:
- Add a bank account → status moves to `verified` after the prenote clears (1–3 days).
- Create an invoice → open the hosted pay page → pay it → webhook marks it paid.
- Send a small payout → approve it → confirm it reaches the payee.

## 6. Live webhook signature check (item #7 from the audit)

On the first **live** MT webhook delivery, confirm it verifies. Our verifier
(`crates/paybank-api/src/routes/webhooks.rs`) expects an `x-signature` header
that is the **hex HMAC-SHA256 of the raw request body** keyed by
`MODERN_TREASURY_WEBHOOK_SECRET`. If MT's live format differs (e.g. a timestamped
scheme), adjust `verify_hmac` accordingly. A returned/failed event then drives
the ledger reversal automatically.

## 7. Rollback

To return to safe testing: set the four MT variables back to the **sandbox**
values (org `…`, `test-…` key, sandbox internal account) and redeploy. No data
migration needed; sandbox and live are separate MT organizations.

---

### What stays the same going live
Everything in the app: invoicing, payment links, direct-entry checkout, payouts
with dual-control approval, multi-user accounts, reports/CSV/recurring,
white-labeling. They all already run against MT; the only difference is the MT
org behind the four variables above.
