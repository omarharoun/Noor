import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtMoney, fmtCompact, fmtDate } from '../lib/format';
import { Card, StatTile, Spinner, Empty } from '../components/primitives';

const TONE: Record<string, string> = {
  completed: 'success',
  credited: 'success',
  processing: 'info',
  pending: 'warning',
  failed: 'danger',
  returned: 'danger',
};

export function Deposits() {
  const { data, loading, error } = useAsync(() => api.depositsAll(), []);
  const rows = data?.deposits ?? [];
  const settled = rows.filter((d) => d.status === 'completed' || d.status === 'credited');
  const settledTotal = settled.reduce((a, d) => a + d.amount, 0);

  return (
    <div>
      <div className="stat-grid" style={{ gridTemplateColumns: 'repeat(3,1fr)' }}>
        <StatTile icon="download" label="Funded volume" value={fmtCompact(settledTotal)} sub={`${settled.length} top-ups`} />
        <StatTile icon="list" label="Total deposits" value={rows.length} />
        <StatTile icon="clock" label="In flight" value={rows.length - settled.length} />
      </div>
      <Card title="Wallet top-ups across merchants" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Merchant</th>
                <th>Amount</th>
                <th>Status</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((d, idx) => (
                <tr key={idx}>
                  <td className="cell-strong">{d.merchant}</td>
                  <td className="cell-amt">{fmtMoney(d.amount)}</td>
                  <td>
                    <span className={`nbadge tone-${TONE[d.status] || 'neutral'}`}>
                      <span className="nbadge-dot" />
                      {d.status}
                    </span>
                  </td>
                  <td className="cell-muted">{fmtDate(d.created_at)}</td>
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
          {!loading && !error && rows.length === 0 && <Empty>No deposits yet.</Empty>}
        </div>
      </Card>
    </div>
  );
}
