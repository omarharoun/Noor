import { useState } from 'react';
import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtDate } from '../lib/format';
import { Card, Button, StatusBadge, RiskTag, Avatar, Spinner, Empty } from '../components/primitives';

export function Merchants() {
  const { data, loading, error, reload } = useAsync(() => api.merchants({ limit: 200 }), []);
  const [adding, setAdding] = useState(false);
  const merchants = data?.merchants ?? [];

  return (
    <Card
      title="Merchants"
      action={
        <Button variant="primary" size="sm" icon="plus" onClick={() => setAdding(true)}>
          Add merchant
        </Button>
      }
      pad={false}
    >
      {adding && <AddMerchantModal onClose={() => setAdding(false)} onCreated={reload} />}
      <div className="tbl-wrap">
        <table className="ntbl">
          <thead>
            <tr>
              <th>Merchant</th>
              <th>Business</th>
              <th>Status</th>
              <th>KYC</th>
              <th>Risk</th>
              <th>Joined</th>
            </tr>
          </thead>
          <tbody>
            {merchants.map((m) => (
              <tr key={m.id}>
                <td>
                  <span className="cell-name">
                    <Avatar name={m.name} />
                    <span>
                      <span className="cell-strong" style={{ display: 'block' }}>
                        {m.name}
                      </span>
                      <span className="cell-muted" style={{ fontSize: 11.5 }}>
                        {m.email}
                      </span>
                    </span>
                  </span>
                </td>
                <td>
                  <span className="cell-strong" style={{ fontWeight: 500 }}>
                    {m.business_name ?? '—'}
                  </span>
                  <br />
                  <span className="cell-muted" style={{ fontSize: 11.5 }}>
                    {m.business_type ?? ''}
                  </span>
                </td>
                <td>
                  <StatusBadge status={m.status} />
                </td>
                <td>
                  <StatusBadge status={m.kyc_status} dot={false} />
                </td>
                <td>
                  <RiskTag level={m.risk_level} />
                </td>
                <td className="cell-muted">{fmtDate(m.created_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {loading && <div className="center-load"><Spinner /></div>}
        {error && <div className="flash error" style={{ margin: 16 }}>{error}</div>}
        {!loading && !error && merchants.length === 0 && <Empty>No merchants yet. Add one to get started.</Empty>}
      </div>
    </Card>
  );
}

function AddMerchantModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      if (!name.trim() || !email.trim()) throw new Error('Name and email are required');
      await api.createMerchant({ name: name.trim(), email: email.trim() });
      onCreated();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-scrim" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">Add merchant</div>
        <div className="modal-body">
          <div>
            <label className="field-label">Name</label>
            <input className="field-input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Acme Inc" />
          </div>
          <div>
            <label className="field-label">Email</label>
            <input className="field-input" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="ops@acme.com" />
          </div>
          {error && <div className="flash error" style={{ marginBottom: 0 }}>{error}</div>}
        </div>
        <div className="modal-foot">
          <Button variant="ghost" size="md" onClick={onClose}>
            Cancel
          </Button>
          <Button variant="primary" size="md" icon="plus" onClick={submit} disabled={busy}>
            {busy ? 'Creating…' : 'Create merchant'}
          </Button>
        </div>
      </div>
    </div>
  );
}
