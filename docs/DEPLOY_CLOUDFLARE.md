# Deploy Depost on Cloudflare Containers

One Worker (`worker/index.ts`) fronts one Docker container (the Rust backend),
defined in `wrangler.jsonc`. The container serves **all** subdomains — the Rust
app routes by `Host` — so `/api/*` answers on every host and each UI calls its
own origin (no CORS). A cron trigger keeps the container warm so the in-process
`reconcile` / `recurring` / idempotency-reaper loops keep running.

## Prerequisites

- A Cloudflare account on the **Workers Paid** plan with **Containers** enabled.
- **Docker** running locally (Wrangler builds the image and pushes it).
- `npm install` at the repo root (installs `wrangler` + `@cloudflare/containers`).
- The `norhadi.com` zone on this Cloudflare account.

## 1. Authenticate

```bash
npx wrangler login          # or set CLOUDFLARE_API_TOKEN with Workers + Containers edit scope
```

## 2. Set secrets (never commit these)

Non-secret config lives in `wrangler.jsonc` → `vars`. Everything sensitive is a
Wrangler secret:

```bash
npx wrangler secret put DATABASE_URL                 # Neon connection string
npx wrangler secret put JWT_SECRET                   # >= 32 bytes
npx wrangler secret put PII_ENCRYPTION_KEY           # AES-256 key
npx wrangler secret put MODERN_TREASURY_ORG_ID
npx wrangler secret put MODERN_TREASURY_API_KEY
npx wrangler secret put MODERN_TREASURY_WEBHOOK_SECRET
npx wrangler secret put MT_INTERNAL_ACCOUNT_ID
npx wrangler secret put SENDGRID_API_KEY
npx wrangler secret put ADMIN_EMAIL
npx wrangler secret put ADMIN_PASSWORD
# Column (only if still used):
npx wrangler secret put COLUMN_API_KEY
npx wrangler secret put COLUMN_MERCHANT_ACCOUNT_ID
npx wrangler secret put WEBHOOK_SECRET
```

The Worker forwards every secret listed in `worker/index.ts` (`BACKEND_ENV`)
into the container as environment variables, plus `PORT=8080`.

## 3. DNS cutover (do this in the dashboard before/at deploy)

`routes` in `wrangler.jsonc` use `custom_domain: true`, so Cloudflare manages
DNS + TLS for each name. **Delete any pre-existing record on those names first**
— in particular the old Railway `api.norhadi.com` CNAME — or the custom-domain
attach will conflict.

Names attached: `norhadi.com`, `www.norhadi.com`, `app.norhadi.com`,
`platform.norhadi.com`, `api.norhadi.com`.

## 4. Deploy

```bash
npm run deploy        # = wrangler deploy (builds the image, uploads, attaches routes)
```

First build is slow (full Rust release compile). Subsequent deploys reuse cache.

## 5. Verify

```bash
curl -s https://api.norhadi.com/health           # 200
curl -s https://api.norhadi.com/                 # "Depost API" info page
curl -s https://app.norhadi.com/      | grep -o "Available balance"   # merchant dashboard
curl -s https://platform.norhadi.com/ | grep -o "Depost Operations"    # operator console
curl -s https://norhadi.com/          | grep -o "Money movement"      # marketing
npx wrangler tail                                  # live logs
```

## Notes / tradeoffs

- **Always-warm instance.** `max_instances: 1` + the 5-min cron keep one
  instance alive so the background loops never pause. This bills like a small
  always-on VM. The cleaner long-term move is to convert `reconcile` /
  `recurring` / reaper into Worker **cron endpoints** and let the container
  scale to zero — a follow-up, not required to ship.
- **Single instance / region.** One shared Durable Object id (`noor-main`)
  means one container handles all traffic and holds a single copy of the
  background workers (no duplicate ticks). Revisit if you need multi-region.
- **Migrations** still run at boot via `sqlx::migrate!`. To run them separately
  under a least-privilege role, set `NOOR_SKIP_MIGRATE=true` (a `var`) and
  migrate as the owner role in a deploy step.
- Railway config (`Dockerfile` aside) is left in the repo as a fallback; it is
  no longer the deploy target.
