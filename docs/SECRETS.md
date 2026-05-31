# Secrets management

Noor reads **all** secrets from environment variables at startup (see
`crates/paybank-api/src/state.rs::Config` and `std::env` reads in the payment
providers). Nothing secret is committed to the repo.

## Rule
- **Never** put a real secret in `docker-compose.yml`, the Dockerfile, the repo,
  or any tracked file. `docker-compose.yml` only references `${VARS}` and reads a
  local, git-ignored `.env` for development.
- `.env` is git-ignored and **dev-only**. Copy `.env.example` → `.env` locally.
- In every shared/staging/production environment, inject these vars from a
  **secret manager** — never a file on disk.

## Required secrets
| Var | Purpose |
|---|---|
| `DATABASE_URL` | Postgres connection string (managed DB in prod) |
| `JWT_SECRET` | signs admin/operator session JWTs (≥32 bytes) |
| `ADMIN_EMAIL` / `ADMIN_PASSWORD` | bootstrap operator login (replace with a real user store) |
| `PII_ENCRYPTION_KEY` | 32-byte (hex or base64) AES-256-GCM key for account/routing at rest |
| `WEBHOOK_SECRET` | inbound webhook HMAC verify + outbound signing fallback |
| `MODERN_TREASURY_API_KEY`, `MODERN_TREASURY_ORG_ID`, `MT_INTERNAL_ACCOUNT_ID`, `MODERN_TREASURY_WEBHOOK_SECRET` | Modern Treasury |
| `COLUMN_API_KEY`, `COLUMN_MERCHANT_ACCOUNT_ID` | Column |
| `PLAID_CLIENT_ID`, `PLAID_SECRET`, `PLAID_ENV` | Plaid |

## Deploy pattern (manager-agnostic)
The app needs only environment variables, so any of these work without code
changes — the platform fetches from the manager and injects env at launch:
- **AWS** — Secrets Manager / SSM Parameter Store → ECS task `secrets:` or EKS
  External Secrets Operator.
- **GCP** — Secret Manager → Cloud Run `--set-secrets` or GKE External Secrets.
- **Kubernetes** — External Secrets Operator or Vault Agent Injector → env.
- **HashiCorp Vault** — `vault agent` template / `envconsul` → env.

If you want a specific provider wired in-process (e.g. fetch-at-boot via the AWS
SDK or a Vault client) instead of env injection, name the platform and it can be
added behind `Config::from_env`.

## Rotation
Any secret that was ever committed to git history is **compromised** and must be
**rotated at the provider**, not merely removed:
- ✅ `PLAID_SECRET` — rotated.
- ⬜ `MODERN_TREASURY_API_KEY`, `MT_INTERNAL_ACCOUNT_ID`, `MODERN_TREASURY_ORG_ID`
- ⬜ `COLUMN_API_KEY`, `COLUMN_MERCHANT_ACCOUNT_ID`
- ⬜ `WEBHOOK_SECRET`
- ⬜ `PLAID_CLIENT_ID` (not strictly secret, but was exposed)

History was scrubbed of these values, but rotation is the actual remediation.
