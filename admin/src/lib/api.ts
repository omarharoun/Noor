// Thin client for Noor's admin API. All shapes mirror the Rust handlers in
// crates/paybank-api/src/routes/admin.rs and the models in paybank-core.

import { STORAGE_KEY, dispatchAuthExpired } from './auth';

export type SessionStatus =
  | 'pending'
  | 'authorized'
  | 'processing'
  | 'completed'
  | 'expired';

export type Rail = 'fednow' | 'rtp' | 'ach' | 'wire';

export interface PaymentSession {
  id: string;
  merchant_id: string;
  bank_id: string;
  amount_cents: number;
  currency: string;
  note: string | null;
  status: SessionStatus;
  rail_used: Rail | null;
  column_ref: string | null;
  column_counterparty_id: string | null;
  customer_name: string | null;
  customer_email: string | null;
  customer_phone: string | null;
  customer_address_line_1: string | null;
  customer_address_city: string | null;
  customer_address_state: string | null;
  customer_address_postal_code: string | null;
  customer_address_country_code: string | null;
  customer_account_number: string | null;
  customer_routing_number: string | null;
  customer_account_type: string | null;
  expires_at: string;
  created_at: string;
  updated_at: string;
}

export interface Merchant {
  id: string;
  name: string;
  email: string;
  // api_key is intentionally absent — the backend redacts secrets from admin responses
  webhook_url: string | null;
  business_name: string | null;
  business_type: string | null;
  registration_number: string | null;
  tax_id: string | null;
  industry_category: string | null;
  website_url: string | null;
  status: string;
  kyc_status: string;
  risk_level: string | null;
  onboarding_completed_at: string | null;
  created_at: string;
  updated_at: string;
}

export interface Bank {
  id: string;
  name: string;
  routing_number: string | null;
  supports_fednow: boolean;
  supports_rtp: boolean;
  supports_wire: boolean;
  logo_url: string | null;
  display_order: number;
}

export interface Settlement {
  merchant_id: string;
  merchant_name: string | null;
  total_count: number;
  total_volume_cents: number;
  last_payment_date: string | null;
}

export interface WebhookEvent {
  id: string;
  merchant_id: string;
  session_id: string | null;
  event_type: string;
  payload: unknown;
  status: string;
  attempts: number;
  next_retry_at: string | null;
  sent_at: string | null;
  created_at: string;
}

export interface RailStat {
  rail_used: string | null;
  count: number;
  volume: number;
}

export interface DailyStat {
  date: string;
  count: number;
  volume: number;
}

export interface DashboardStats {
  today_total_transactions: number;
  today_completed: number;
  pending_count: number;
  total_volume_cents: number;
  by_rail: RailStat[];
  last_7_days: DailyStat[];
}

export interface HealthSummary {
  stuck_processing: number;
  failed_sessions: number;
  returned_sessions: number;
  reversed_sessions: number;
  webhooks_failed: number;
  webhooks_exhausted: number;
  unbalanced_journal_entries: number;
}

export interface Operator {
  id: string;
  email: string;
  name: string;
  role: string;
  is_active: boolean;
  created_at: string;
}

export interface AuditEntry {
  id: string;
  actor: string;
  actor_role: string | null;
  action: string;
  target_type: string | null;
  target_id: string | null;
  metadata: unknown;
  created_at: string;
}

const BASE = '/api/admin';

function authHeaders(): Record<string, string> {
  const token = localStorage.getItem(STORAGE_KEY);
  return token ? { Authorization: `Bearer ${token}` } : {};
}

function handle401(res: Response) {
  if (res.status === 401) {
    localStorage.removeItem(STORAGE_KEY);
    dispatchAuthExpired();
  }
}

async function get<T>(path: string, params?: Record<string, string | number | undefined>): Promise<T> {
  const qs = new URLSearchParams();
  if (params) {
    for (const [k, v] of Object.entries(params)) {
      if (v !== undefined && v !== '') qs.set(k, String(v));
    }
  }
  const url = `${BASE}${path}${qs.toString() ? `?${qs}` : ''}`;
  const res = await fetch(url, { headers: authHeaders() });
  if (!res.ok) {
    handle401(res);
    throw new Error((await res.text()) || `Request failed (${res.status})`);
  }
  return res.json() as Promise<T>;
}

async function post<T>(path: string, body?: unknown): Promise<T> {
  const res = await fetch(`${BASE}${path}`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: body ? JSON.stringify(body) : undefined,
  });
  if (!res.ok) {
    handle401(res);
    throw new Error((await res.text()) || `Request failed (${res.status})`);
  }
  return res.json() as Promise<T>;
}

export const api = {
  stats: () => get<DashboardStats>('/stats'),
  sessions: (params?: { limit?: number; offset?: number; status?: string; rail?: string }) =>
    get<{ sessions: PaymentSession[]; total: number }>('/sessions', params),
  merchants: (params?: { limit?: number; offset?: number }) =>
    get<{ merchants: Merchant[]; total: number }>('/merchants', params),
  createMerchant: (body: { name: string; email: string }) =>
    post<Merchant>('/merchants', body),
  banks: () => get<{ banks: Bank[] }>('/banks'),
  settlements: () => get<{ settlements: Settlement[] }>('/settlements'),
  webhooks: (params?: { limit?: number; offset?: number }) =>
    get<{ events: WebhookEvent[]; total: number }>('/webhooks', params),
  retryWebhook: (id: string) => post<{ requeued: boolean }>(`/webhooks/${id}/retry`),
  health: () => get<HealthSummary>('/health'),
  audit: () => get<{ entries: AuditEntry[] }>('/audit'),
  operators: () => get<{ operators: Operator[] }>('/operators'),
  createOperator: (body: { email: string; name: string; password: string; role?: string }) =>
    post<Operator>('/operators', body),
  updateOperator: async (id: string, body: { is_active?: boolean; role?: string }): Promise<Operator> => {
    const res = await fetch(`${BASE}/operators/${id}`, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json', ...authHeaders() },
      body: JSON.stringify(body),
    });
    if (!res.ok) {
      handle401(res);
      throw new Error((await res.text()) || 'Update failed');
    }
    return res.json() as Promise<Operator>;
  },
};

export interface CreateSessionResult {
  id: string;
  amount_cents: number;
  currency: string;
  expires_at: string;
  qr_data: string | null;
}

// Session create + transfer initiation are operator actions (admin JWT required).
export async function createSession(body: {
  merchant_id: string;
  bank_id: string;
  amount_cents: number;
  currency: string;
  note?: string;
}): Promise<CreateSessionResult> {
  const res = await fetch('/api/sessions', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    handle401(res);
    throw new Error((await res.text()) || 'Create failed');
  }
  return res.json();
}

export async function initiateTransfer(id: string): Promise<{ status: string; transaction_id?: string }> {
  const res = await fetch(`/api/sessions/${id}/initiate`, { method: 'POST', headers: authHeaders() });
  if (!res.ok) {
    handle401(res);
    throw new Error((await res.text()) || 'Initiate failed');
  }
  return res.json();
}

export async function fetchQrSvg(id: string): Promise<string> {
  const res = await fetch(`/api/sessions/${id}/qr`);
  if (!res.ok) throw new Error('QR unavailable');
  return res.text();
}
