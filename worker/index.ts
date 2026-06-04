/**
 * Cloudflare Worker entry — fronts the Noor backend container.
 *
 * One container instance serves every subdomain: the Rust app routes by the
 * `Host` header (app./platform./api./apex), so we forward each request as-is to
 * a single shared instance. A cron trigger (see wrangler.jsonc) keeps that
 * instance warm so the in-process reconcile/recurring/reaper loops keep running.
 */
import { Container, getContainer } from '@cloudflare/containers';

// Every env name the Rust backend reads. Secrets go in via
// `wrangler secret put <NAME>`; non-secret config via [vars] in wrangler.jsonc.
// Both land on the Worker `env`, and we forward them into the container.
const BACKEND_ENV = [
  'DATABASE_URL',
  'DATABASE_MAX_CONNECTIONS',
  'JWT_SECRET',
  'PII_ENCRYPTION_KEY',
  'MODERN_TREASURY_ORG_ID',
  'MODERN_TREASURY_API_KEY',
  'MODERN_TREASURY_WEBHOOK_SECRET',
  'MT_INTERNAL_ACCOUNT_ID',
  'COLUMN_API_KEY',
  'COLUMN_MERCHANT_ACCOUNT_ID',
  'WEBHOOK_SECRET',
  'SENDGRID_API_KEY',
  'EMAIL_FROM',
  'EMAIL_TO',
  'PUBLIC_APP_URL',
  'CORS_ALLOWED_ORIGINS',
  'ADMIN_EMAIL',
  'ADMIN_PASSWORD',
  'NOOR_ENV',
  'NOOR_SKIP_MIGRATE',
  'NOOR_CRON_DRIVEN',
  'CRON_SECRET',
  'RUST_LOG',
] as const;

interface Env {
  BACKEND: DurableObjectNamespace;
  CRON_SECRET?: string;
  [key: string]: unknown;
}

export class Backend extends Container {
  // The Rust app listens on $PORT; we pin it to 8080 (see Dockerfile + envVars).
  defaultPort = 8080;
  // Scale to zero: sleep ~1m after the last request. The cron tick (every 2m)
  // wakes it to run background jobs, so it's idle most of a low-traffic period.
  sleepAfter = '1m';

  constructor(ctx: DurableObjectState, env: Env) {
    super(ctx, env);
    const vars: Record<string, string> = { PORT: '8080' };
    for (const k of BACKEND_ENV) {
      const v = env[k];
      if (typeof v === 'string' && v.length > 0) vars[k] = v;
    }
    this.envVars = vars;
  }
}

// A single shared instance handles all hosts (and keeps one copy of the
// background workers). Same id => same Durable Object => same container.
const INSTANCE = 'noor-main';

export default {
  async fetch(request: Request, env: Env): Promise<Response> {
    return getContainer(env.BACKEND, INSTANCE).fetch(request);
  },

  // Cron-driven background jobs: run one pass of webhooks/reconcile/recurring.
  // Secret-gated; the container does the work then is free to sleep again.
  async scheduled(_event: ScheduledController, env: Env): Promise<void> {
    await getContainer(env.BACKEND, INSTANCE).fetch(
      new Request('http://container/internal/cron', {
        method: 'POST',
        headers: { 'x-cron-key': env.CRON_SECRET ?? '' },
      }),
    );
  },
};
