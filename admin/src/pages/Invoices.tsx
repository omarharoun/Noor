import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtMoney, fmtCompact, fmtDate } from '../lib/format';
import { Card, StatTile, Spinner, Empty } from '../components/primitives';

const TONE: Record<string, string> = {
  paid: 'success',
  payment_pending: 'info',
  unpaid: 'warning',
  draft: 'neutral',
  voided: 'neutral',
  partially_paid: 'info',
};

export function Invoices() {
  const { data, loading, error } = useAsync(() => api.invoicesAll(), []);
  const rows = data?.invoices ?? [];
  const paid = rows.filter((i) => i.status === 'paid');
  const outstanding = rows.filter((i) => i.status !== 'paid' && i.status !== 'voided');
  const outstandingTotal = outstanding.reduce((a, i) => a + i.amount, 0);

  return (
    <div>
      <div className="stat-grid" style={{ gridTemplateColumns: 'repeat(3,1fr)' }}>
        <StatTile icon="file-text" label="Total invoices" value={rows.length} />
        <StatTile icon="check" label="Paid" value={paid.length} />
        <StatTile icon="clock" label="Outstanding" value={fmtCompact(outstandingTotal)} sub={`${outstanding.length} open`} />
      </div>
      <Card title="Invoices across merchants" pad={false}>
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Merchant</th>
                <th>Invoice #</th>
                <th>Customer</th>
                <th>Amount</th>
                <th>Status</th>
                <th>Created</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((i, idx) => (
                <tr key={idx}>
                  <td className="cell-strong">{i.merchant}</td>
                  <td className="cell-muted">{i.number || '—'}</td>
                  <td>{i.customer}</td>
                  <td className="cell-amt">{fmtMoney(i.amount)}</td>
                  <td>
                    <span className={`nbadge tone-${TONE[i.status] || 'neutral'}`}>
                      <span className="nbadge-dot" />
                      {i.status}
                    </span>
                  </td>
                  <td className="cell-muted">{fmtDate(i.created_at)}</td>
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
          {!loading && !error && rows.length === 0 && <Empty>No invoices yet.</Empty>}
        </div>
      </Card>
    </div>
  );
}
