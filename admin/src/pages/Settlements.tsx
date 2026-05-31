import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtMoney, fmtCompact, fmtDate } from '../lib/format';
import { Card, StatTile, Avatar, Spinner, Empty } from '../components/primitives';

export function Settlements() {
  const { data, loading, error } = useAsync(() => api.settlements(), []);
  const rows = data?.settlements ?? [];
  const totalVolume = rows.reduce((a, s) => a + s.total_volume_cents, 0);
  const lastActivity = rows
    .map((s) => s.last_payment_date)
    .filter(Boolean)
    .sort()
    .pop();

  return (
    <div>
      <div className="stat-grid" style={{ gridTemplateColumns: 'repeat(3,1fr)' }}>
        <StatTile icon="wallet" label="Settled volume" value={fmtCompact(totalVolume)} sub="completed sessions" />
        <StatTile icon="store" label="Merchants" value={rows.length} sub="with settled payments" />
        <StatTile icon="clock" label="Last activity" value={lastActivity ? fmtDate(lastActivity) : '—'} />
      </div>
      <Card title="Settlements by merchant" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Merchant</th>
                <th>Payments</th>
                <th>Settled volume</th>
                <th>Last payment</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((s) => (
                <tr key={s.merchant_id}>
                  <td>
                    <span className="cell-name">
                      <Avatar name={s.merchant_name ?? '?'} size={26} />
                      <span className="cell-strong">{s.merchant_name ?? s.merchant_id.slice(0, 8) + '…'}</span>
                    </span>
                  </td>
                  <td className="cell-muted">{s.total_count}</td>
                  <td className="cell-amt">{fmtMoney(s.total_volume_cents)}</td>
                  <td className="cell-muted">{fmtDate(s.last_payment_date)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {loading && <div className="center-load"><Spinner /></div>}
          {error && <div className="flash error" style={{ margin: 16 }}>{error}</div>}
          {!loading && !error && rows.length === 0 && <Empty>No settlements yet.</Empty>}
        </div>
      </Card>
    </div>
  );
}
