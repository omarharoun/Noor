import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { Card, Spinner, Empty } from '../components/primitives';
import { Icon } from '../components/Icon';

function RailSupport({ ok }: { ok: boolean }) {
  return <span className={`rs ${ok ? 'yes' : 'no'}`}>{ok ? '✓' : '—'}</span>;
}

export function Banks() {
  const { data, loading, error } = useAsync(() => api.banks(), []);
  const banks = data?.banks ?? [];

  return (
    <Card
      title="Banks"
      action={
        <span className="cell-muted" style={{ fontSize: 12.5 }}>
          {banks.length} institutions
        </span>
      }
      pad={false}
    >
      <div className="tbl-wrap">
        <table className="ntbl">
          <thead>
            <tr>
              <th>Bank</th>
              <th>Routing</th>
              <th>FedNow</th>
              <th>RTP</th>
              <th>Wire</th>
              <th>Default rail</th>
            </tr>
          </thead>
          <tbody>
            {banks.map((b) => {
              const rail = b.supports_fednow ? 'FEDNOW' : b.supports_rtp ? 'RTP' : 'ACH';
              return (
                <tr key={b.id}>
                  <td>
                    <span className="cell-name">
                      <span className="stat-ico" style={{ width: 26, height: 26 }}>
                        <Icon name="landmark" size={14} color="var(--clay-600)" />
                      </span>
                      <span className="cell-strong">{b.name}</span>
                    </span>
                  </td>
                  <td className="cell-id">{b.routing_number ?? b.id}</td>
                  <td>
                    <RailSupport ok={b.supports_fednow} />
                  </td>
                  <td>
                    <RailSupport ok={b.supports_rtp} />
                  </td>
                  <td>
                    <RailSupport ok={b.supports_wire} />
                  </td>
                  <td>
                    <span className="rail-tag">{rail}</span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        {loading && <div className="center-load"><Spinner /></div>}
        {error && <div className="flash error" style={{ margin: 16 }}>{error}</div>}
        {!loading && !error && banks.length === 0 && <Empty>No banks configured.</Empty>}
      </div>
    </Card>
  );
}
