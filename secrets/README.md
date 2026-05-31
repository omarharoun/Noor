# Local secret files (file-backed secrets)

Used by `docker-compose.secrets.yml`. Each file holds **one secret, raw, no
trailing newline beyond what's stripped**. The app reads `FOO` from the file
named by `FOO_FILE` (see `crates/paybank-api/src/secrets.rs`).

Everything in this directory except `.gitignore` and this README is gitignored —
real secret values must never be committed.

Create them like:

```sh
printf '%s' "$(openssl rand -base64 32)"   > pii_key         # 32-byte PII key
printf '%s' "postgresql://...@host/db?..." > database_url     # Neon DIRECT url
printf '%s' "$(openssl rand -hex 32)"      > jwt_secret
printf '%s' "<modern-treasury-api-key>"    > mt_api_key
printf '%s' "<webhook-signing-secret>"     > webhook_secret
printf '%s' "<column-api-key>"             > column_api_key
printf '%s' "<plaid-secret>"               > plaid_secret
chmod 600 *                                 # owner-only
```

Then: `docker compose -f docker-compose.yml -f docker-compose.secrets.yml up`

For Docker Swarm / production, prefer `docker secret create` over `file:` sources
so the values never touch the host filesystem in plaintext.
