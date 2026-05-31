import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtCompact, fmtMoney, short, ago } from '../lib/format';
import { Card, StatTile, StatusBadge, RailTag, Spinner, Empty } from '../components/primitives';
import { Button } from '../components/primitives';
import { BarChart, LineChart } from '../components/charts';
import type { Ctx } from '../App';

export function Dashboard({ ctx }: { ctx: Ctx }) {
  const stats = useAsync(() => api.stats(), []);
  const recent = useAsync(() => api.sessions({ limit: 6 }), []);

  if (stats.loading) return <div className="center-load"><Spinner /></div>;
  if (stats.error || !stats.data) return <div className="flash error">{stats.error ?? 'Failed to load stats'}</div>;

  const s = stats.data;
  const byRail = s.by_rail.map((r) => ({ rail: r.rail_used ?? 'n/a', count: r.count, volume: r.volume }));
  const last7 = s.last_7_days.map((d) => ({
    date: new Date(d.date).toLocaleDateString('en-US', { month: 'numeric', day: 'numeric' }),
    count: d.count,
    volume: d.volume,
  }));

  return (
    <div>
      <div className="stat-grid">
        <StatTile icon="trending-up" label="Total volume" value={fmtCompact(s.total_volume_cents)} sub="completed sessions" />
        <StatTile icon="arrow-left-right" label="Today" value={s.today_total_transactions} sub="sessions created" />
        <StatTile icon="check-circle-2" label="Completed today" value={s.today_completed} />
        <StatTile icon="clock" label="Pending" value={s.pending_count} sub="awaiting confirmation" />
      </div>

      <div className="chart-grid">
        <Card title="Volume by rail">
          <BarChart data={byRail} />
        </Card>
        <Card title="Last 7 days">
          <LineChart data={last7} />
        </Card>
      </div>

      <div className="section-gap" />

      <Card
        title="Recent activity"
        live
        action={
          <Button variant="ghost" size="sm" icon="arrow-right" onClick={() => ctx.go('sessions')}>
            All sessions
          </Button>
        }
        pad={false}
      >
        <div className="tbl-wrap">
          <table className="ntbl">
            <thead>
              <tr>
                <th>Session</th>
                <th>Merchant</th>
                <th>Amount</th>
                <th>Status</th>
                <th>Rail</th>
                <th>When</th>
              </tr>
            </thead>
            <tbody>
              {recent.data?.sessions.map((sess) => (
                <tr key={sess.id} onClick={() => ctx.go('sessions')}>
                  <td className="cell-id">{short(sess.id)}</td>
                  <td>
                    <span className="cell-strong">{ctx.merchantName(sess.merchant_id)}</span>
                  </td>
                  <td className="cell-amt">{fmtMoney(sess.amount_cents)}</td>
                  <td>
                    <StatusBadge status={sess.status} />
                  </td>
                  <td>
                    <RailTag rail={sess.rail_used} />
                  </td>
                  <td className="cell-muted">{ago(sess.created_at)}</td>
                </tr>
              ))}
            </tbody>
          </table>
          {recent.loading && <div className="center-load"><Spinner /></div>}
          {recent.data && recent.data.sessions.length === 0 && (
            <Empty>No sessions yet. Create one to start accepting payments.</Empty>
          )}
        </div>
      </Card>
    </div>
  );
}
