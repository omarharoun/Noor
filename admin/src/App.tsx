import { useEffect, useState } from 'react';
import { Icon } from './components/Icon';
import { Avatar } from './components/primitives';
import { api, type Merchant } from './lib/api';
import { AuthProvider, useAuth, AUTH_EXPIRED_EVENT } from './lib/auth';
import { Login } from './pages/Login';
import { Dashboard } from './pages/Dashboard';
import { Sessions } from './pages/Sessions';
import { Merchants } from './pages/Merchants';
import { Settlements } from './pages/Settlements';
import { Payouts } from './pages/Payouts';
import { Invoices } from './pages/Invoices';
import { Deposits } from './pages/Deposits';
import { Webhooks } from './pages/Webhooks';
import { Health } from './pages/Health';
import { Operators } from './pages/Operators';

export type PageId =
  | 'dashboard'
  | 'sessions'
  | 'merchants'
  | 'settlements'
  | 'payouts'
  | 'invoices'
  | 'deposits'
  | 'webhooks'
  | 'health'
  | 'operators';

// Shared lookup so sessions/dashboard can show merchant names (the sessions
// endpoint returns merchant_id only).
export interface Ctx {
  go: (p: PageId) => void;
  merchantName: (id: string) => string;
  merchants: Merchant[];
}

const TITLES: Record<PageId, [string, string]> = {
  dashboard: ['Dashboard', 'Live overview of payment operations'],
  sessions: ['Sessions', 'Every payment session and its lifecycle'],
  merchants: ['Merchants', 'Accounts, onboarding and KYC status'],
  settlements: ['Settlements', 'Merchant balances and payouts'],
  payouts: ['Payouts', 'Outbound payments across merchants'],
  invoices: ['Invoices', 'Billing activity across merchants'],
  deposits: ['Deposits', 'Wallet top-ups across merchants'],
  webhooks: ['Webhooks', 'Event delivery log and retries'],
  health: ['Health', 'Issues and anomalies to act on'],
  operators: ['Operators', 'Console users and their roles'],
};

// ---- Inner console (rendered only when authenticated) ------------------------

