# Deploying Noor — Railway (API) + Cloudflare (DNS/proxy) + Neon (DB)

Target topology under **`norhadi.com`**:

```
                 ┌─ app.norhadi.com  → Cloudflare Pages (admin SPA)
Cloudflare DNS ──┼─ pay.norhadi.com  → Cloudflare Pages (web: checkout/dashboard/invoice)
   (norhadi.com) └─ api.norhadi.com  → Railway service (Rust API)   [proxied / orange cloud]
                                              │
                                              └──TLS──> Neon Postgres (existing, migrated)
```

This doc covers the **API on Railway**. Frontends → see Cloudflare Pages section at the end. DB stays on **Neon**.

---

## 1. Build / runtime config (already in repo)

- **`Dockerfile`** — multi-stage; builds the Rust API (release, sqlx offline) and ships a slim non-root runtime. Railway builds it directly.
- **`railway.json`** — config-as-code: Dockerfile builder, `/health` healthcheck, restart-on-failure, **`numReplicas: 1`**.
- The API now honors Railway's injected **`PORT`** automatically (binds `0.0.0.0:$PORT`); leave `BIND_ADDR` unset on Railway.

> **Why `numReplicas: 1`:** the API runs two in-process background loops (`reconcile`, `webhook_worker`) and an in-memory login throttle. Until those are extracted to a leader/cron, run exactly **one** instance. Horizontal scaling is a follow-up (extract pollers → Railway cron or a leader lock).

---

## 2. Environment variables (set in Railway → service → Variables)

Railway stores variables **encrypted at rest** and injects them at runtime, so set the plain variable names here (the `*_FILE` Docker-secrets convention is not needed on Railway).

### Required
| Var | Value | Notes |
|---|---|---|
| `DATABASE_URL` | Neon **DIRECT** url (host **without** `-pooler`) | `...require&channel_binding=require` |
| `JWT_SECRET` | 32+ byte random | `openssl rand -hex 32` |
| `PII_ENCRYPTION_KEY` | 32-byte key (hex or base64) | `openssl rand -base64 32` — **required in prod** |
| `NOOR_ENV` | `production` | Fail-closed: refuses to boot if PII key missing/invalid |

### Recommended
| Var | Value |
|---|---|
| `PUBLIC_APP_URL` | `https://pay.norhadi.com` |
| `CORS_ALLOWED_ORIGINS` | `https://app.norhadi.com,https://pay.norhadi.com` |
| `DATABASE_MAX_CONNECTIONS` | `5` (modest; respects Neon connection limits) |

### Providers (set what you use)
`MODERN_TREASURY_API_KEY`, `MODERN_TREASURY_ORG_ID`, `MT_INTERNAL_ACCOUNT_ID`,
`WEBHOOK_SECRET` (and/or `MODERN_TREASURY_WEBHOOK_SECRET`),
`COLUMN_API_KEY`, `COLUMN_MERCHANT_ACCOUNT_ID` (fallback rail),
`PLAID_CLIENT_ID`, `PLAID_SECRET`, `PLAID_ENV`.

### Bootstrap operator (one-time)
Set `ADMIN_EMAIL` + `ADMIN_PASSWORD` for the first login on a fresh DB, then
**remove `ADMIN_PASSWORD`** after you've created real operators. Do not leave it set.

### Do NOT set
`PORT` (Railway injects it), `BIND_ADDR` (let `PORT` win).

---

## 3. Deploy

### Option A — GitHub auto-deploy (recommended)
1. Railway dashboard → **New Project → Deploy from GitHub repo** → select `omarharoun/Noor`, branch `main`.
2. Railway reads `railway.json` for build/deploy settings.
3. Add the variables from §2.
4. Every push to `main` redeploys (CI is green, so `main` is deployable).

### Option B — CLI from local
```sh
npm i -g @railway/cli            # or: npx @railway/cli
export RAILWAY_TOKEN=...          # already stored in .env (gitignored)
railway link                      # link to the project (first time)
railway up                        # build + deploy the current dir
railway variables --set NOOR_ENV=production   # repeat per var, or paste in dashboard
```

---

## 4. Custom domain `api.norhadi.com`

1. Railway → service → **Settings → Networking → Custom Domain** → add `api.norhadi.com`. Railway returns a CNAME target like `xxxx.up.railway.app`.
2. Once `norhadi.com` is **Active** in Cloudflare, add a DNS record:
   - **Type** CNAME, **Name** `api`, **Target** `xxxx.up.railway.app`.
   - **Start DNS-only (grey cloud)** so Railway can issue its TLS cert.
3. After Railway shows the cert issued, **flip the record to Proxied (orange cloud)** and set Cloudflare **SSL/TLS → Full (strict)**. This puts Cloudflare's CDN/WAF/DDoS in front of the money endpoints.
4. (Hardening) Lock the Railway origin to Cloudflare IPs / use a Cloudflare Tunnel so attackers can't bypass the proxy.

---

## 5. Verify

```sh
curl -fsS https://api.norhadi.com/health         # -> OK              (liveness)
curl -fsS https://api.norhadi.com/health/ready    # -> 200 if DB reachable (readiness)
```

If `/health/ready` is non-200, the app can't reach Neon — check `DATABASE_URL` (direct endpoint) and Neon connection limits.

---

## 6. Frontends → Cloudflare Pages

Two static projects, each `wrangler pages deploy` or GitHub-connected in the Pages UI:

| Subdomain | Source | Build |
|---|---|---|
| `app.norhadi.com` | `admin/` (Vite + React) | build cmd `npm run build`, output `admin/dist` |
| `pay.norhadi.com` | `web/` (static HTML/CSS/JS) | no build; output dir `web/` |

Point each Pages project's custom domain at the subdomain (Cloudflare wires the DNS automatically for Pages). Set the admin SPA's API base URL to `https://api.norhadi.com`.

---

## Checklist before go-live
- [ ] `norhadi.com` Active in Cloudflare; token covers the zone
- [ ] Railway vars set (§2); `PII_ENCRYPTION_KEY` + `NOOR_ENV=production`
- [ ] `DATABASE_URL` = Neon **direct** endpoint
- [ ] `api.norhadi.com` cert issued, then proxied + Full (strict)
- [ ] `ADMIN_PASSWORD` removed after first login
- [ ] `/health` and `/health/ready` green
- [ ] Frontends deployed to Pages, API base URL pointed at `api.norhadi.com`
