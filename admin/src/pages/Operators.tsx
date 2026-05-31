import { useState } from 'react';
import { api } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { fmtDate } from '../lib/format';
import { Card, Button, Avatar, Spinner, Empty } from '../components/primitives';

export function Operators() {
  const { data, loading, error, reload } = useAsync(() => api.operators(), []);
  const [adding, setAdding] = useState(false);
  const operators = data?.operators ?? [];

  return (
    <Card
      title="Operators"
      action={
        <Button variant="primary" size="sm" icon="plus" onClick={() => setAdding(true)}>
          Add operator
        </Button>
      }
      pad={false}
    >
      {adding && <AddOperatorModal onClose={() => setAdding(false)} onCreated={reload} />}
      <div className="tbl-wrap">
        <table className="ntbl">
          <thead>
            <tr>
              <th>Operator</th>
              <th>Role</th>
              <th>Status</th>
              <th>Created</th>
            </tr>
          </thead>
          <tbody>
            {operators.map((o) => (
              <tr key={o.id}>
                <td>
                  <span className="cell-name">
                    <Avatar name={o.name} />
                    <span>
                      <span className="cell-strong" style={{ display: 'block' }}>{o.name}</span>
                      <span className="cell-muted" style={{ fontSize: 11.5 }}>{o.email}</span>
                    </span>
                  </span>
                </td>
                <td>
                  <span className={`nbadge ${o.role === 'owner' ? 'tone-warning' : 'tone-neutral'}`} style={{ textTransform: 'capitalize' }}>
                    {o.role}
                  </span>
                </td>
                <td>
                  <span className={`nbadge ${o.is_active ? 'tone-success' : 'tone-danger'}`}>
                    {o.is_active ? 'Active' : 'Disabled'}
                  </span>
                </td>
                <td className="cell-muted">{fmtDate(o.created_at)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {loading && <div className="center-load"><Spinner /></div>}
        {error && <div className="flash error" style={{ margin: 16 }}>{error}</div>}
        {!loading && !error && operators.length === 0 && <Empty>No operators yet.</Empty>}
      </div>
    </Card>
  );
}

function AddOperatorModal({ onClose, onCreated }: { onClose: () => void; onCreated: () => void }) {
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [role, setRole] = useState('operator');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const submit = async () => {
    setBusy(true);
    setErr(null);
    try {
      if (!name.trim() || !email.trim()) throw new Error('Name and email are required');
      if (password.length < 12) throw new Error('Password must be at least 12 characters');
      await api.createOperator({ email: email.trim(), name: name.trim(), password, role });
      onCreated();
      onClose();
    } catch (e) {
      // 401 here means the signed-in operator isn't an owner.
      setErr(e instanceof Error ? e.message : String(e));
      setBusy(false);
    }
  };

  return (
    <div className="modal-scrim" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">Add operator</div>
        <div className="modal-body">
          <div>
            <label className="field-label">Name</label>
            <input className="field-input" value={name} onChange={(e) => setName(e.target.value)} placeholder="Jane Operator" />
          </div>
          <div>
            <label className="field-label">Email</label>
            <input className="field-input" value={email} onChange={(e) => setEmail(e.target.value)} placeholder="jane@noor.local" />
          </div>
          <div>
            <label className="field-label">Password (min 12 chars)</label>
            <input className="field-input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
          </div>
          <div>
            <label className="field-label">Role</label>
            <select className="field-input" value={role} onChange={(e) => setRole(e.target.value)}>
              <option value="operator">operator</option>
              <option value="owner">owner</option>
            </select>
          </div>
          {err && <div className="flash error" style={{ marginBottom: 0 }}>{err}</div>}
        </div>
        <div className="modal-foot">
          <Button variant="ghost" size="md" onClick={onClose}>Cancel</Button>
          <Button variant="primary" size="md" icon="plus" onClick={submit} disabled={busy}>
            {busy ? 'Creating…' : 'Create operator'}
          </Button>
        </div>
      </div>
    </div>
  );
}