function Console() {
  const { operator, logout } = useAuth();
  const [page, setPage] = useState<PageId>('dashboard');
  const [merchants, setMerchants] = useState<Merchant[]>([]);
  const [sessionCount, setSessionCount] = useState<number | undefined>(undefined);
  const [failedWebhooks, setFailedWebhooks] = useState<number | undefined>(undefined);

  useEffect(() => {
    api.merchants({ limit: 200 }).then((r) => setMerchants(r.merchants)).catch(() => {});
    api.sessions({ limit: 1 }).then((r) => setSessionCount(r.total)).catch(() => {});
    api
      .webhooks({ limit: 200 })
      .then((r) => setFailedWebhooks(r.events.filter((e) => e.status === 'failed').length || undefined))
      .catch(() => {});
  }, []);

  const ctx: Ctx = {
    go: setPage,
    merchants,
    merchantName: (id) => merchants.find((m) => m.id === id)?.name ?? id.slice(0, 8) + '…',
  };

  const nav = [
    { id: 'dashboard' as const, label: 'Dashboard', icon: 'layout-dashboard' },
    { id: 'sessions' as const, label: 'Sessions', icon: 'arrow-left-right', count: sessionCount },
    { id: 'merchants' as const, label: 'Merchants', icon: 'store', count: merchants.length || undefined },
    { id: 'settlements' as const, label: 'Settlements', icon: 'wallet' },
    { id: 'payouts' as const, label: 'Payouts', icon: 'send' },
    { id: 'invoices' as const, label: 'Invoices', icon: 'file-text' },
    { id: 'deposits' as const, label: 'Deposits', icon: 'download' },
    { id: 'webhooks' as const, label: 'Webhooks', icon: 'webhook', count: failedWebhooks },
    { id: 'health' as const, label: 'Health', icon: 'activity' },
    { id: 'operators' as const, label: 'Operators', icon: 'users' },
  ];

  const [title, sub] = TITLES[page];

  const operatorName = operator?.name ?? '';
  const operatorRole = operator?.role ?? '';

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-glyph">
            <svg viewBox="0 0 30 30" style={{ position: 'absolute', inset: 0 }}>
              <path d="M20 8.5a8.2 8.2 0 1 0 0 13 6.4 6.4 0 1 1 0-13Z" fill="var(--cream-deep)" />
            </svg>
          </div>
          <span className="brand-word">Noor</span>
        </div>
        <div className="nav-section">Operations</div>
        {nav.slice(0, 3).map((n) => (
          <NavItem key={n.id} n={n} active={page === n.id} onClick={() => setPage(n.id)} />
        ))}
        <div className="nav-section">Money</div>
        {nav.slice(3, 7).map((n) => (
          <NavItem key={n.id} n={n} active={page === n.id} onClick={() => setPage(n.id)} />
        ))}
        <div className="nav-section">System</div>
        {nav.slice(7).map((n) => (
          <NavItem key={n.id} n={n} active={page === n.id} onClick={() => setPage(n.id)} />
        ))}
        <div className="sidebar-foot">
          <Avatar name={operatorName || '?'} size={32} />
          <div style={{ flex: 1, minWidth: 0 }}>
            <div className="who">{operatorName || '—'}</div>
            <div className="role" style={{ textTransform: 'capitalize' }}>{operatorRole || '—'}</div>
          </div>
          <button
            className="icon-btn"
            onClick={logout}
            title="Sign out"
            style={{ flex: 'none', width: 28, height: 28, border: 'none', background: 'transparent', boxShadow: 'none' }}
          >
            <Icon name="log-out" size={15} />
          </button>
        </div>
      </aside>

      <div className="main">
        <header className="topbar">
          <h1>{title}</h1>
          <div className="topbar-spacer" />
          <div className="searchbox">
            <Icon name="search" size={15} />
            Search sessions…
          </div>
          <button className="icon-btn">
            <Icon name="bell" size={17} />
            <span className="dotnote" />
          </button>
          <button className="icon-btn" onClick={logout} title="Sign out">
            <Icon name="settings" size={17} />
          </button>
        </header>
        <div className="content">
          <p className="page-intro">{sub}</p>
          {page === 'dashboard' && <Dashboard ctx={ctx} />}
          {page === 'sessions' && <Sessions ctx={ctx} />}
          {page === 'merchants' && <Merchants />}
          {page === 'settlements' && <Settlements />}
          {page === 'payouts' && <Payouts />}
          {page === 'invoices' && <Invoices />}
          {page === 'deposits' && <Deposits />}
          {page === 'webhooks' && <Webhooks />}
          {page === 'health' && <Health />}
          {page === 'operators' && <Operators />}
        </div>
      </div>
    </div>
  );
}

// ---- Auth gate ---------------------------------------------------------------

function AuthGate() {
  const { operator, logout } = useAuth();

  // Listen for 401 events dispatched by api.ts
  useEffect(() => {
    const handler = () => logout();
    window.addEventListener(AUTH_EXPIRED_EVENT, handler);
    return () => window.removeEventListener(AUTH_EXPIRED_EVENT, handler);
  }, [logout]);

  // null = still booting (validating stored token)
  if (operator === null) {
    return (
      <div style={{ minHeight: '100vh', display: 'grid', placeItems: 'center', background: 'var(--cream)' }}>
        <span className="spinner" style={{ width: 32, height: 32, borderWidth: 3 }} />
      </div>
    );
  }

  // undefined = no valid session
  if (operator === undefined) {
    return <Login />;
  }

  // Authenticated
  return <Console />;
}

// ---- Root export -------------------------------------------------------------

export default function App() {
  return (
    <AuthProvider>
      <AuthGate />
    </AuthProvider>
  );
}

// ---- NavItem (unchanged) -----------------------------------------------------

function NavItem({
  n,
  active,
  onClick,
}: {
  n: { label: string; icon: string; count?: number };
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button className={`nav-item ${active ? 'active' : ''}`} onClick={onClick}>
      <Icon name={n.icon} size={16} />
      {n.label}
      {n.count !== undefined && <span className="nav-count">{n.count}</span>}
    </button>
  );
}
