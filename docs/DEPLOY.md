# Deploy topology (Supabase + Cloudflare)

Noor is a long-lived Rust (`axum` + `tokio`) server with a Postgres database and
two background workers (webhook delivery, reconciliation). Recommended setup:

```
Cloudflare Pages  ──>  static admin console (admin/dist) + checkout (web/)
Cloudflare DNS/CDN/WAF ──> in front of the API
Container host (Fly.io / Railway / Render / ECS) ──> the Rust API binary
Supabase ──> managed Postgres (the ledger / source of truth)
```

Supabase is a real Postgres server, so the entire codebase runs unchanged
(sqlx, compile-time-checked queries, JSONB, gen_random_uuid(), FOR UPDATE SKIP
LOCKED, the CAS/idempotency locking, double-entry ledger, migrations 001–003).

## Database (Supabase)
- **Use the direct connection or the Session-mode pooler (port 5432).**
  Do **NOT** use the transaction-mode pooler (port 6543): it does not support
  prepared statements, which `sqlx` uses for every query — the app will break.
- `DATABASE_URL` example (session pooler):
  `postgresql://postgres.<project-ref>:<password>@aws-0-<region>.pooler.supabase.com:5432/postgres?sslmode=require`
- Migrations apply automatically on boot via `sqlx::migrate!` (001 schema, 002
  seed, 003 money-safety constraints). For multi-instance deploys, prefer running
  migrations as a one-off pre-deploy step instead of on every boot.
- `gen_random_uuid()` and `JSONB` are built into Supabase Postgres — no extra
  extensions required.
- Enable automated backups / PITR in the Supabase dashboard.

## API host
- Build: `docker build` (multi-stage Dockerfile) or `cargo build --release`.
- Inject all secrets from the platform's secret store (see `docs/SECRETS.md`) —
  `DATABASE_URL`, `JWT_SECRET`, `PII_ENCRYPTION_KEY`, `WEBHOOK_SECRET`,
  `MODERN_TREASURY_*`, `COLUMN_*`, `PLAID_*`, `ADMIN_EMAIL`/`ADMIN_PASSWORD`.
- Set `PUBLIC_APP_URL` to the public origin (used for pay links / QR).
- Health: `/health` (liveness), `/health/ready` (DB-backed readiness).
- Run a single instance first; the background workers (webhook delivery,
  reconciliation) use `FOR UPDATE SKIP LOCKED` so multiple instances are safe.

## Frontend (Cloudflare Pages)
- Admin console: `cd admin && npm ci && npm run build` → deploy `admin/dist`
  (Vite base is `/admin/`). Currently the Rust server also serves it at `/admin`;
  on Pages, serve it at the same path and proxy `/api/*` to the API host.
- Checkout pages (`web/pay.html`, `web/invoice.html`) are served by the API via
  `/pay/:id` and `/invoice/:id` (they need server-side session-id injection), so
  keep those routes on the API host, not Pages.

## Compliance / KYC
- Modern Treasury handles KYC + sanctions/compliance. The local compliance gate
  should defer to MT's verification status (sync `kyc_status` from MT) rather
  than the built-in stub deny-list.

## Production hardening
- **Fail-closed PII**: set `NOOR_ENV=production`. The API then refuses to start
  unless `PII_ENCRYPTION_KEY` is set and valid — no silent plaintext PII.
- **Least-privilege DB role**: run the API as the DML-only `noor_app` role
  (`docs/sql/least_privilege_role.sql`), set `NOOR_SKIP_MIGRATE=true`, and run
  migrations separately as the owner role. A leaked API DB credential then can't
  DROP/ALTER tables.
- **Audit log**: every money-moving and admin action is recorded in `audit_log`
  (visible under Health → Audit log). Keep it append-only; don't grant DELETE on
  it to anything but admin.

## KMS / envelope-encryption upgrade path
Today PII uses AES-256-GCM with a single static `PII_ENCRYPTION_KEY` from the
environment — strong, but the key sits in env. The production-grade upgrade:
1. Keep the **master key (KEK) in a KMS/HSM** (AWS KMS, GCP KMS, Vault Transit).
2. Generate a **unique data key (DEK) per record**, encrypt the field with the
   DEK, and store the KMS-wrapped DEK alongside the ciphertext (envelope).
3. Rotate the KEK in the KMS without re-encrypting data (only DEKs re-wrap).
The `paybank_core::crypto` module is the single integration point — swap the
key source behind it. This needs a chosen KMS + credentials; the rest is in place.

## Optional: Supabase Auth for operators
- Supabase Auth (GoTrue) can replace the single bootstrap `ADMIN_EMAIL`/
  `ADMIN_PASSWORD` with real per-operator accounts, hashed credentials, and
  roles — validate the Supabase JWT in `auth::admin_auth` instead of the
  bootstrap check. This closes the "real operator user store" follow-up.
```
