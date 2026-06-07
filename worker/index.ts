/**
 * Cloudflare Worker entry — fronts the Depost backend container.
 *
 * One container instance serves every subdomain: the Rust app routes by the
 * `Host` header (app./platform./api./apex), so we forward each request as-is to
 * a single shared instance. A cron trigger (see wrangler.jsonc) drives the
 * background jobs so the container can scale to zero.
 *
 * Outbound email: the Rust container can't hold a Worker binding, so it POSTs
 * the internal `/__cf/email` path here (auth'd with CRON_SECRET) and the Worker
 * sends via the Cloudflare Email binding `env.EMAIL.send()`.
 */
import { Container, getContainer } from '@cloudflare/containers';

// Env names forwarded into the container. (Email is sent by the Worker now, so
// SENDGRID_API_KEY / EMAIL_FROM are no longer forwarded; the container only
// needs EMAIL_ENDPOINT + CRON_SECRET to reach the internal email endpoint.)
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
  'EMAIL_ENDPOINT',
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

interface EmailBinding {
  send(msg: {
    to: string;
    from: string;
    subject: string;
    text?: string;
    html?: string;
  }): Promise<{ messageId?: string }>;
}

interface Env {
  BACKEND: DurableObjectNamespace;
  EMAIL: EmailBinding;
  EMAIL_FROM?: string;
  CRON_SECRET?: string;
  MODERN_TREASURY_WEBHOOK_SECRET?: string;
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

// Internal path the container POSTs to send email. Secret-gated; handled here,
// never forwarded to the container.
const EMAIL_PATH = '/__cf/email';

function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return diff === 0;
}

async function handleEmail(request: Request, env: Env): Promise<Response> {
  if (request.method !== 'POST') return new Response('method not allowed', { status: 405 });
  const secret = env.CRON_SECRET ?? '';
  const provided = request.headers.get('x-cron-key') ?? '';
  if (!secret || !timingSafeEqual(provided, secret)) {
    return new Response('forbidden', { status: 403 });
  }
  let body: { to?: string; subject?: string; text?: string; html?: string };
  try {
    body = await request.json();
  } catch {
    return new Response('bad json', { status: 400 });
  }
  if (!body.to || !body.subject) return new Response('missing to/subject', { status: 400 });
  try {
    const res = await env.EMAIL.send({
      to: body.to,
      from: env.EMAIL_FROM ?? 'noreply@depost.io',
      subject: body.subject,
      text: body.text ?? '',
      ...(body.html ? { html: body.html } : {}),
    });
    return Response.json({ ok: true, messageId: res?.messageId ?? null });
  } catch (e) {
    return new Response(`email send failed: ${(e as Error)?.message ?? e}`, { status: 502 });
  }
}

// Inbound Modern Treasury webhooks are handled HERE (always-on Worker) instead
// of the container, so MT always gets a fast 200 even when the container is
// asleep — that's what keeps the endpoint from being auto-disabled. We verify
// the HMAC signature at the edge, ack immediately, and process in the container
// asynchronously (it cold-starts if needed; MT isn't kept waiting).
const MT_WEBHOOK_PATH = '/api/webhooks/moderntreasury';

async function verifyHmacHex(secret: string, body: ArrayBuffer, sigHex: string): Promise<boolean> {
  const key = await crypto.subtle.importKey(
    'raw',
    new TextEncoder().encode(secret),
    { name: 'HMAC', hash: 'SHA-256' },
    false,
    ['sign'],
  );
  const mac = await crypto.subtle.sign('HMAC', key, body);
  const expected = [...new Uint8Array(mac)].map((b) => b.toString(16).padStart(2, '0')).join('');
  if (expected.length !== sigHex.length) return false;
  let diff = 0;
  for (let i = 0; i < expected.length; i++) diff |= expected.charCodeAt(i) ^ sigHex.charCodeAt(i);
  return diff === 0;
}

async function handleMtWebhook(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
  const sig = request.headers.get('x-signature') ?? '';
  const secret = env.MODERN_TREASURY_WEBHOOK_SECRET ?? '';
  const body = await request.arrayBuffer();
  if (!secret || !sig || !(await verifyHmacHex(secret, body, sig))) {
    return new Response('invalid signature', { status: 401 });
  }
  // Verified — hand off to the container to post ledger entries, then ack MT now.
  ctx.waitUntil(
    getContainer(env.BACKEND, INSTANCE)
      .fetch(
        new Request('http://container/api/webhooks/moderntreasury', {
          method: 'POST',
          headers: {
            'content-type': request.headers.get('content-type') ?? 'application/json',
            'x-signature': sig,
          },
          body,
        }),
      )
      .catch(() => {}),
  );
  return Response.json({ received: true });
}

export default {
  async fetch(request: Request, env: Env, ctx: ExecutionContext): Promise<Response> {
    const { pathname } = new URL(request.url);
    if (pathname === EMAIL_PATH) return handleEmail(request, env);
    if (pathname === MT_WEBHOOK_PATH && request.method === 'POST') {
      return handleMtWebhook(request, env, ctx);
    }
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
