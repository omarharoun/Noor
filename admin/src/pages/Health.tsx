import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtMoney, short, ago } from '../lib/format';
import { Card, Button, StatusBadge, RailTag, Spinner, Empty } from '../components/primitives';
import { Icon } from '../components/Icon';

// A health counter: green when zero, amber/red when there's something to look at.
function IssueTile({
  label,
  value,
  critical,
}: {
  label: string;
  value: number | undefined;
  critical?: boolean;
}) {
  const v = value ?? 0;
  const tone = v === 0 ? 'success' : critical ? 'danger' : 'warning';
  const color = `var(--${tone})`;
  return (
    <div className="stat-tile" style={{ borderColor: v === 0 ? undefined : color }}>
      <div className="stat-ico" style={{ background: v === 0 ? 'var(--success-fill)' : `var(--${tone}-fill)` }}>
        <Icon name={v === 0 ? 'check-circle-2' : critical ? 'circle' : 'clock'} size={16} color={color} />
      </div>
      <div className="stat-label">{label}</div>
      <div className="stat-value" style={{ color: v === 0 ? 'var(--ink)' : color }}>
        {value === undefined ? '—' : v}
      </div>
    </div>
  );
}

export function Health() {
  const health = useAsync(() => api.health(), []);
  const stuck = useAsync(() => api.sessions({ status: 'processing', limit: 50 }), []);
  const hooks = useAsync(() => api.webhooks({ limit: 100 }), []);
  const audit = useAsync(() => api.audit(), []);

  const h = health.data;
  const failedHooks = (hooks.data?.events ?? []).filter(
    (e) => e.status === 'failed' || e.status === 'exhausted'
  );

  const refresh = () => {
    health.reload();
    stuck.reload();
    hooks.reload();
    audit.reload();
  };

  return (
    <div>
      <div className="filterbar">
        <div className="filter-spacer" />
        <Button variant="secondary" size="sm" icon="rotate-cw" onClick={refresh}>
          Refresh
        </Button>
      </div>

      {health.error && <div className="flash error">{health.error}</div>}

      <div className="stat-grid" style={{ gridTemplateColumns: 'repeat(4, 1fr)' }}>
        <IssueTile label="Unbalanced ledger entries" value={h?.unbalanced_journal_entries} critical />
        <IssueTile label="Stuck processing (>5m)" value={h?.stuck_processing} critical />
        <IssueTile label="Failed sessions" value={h?.failed_sessions} />
        <IssueTile label="Returned" value={h?.returned_sessions} />
        <IssueTile label="Reversed" value={h?.reversed_sessions} critical />
        <IssueTile label="Webhooks failed" value={h?.webhooks_failed} />
        <IssueTile label="Webhooks exhausted" value={h?.webhooks_exhausted} critical />
      </div>
      {health.loading && <div className="center-load"><Spinner /></div>}

      <div className="section-gap" />

      <Card title="In-flight / stuck sessions" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Session</th>
                <th>Amount</th>
                <th>Status</th>
                <th>Rail</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              {(stuck.data?.sessions ?? []).map((s) => (
                <tr key={s.id}>
                  <td className="cell-id">{short(s.id)}</td>
                  <td className="cell-amt">{fmtMoney(s.amount_cents)}</td>
                  <td><StatusBadge status={s.status} /></td>
                  <td><RailTag rail={s.rail_used} /></td>
                  <td className="cell-muted">{ago(s.updated_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {stuck.loading && <div className="center-load"><Spinner /></div>}
          {!stuck.loading && (stuck.data?.sessions.length ?? 0) === 0 && (
            <Empty>No in-flight sessions — nothing stuck. ✓</Empty>
          )}
        </div>
      </Card>

      <div className="section-gap" />

      <Card title="Failed webhook deliveries" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Event</th>
                <th>Status</th>
                <th>Attempts</th>
                <th>Created</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {failedHooks.map((w) => (
                <tr key={w.id}>
                  <td>
                    <span className="cell-id" style={{ fontSize: 11 }}>{short(w.id)}</span>{' '}
                    <span className="cell-strong" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>
                      {w.event_type}
                    </span>
                  </td>
                  <td><StatusBadge status={w.status} /></td>
                  <td className="cell-muted">{w.attempts}×</td>
                  <td className="cell-muted">{ago(w.created_at)}</td>
                  <td>
                    <Button
                      variant="ghost"
                      size="sm"
                      icon="rotate-cw"
                      onClick={() => api.retryWebhook(w.id).then(refresh).catch(() => {})}
                    >
                      Retry
                    </Button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {hooks.loading && <div className="center-load"><Spinner /></div>}
          {!hooks.loading && failedHooks.length === 0 && (
            <Empty>No failed webhook deliveries. ✓</Empty>
          )}
        </div>
      </Card>

      <div className="section-gap" />

      <Card title="Audit log" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>When</th>
                <th>Actor</th>
                <th>Action</th>
                <th>Target</th>
              </tr>
            </thead>
            <tbody>
              {(audit.data?.entries ?? []).map((e) => (
                <tr key={e.id}>
                  <td className="cell-muted">{ago(e.created_at)}</td>
                  <td>
                    <span className="cell-strong">{e.actor}</span>
                    {e.actor_role && <span className="cell-muted"> · {e.actor_role}</span>}
                  </td>
                  <td>
                    <span className="rail-tag">{e.action}</span>
                  </td>
                  <td className="cell-id">
                    {e.target_type ? `${e.target_type} ${short(e.target_id)}` : '—'}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          {audit.loading && <div className="center-load"><Spinner /></div>}
          {!audit.loading && (audit.data?.entries.length ?? 0) === 0 && (
            <Empty>No audit entries yet.</Empty>
          )}
        </div>
      </Card>
    </div>
  );
}
