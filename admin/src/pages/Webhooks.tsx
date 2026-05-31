import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { ago, short } from '../lib/format';
import { Card, Button, StatusBadge, Spinner, Empty } from '../components/primitives';

export function Webhooks() {
  const { data, loading, error, reload } = useAsync(() => api.webhooks({ limit: 100 }), []);
  const events = data?.events ?? [];

  const retry = async (id: string) => {
    try {
      await api.retryWebhook(id);
      reload();
    } catch {
      /* surfaced via reload / next render */
    }
  };

  return (
    <Card title="Webhook events" live pad={false}>
      <div className="tbl-wrap">
        <table className="ntbl">
          <thead>
            <tr>
              <th>Event</th>
              <th>Delivery</th>
              <th>Attempts</th>
              <th>Sent</th>
              <th>Created</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {events.map((w) => (
              <tr key={w.id}>
                <td>
                  <span className="cell-id" style={{ fontSize: 11 }}>
                    {short(w.id)}
                  </span>{' '}
                  <span className="cell-strong" style={{ fontFamily: 'var(--font-mono)', fontSize: 12 }}>
                    {w.event_type}
                  </span>
                </td>
                <td>
                  <StatusBadge status={w.status} />
                </td>
                <td className="cell-muted">{w.attempts}×</td>
                <td className="cell-muted">{w.sent_at ? ago(w.sent_at) : '—'}</td>
                <td className="cell-muted">{ago(w.created_at)}</td>
                <td>
                  {w.status === 'failed' || w.status === 'exhausted' ? (
                    <Button variant="ghost" size="sm" icon="rotate-cw" onClick={() => retry(w.id)}>
                      Retry
                    </Button>
                  ) : (
                    <Button variant="ghost" size="sm" icon="eye">
                      View
                    </Button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {loading && <div className="center-load"><Spinner /></div>}
        {error && <div className="flash error" style={{ margin: 16 }}>{error}</div>}
        {!loading && !error && events.length === 0 && <Empty>No webhook events yet.</Empty>}
      </div>
    </Card>
  );
}
