import { useEffect, useState } from 'react';
import { api, initiateTransfer, fetchQrSvg, type PaymentSession } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtMoney, short, ago, fmtDateTime } from '../lib/format';
import { Card, Button, StatusBadge, RailTag, Avatar, Spinner, Empty } from '../components/primitives';
import { Icon } from '../components/Icon';
import { NewSessionModal } from '../components/NewSessionModal';
import type { Ctx } from '../App';

const FILTERS = ['all', 'completed', 'processing', 'pending', 'authorized', 'expired'] as const;

export function Sessions({ ctx }: { ctx: Ctx }) {
  const [filter, setFilter] = useState<(typeof FILTERS)[number]>('all');
  const [sel, setSel] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);

  const { data, loading, error, reload } = useAsync(
    () => api.sessions({ limit: 100, status: filter === 'all' ? undefined : filter }),
    [filter]
  );

  const sessions = data?.sessions ?? [];
  const current = sessions.find((s) => s.id === sel) ?? null;

  return (
    <div>
      <div className="filterbar">
        {FILTERS.map((f) => (
          <button key={f} className={`chipfilter ${filter === f ? 'on' : ''}`} onClick={() => setFilter(f)}>
            {f === 'all' ? 'All sessions' : f.charAt(0).toUpperCase() + f.slice(1)}
          </button>
        ))}
        <div className="filter-spacer" />
        <Button variant="primary" size="sm" icon="plus" onClick={() => setCreating(true)}>
          New session
        </Button>
      </div>

      {creating && (
        <NewSessionModal merchants={ctx.merchants} onClose={() => setCreating(false)} onCreated={reload} />
      )}

      <Card title="Sessions ledger" live pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Session</th>
                <th>Merchant</th>
                <th>Amount</th>
                <th>Status</th>
                <th>Rail</th>
                <th>Created</th>
                <th>Settlement</th>
              </tr>
            </thead>
            <tbody>
              {sessions.map((s) => (
                <tr key={s.id} className={sel === s.id ? 'sel' : ''} onClick={() => setSel(s.id)}>
                  <td className="cell-id">{short(s.id)}</td>
                  <td>
                    <span className="cell-strong">{ctx.merchantName(s.merchant_id)}</span>
                  </td>
                  <td className="cell-amt">{fmtMoney(s.amount_cents)}</td>
                  <td>
                    <StatusBadge status={s.status} />
                  </td>
                  <td>
                    <RailTag rail={s.rail_used} />
                  </td>
                  <td className="cell-muted">{ago(s.created_at)}</td>
                  <td>
                    {s.status === 'completed' ? (
                      <span className="nbadge tone-success">Settled</span>
                    ) : (
                      <span className="nbadge tone-neutral">Open</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {loading && <div className="center-load"><Spinner /></div>}
          {error && <div className="flash error" style={{ margin: 16 }}>{error}</div>}
          {!loading && !error && sessions.length === 0 && (
            <Empty>No sessions found. Create one to start accepting payments.</Empty>
          )}
        </div>
      </Card>

      <div className="section-gap" />
      {current && (
        <SessionDetail
          s={current}
          merchantName={ctx.merchantName(current.merchant_id)}
          onChanged={reload}
        />
      )}
    </div>
  );
}

function SessionDetail({
  s,
  merchantName,
  onChanged,
}: {
  s: PaymentSession;
  merchantName: string;
  onChanged: () => void;
}) {
  const [qr, setQr] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [flash, setFlash] = useState<{ kind: 'success' | 'error'; msg: string } | null>(null);

  useEffect(() => {
    setQr(null);
    fetchQrSvg(s.id).then(setQr).catch(() => setQr(null));
  }, [s.id]);

  const hasBank = !!s.customer_account_number;
  const [cpId, extAcct] = (s.column_counterparty_id ?? '').split('|');
  const payUrl = `${window.location.origin}/pay/${s.id}`;

  const steps: { label: string; time: string; state: 'done' | 'now' | 'wait' }[] = [
    { label: 'Session created', time: ago(s.created_at), state: 'done' },
    { label: 'Bank linked', time: hasBank ? 'verified' : 'awaiting', state: hasBank ? 'done' : 'wait' },
    {
      label: 'Confirmed · counterparty',
      time: ['completed', 'processing', 'authorized'].includes(s.status) ? 'created' : 'awaiting',
      state: ['completed', 'processing', 'authorized'].includes(s.status) ? 'done' : 'wait',
    },
    {
      label: 'Transfer initiated',
      time: s.status === 'completed' ? 'sent' : s.status === 'processing' ? 'in flight' : 'pending',
      state: s.status === 'completed' ? 'done' : s.status === 'processing' ? 'now' : 'wait',
    },
    { label: 'Settled', time: s.status === 'completed' ? 'done' : 'pending', state: s.status === 'completed' ? 'done' : 'wait' },
  ];

  const initiate = async () => {
    setBusy(true);
    setFlash(null);
    try {
      const r = await initiateTransfer(s.id);
      setFlash({ kind: 'success', msg: `Transfer initiated · MT payment order created. ${r.transaction_id ? 'TXN ' + r.transaction_id.slice(0, 8) : ''}` });
      onChanged();
    } catch (e) {
      setFlash({ kind: 'error', msg: e instanceof Error ? e.message : String(e) });
    } finally {
      setBusy(false);
    }
  };

  return (
    <Card
      title={`Session ${short(s.id)}`}
      action={
        <div style={{ display: 'flex', gap: 8 }}>
          {s.status === 'authorized' && (
            <Button variant="success" size="sm" icon="zap" pulse onClick={initiate} disabled={busy}>
              {busy ? 'Initiating…' : 'Initiate transfer'}
            </Button>
          )}
          {(s.status === 'completed' || s.status === 'expired') && (
            <Button variant="secondary" size="sm" icon="receipt" onClick={() => window.open(`/invoice/${s.id}`, '_blank')}>
              Invoice
            </Button>
          )}
          <Button variant="secondary" size="sm" icon="external-link" onClick={() => window.open(payUrl, '_blank')}>
            Pay page
          </Button>
        </div>
      }
    >
      {flash && (
        <div className={`flash ${flash.kind}`}>
          <Icon name={flash.kind === 'success' ? 'check-circle-2' : 'circle'} size={15} />
          {flash.msg}
        </div>
      )}
      <div className="detail">
        <div>
          <dl className="dgrid">
            <dt>Status</dt>
            <dd>
              <StatusBadge status={s.status} />
            </dd>
            <dt>Amount</dt>
            <dd>
              <span style={{ fontFamily: 'var(--font-serif)', fontSize: 22, fontWeight: 500 }}>{fmtMoney(s.amount_cents)}</span>{' '}
              <span className="cell-muted">{s.currency}</span>
            </dd>
            <dt>Rail</dt>
            <dd>
              <RailTag rail={s.rail_used} />
            </dd>
            <dt>Merchant</dt>
            <dd>
              <span className="cell-name">
                <Avatar name={merchantName} size={22} />
                {merchantName}
              </span>
            </dd>
            {s.customer_name && (
              <>
                <dt>Customer</dt>
                <dd>
                  {s.customer_name}
                  {s.customer_email ? ' · ' + s.customer_email : ''}
                </dd>
              </>
            )}
            <dt>Bank</dt>
            <dd>{s.bank_id}</dd>
            {hasBank && (
              <>
                <dt>Bank linked</dt>
                <dd>
                  <span className="nbadge tone-success">Yes · {s.customer_account_type}</span>
                </dd>
              </>
            )}
            {s.note && (
              <>
                <dt>Note</dt>
                <dd>{s.note}</dd>
              </>
            )}
            {cpId && (
              <>
                <dt>MT counterparty</dt>
                <dd className="cell-id">{cpId}</dd>
              </>
            )}
            {extAcct && (
              <>
                <dt>MT ext. account</dt>
                <dd className="cell-id">{extAcct}</dd>
              </>
            )}
            {s.column_ref && (
              <>
                <dt>MT payment order</dt>
                <dd>
                  <a
                    className="cell-id"
                    href={`https://app.moderntreasury.com/payment-orders/${s.column_ref}`}
                    target="_blank"
                    rel="noreferrer"
                  >
                    {s.column_ref}
                  </a>
                </dd>
              </>
            )}
            <dt>Created</dt>
            <dd className="cell-muted">{fmtDateTime(s.created_at)}</dd>
            <dt>Expires</dt>
            <dd className="cell-muted">{fmtDateTime(s.expires_at)}</dd>
          </dl>

          <div style={{ marginTop: 22 }} className="nav-section">
            Lifecycle
          </div>
          <div className="timeline">
            {steps.map((st, i) => (
              <div className="tl-item" key={i}>
                <span className={`tl-dot ${st.state}`}>
                  <Icon name={st.state === 'done' ? 'check' : st.state === 'now' ? 'loader' : 'circle'} size={12} />
                </span>
                <div>
                  <div className="tl-label">{st.label}</div>
                  <div className="tl-time">{st.time}</div>
                </div>
              </div>
            ))}
          </div>
        </div>
        <div style={{ textAlign: 'center' }}>
          <div className="qr-well">
            {qr ? <span dangerouslySetInnerHTML={{ __html: qr }} /> : <Spinner />}
          </div>
          <div className="paylink">{payUrl}</div>
        </div>
      </div>
    </Card>
  );
}
