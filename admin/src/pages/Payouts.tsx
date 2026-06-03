import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtMoney, fmtCompact, fmtDate } from '../lib/format';
import { Card, StatTile, Spinner, Empty } from '../components/primitives';

const TONE: Record<string, string> = {
  completed: 'success',
  processing: 'info',
  pending_approval: 'warning',
  failed: 'danger',
  returned: 'danger',
  rejected: 'neutral',
};

export function Payouts() {
  const { data, loading, error } = useAsync(() => api.payoutsAll(), []);
  const rows = data?.payouts ?? [];
  const sent = rows.filter((p) => p.status === 'processing' || p.status === 'completed');
  const pending = rows.filter((p) => p.status === 'pending_approval');
  const totalSent = sent.reduce((a, p) => a + p.amount, 0);

  return (
    <div>
      <div className="stat-grid" style={{ gridTemplateColumns: 'repeat(3,1fr)' }}>
        <StatTile icon="send" label="Sent volume" value={fmtCompact(totalSent)} sub={`${sent.length} payouts`} />
        <StatTile icon="clock" label="Awaiting approval" value={pending.length} />
        <StatTile icon="list" label="Total payouts" value={rows.length} />
      </div>
      <Card title="Payouts across merchants" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Merchant</th>
                <th>Payee</th>
                <th>Amount</th>
                <th>Rail</th>
                <th>Status</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((p, i) => (
                <tr key={i}>
                  <td className="cell-strong">{p.merchant}</td>
                  <td>{p.payee}</td>
                  <td className="cell-amt">{fmtMoney(p.amount)}</td>
                  <td>
                    <span className="rail-tag">{(p.rail || '—').toUpperCase()}</span>
                  </td>
                  <td>
                    <span className={`nbadge tone-${TONE[p.status] || 'neutral'}`}>
                      <span className="nbadge-dot" />
                      {p.status}
                    </span>
                  </td>
                  <td className="cell-muted">{fmtDate(p.created_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {loading && (
            <div className="center-load">
              <Spinner />
            </div>
          )}
          {error && (
            <div className="flash error" style={{ margin: 16 }}>
              {error}
            </div>
          )}
          {!loading && !error && rows.length === 0 && <Empty>No payouts yet.</Empty>}
        </div>
      </Card>
    </div>
  );
}
