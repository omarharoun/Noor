# Key & Secret Management

How Noor should hold its secrets in production. Today every secret is read from
an environment variable (see `.env.example`). That is fine for local dev and
acceptable for a single hardened host, but for real money you want a **KMS** (Key
Management Service) so that (1) no human ever sees the raw key, (2) keys can be
rotated without a redeploy, and (3) access is audited.

---

## 1. The secrets Noor holds

| Secret | Used for | Blast radius if leaked |
|---|---|---|
| `PII_ENCRYPTION_KEY` | AES-256-GCM of bank account/routing numbers at rest (`crates/paybank-core/src/crypto.rs`) | **Critical** — decrypts all stored customer PII |
| `DATABASE_URL` (password) | Neon Postgres login | **Critical** — full DB read/write |
| `JWT_SECRET` | Signs operator console JWTs | High — forge operator sessions |
| `WEBHOOK_SECRET` / `MODERN_TREASURY_WEBHOOK_SECRET` | HMAC-verify inbound provider webhooks; sign outbound merchant webhooks | High — forge settlement/return events → move money |
| `MODERN_TREASURY_API_KEY` (+ `COLUMN_API_KEY`, `PLAID_SECRET`) | Provider API auth | **Critical** — initiate real transfers |
| `ADMIN_PASSWORD` (bootstrap only) | First operator login | Medium — one-time seed, real accounts are Argon2 in `operators` |

The two **Critical / decrypt-everything** keys are `PII_ENCRYPTION_KEY` and the
provider API keys. Protect those hardest.

---

## 2. Two levels of "use a KMS"

### Level 1 — Secrets manager (do this first, low effort)

Store the raw secrets in a managed secret store and inject them at process start.
No code change to Noor — it still reads env vars; the platform populates them
from the vault instead of a `.env` file on disk.

- **Docker / Compose / Swarm secrets** (implemented — see below): each secret is
  mounted as a file under `/run/secrets/<name>`; Noor reads `FOO` from the file
  named by `FOO_FILE`. Use `docker-compose.secrets.yml`.
- **AWS**: Secrets Manager (or SSM Parameter Store, SecureString). Grant the
  task role `secretsmanager:GetSecretValue` on exactly these ARNs.
- **GCP**: Secret Manager, mounted as env or fetched at boot.
- **Fly.io / Railway / Render / container host**: their built-in encrypted
  secrets store (`fly secrets set`, etc.).
- **HashiCorp Vault**: `kv` engine, app authenticates with AppRole.
- **Kubernetes**: mount a Secret as a volume and point the `*_FILE` vars at it —
  same code path as Docker secrets.

#### File-backed secrets (`*_FILE`) — implemented

`crates/paybank-api/src/secrets.rs` runs first in `main` and applies the
convention the official Postgres/MySQL images use: for any env var `FOO`, if
`FOO_FILE` is set and points at a readable file, that file's contents become
`FOO`. The raw secret stays out of the process environment, so it can't leak via
`docker inspect`, `/proc/<pid>/environ`, or an env crash dump. A `*_FILE` that
points at a missing or empty file is **fatal at boot** (fail fast, never fall
back to a stale/unset secret). Wire-up is in `docker-compose.secrets.yml`
(secret material goes in the gitignored `secrets/` dir); for Swarm/prod prefer
`docker secret create` over `file:` sources.

What this buys you: secrets are encrypted at rest in the vault, access is
IAM-scoped and audit-logged, and rotation is a vault update + process restart —
no secret ever lives in git, the image, or a shell history. **This already closes
the "secrets in env on disk" gap.**

### Level 2 — Envelope encryption for PII (highest assurance)

A secrets manager still hands your process the raw 32-byte PII key. Envelope
encryption means the master key (KEK) **never leaves the KMS** — the KMS decrypts
a wrapped data key (DEK) for you on demand, and the KEK itself is unexportable
hardware-backed.

Flow:

```
KMS (holds KEK, unexportable)
  └─ Decrypt(wrapped_DEK) ──> plaintext DEK (32 bytes, in memory only)
                                 └─ AES-256-GCM encrypt/decrypt PII  (crypto.rs)
```

