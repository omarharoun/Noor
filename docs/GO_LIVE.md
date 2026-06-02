# Noor — Go-Live Runbook (Modern Treasury)

Everything in Noor is built and verified against **MT Sandbox**. Going live is a
**configuration switch, not a code change**: point the app at a live MT
organization backed by a real bank account, register the live webhook, and flip
a few Railway variables.

---

## 0. The one prerequisite that isn't a config flag: a bank behind MT

Modern Treasury is the orchestration layer — it does **not** hold funds, provide
a charter, or grant rail access by itself. A **real bank account must be
connected to your MT live organization**. Which arrangement you need depends on
*whose money moves*:

| Model | What you need | Sponsor bank / license? |
|---|---|---|
| **Your own money** — Noor collects *your* revenue and pays *your* vendors | Connect your business bank account to MT live | **No** — "all MT" is fine |
| **Merchants' money** — merchants hold balances on Noor and you pay them out | A bank partner with FBO/for-benefit-of accounts; likely money-transmitter licensing | **Yes** — required regardless of MT |

Pick the model before going live. The app code is identical either way; the
legal/banking setup differs. (Noor's multi-merchant features exist, but you
control whether you operate it as your-own-money or a funds-holding platform.)

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

In Railway → the **Noor** service → Variables:

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
- [ ] If holding merchant funds: bank-partner/FBO + compliance program in place (§0).

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