- Generate a DEK once: `aws kms generate-data-key --key-id <KEK> --key-spec AES_256`.
  Store the returned **ciphertext** blob (the wrapped DEK) in config/DB — it is
  useless without the KMS.
- At boot, Noor calls `kms:Decrypt(wrapped_DEK)` to get the plaintext DEK and
  loads it into the existing `OnceLock` cipher. The KEK never touches the host.
- Rotate the KEK in the KMS at will; re-wrap the DEK. Rotating the DEK itself is
  the `v2` path in §4.

This is the recommended end state for the PII key specifically. The provider API
keys can't be envelope-encrypted (the provider needs the literal key), so they
stay at Level 1.

---

## 3. Wiring it into Noor (minimal code change)

The crypto layer is already isolated and **versioned** — `crypto.rs` tags
ciphertext with `enc:v1:` and reads the key through a single `cipher()` accessor.
That makes the KMS swap a localized change:

**Level 1 (implemented):** the platform injects secrets either as env vars
(cloud secret managers) or as files via the `*_FILE` convention
(`secrets.rs`, for Docker/Swarm/k8s). Either way, no per-secret code change.

**Level 2 (envelope):** add a boot step that resolves the DEK from KMS and sets
`PII_ENCRYPTION_KEY` in-process *before the first crypto call* (the `OnceLock`
initializes lazily on first use):

```rust
// in src/main.rs startup, before the server binds:
if let Ok(wrapped) = std::env::var("PII_DEK_WRAPPED") {
    let dek = kms_decrypt(&wrapped).await?;          // returns 32 base64/hex bytes
    std::env::set_var("PII_ENCRYPTION_KEY", dek);    // crypto.rs picks it up
}
```

No change to `encrypt`/`decrypt` or any call site. `crypto::is_configured()` is
already checked at startup so production fails closed if the key didn't resolve.

---

## 4. Rotation

**Provider/JWT/webhook secrets:** rotate at the provider/secret-manager, restart
the process. Stateless — nothing stored depends on the old value (JWTs just
expire; in-flight ones die, which is acceptable).

**PII key rotation** is the careful one, because old ciphertext was written with
the old key. The `enc:v1:` prefix exists exactly for this:

1. Introduce `enc:v2:` keyed by the new DEK. `decrypt` selects the cipher by
   prefix (v1 → old key, v2 → new key); `encrypt` always writes v2.
2. New writes are v2 immediately. Old rows stay readable under v1.
3. Run a background re-encrypt pass: read each v1 row, `decrypt` (v1), `encrypt`
   (v2), write back. When zero v1 rows remain, retire the old key.

This is online and zero-downtime. Keep at least the previous key version
available until the re-encrypt pass completes.

---

## 5. Concrete recommendation for the current stack

Given Neon (DB) + a container host (API) + Cloudflare (static/DNS):

1. **Now (Level 1 — code already in place):** move all `.env` secrets into your
   host's encrypted secrets store (or AWS/GCP Secret Manager on a cloud VM), or
   use Docker/Swarm secrets via `docker-compose.secrets.yml` + the `*_FILE`
   loader. Delete `.env` from any deployed host. Scope read access to the API's
   role only. This is now just a config/ops task and removes the biggest
   remaining "secrets at rest in plaintext" exposure.
2. **Before scale (Level 2):** put `PII_ENCRYPTION_KEY` behind AWS KMS (or GCP
   KMS / Cloud HSM) envelope encryption using the boot hook in §3. ~30 lines.
3. **Operationally:** enable the KMS/secret-manager audit log, set an IAM policy
   that denies `kms:Decrypt`/`GetSecretValue` to everything except the API role,
   and add a quarterly rotation calendar entry (PII key via the v2 pass; provider
   keys via provider rotation).

Until Level 2 ships, the PII key must still be a strong 32-byte value
(`openssl rand -base64 32`) sourced from the secrets manager — never committed,
never logged.
