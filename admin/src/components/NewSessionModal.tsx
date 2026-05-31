import { useState } from 'react';
import { api, createSession, fetchQrSvg, type Merchant } from '../lib/api';
import { useAsync } from '../lib/useAsync';
import { Button, Spinner } from './primitives';
import { fmtMoney } from '../lib/format';

// Create a payment session from the ops console. POSTs to /api/sessions and
// shows the resulting pay link + QR so an operator can hand it off.
export function NewSessionModal({
  merchants,
  onClose,
  onCreated,
}: {
  merchants: Merchant[];
  onClose: () => void;
  onCreated: () => void;
}) {
  const banks = useAsync(() => api.banks(), []);
  const [merchantId, setMerchantId] = useState(merchants[0]?.id ?? '');
  const [bankId, setBankId] = useState('');
  const [amount, setAmount] = useState('10.00');
  const [note, setNote] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<{ id: string; qr: string | null } | null>(null);

  const bankList = banks.data?.banks ?? [];
  const effectiveBank = bankId || bankList[0]?.id || '';

  const submit = async () => {
    setBusy(true);
    setError(null);
    try {
      const cents = Math.round(parseFloat(amount) * 100);
      if (!merchantId) throw new Error('Select a merchant');
      if (!effectiveBank) throw new Error('Select a bank');
      if (!Number.isFinite(cents) || cents <= 0) throw new Error('Enter a valid amount');
      const r = await createSession({
        merchant_id: merchantId,
        bank_id: effectiveBank,
        amount_cents: cents,
        currency: 'USD',
        note: note || undefined,
      });
      const qr = await fetchQrSvg(r.id).catch(() => null);
      setCreated({ id: r.id, qr });
      onCreated();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };

  const payUrl = created ? `${window.location.origin}/pay/${created.id}` : '';

  return (
    <div className="modal-scrim" onClick={onClose}>
      <div className="modal" onClick={(e) => e.stopPropagation()}>
        <div className="modal-head">New payment session</div>
        {created ? (
          <div className="modal-body" style={{ alignItems: 'center', textAlign: 'center' }}>
            <div className="qr-well">{created.qr ? <span dangerouslySetInnerHTML={{ __html: created.qr }} /> : <Spinner />}</div>
            <div className="paylink">{payUrl}</div>
            <div className="flash success" style={{ marginBottom: 0 }}>
              Session {created.id.slice(0, 8)}… created · {fmtMoney(Math.round(parseFloat(amount) * 100))}
            </div>
          </div>
        ) : (
          <div className="modal-body">
            <div>
              <label className="field-label">Merchant</label>
              <select className="field-input" value={merchantId} onChange={(e) => setMerchantId(e.target.value)}>
                {merchants.map((m) => (
                  <option key={m.id} value={m.id}>
                    {m.name} ({m.email})
                  </option>
                ))}
              </select>
            </div>
            <div>
              <label className="field-label">Bank</label>
              <select className="field-input" value={effectiveBank} onChange={(e) => setBankId(e.target.value)} disabled={banks.loading}>
                {bankList.map((b) => (
                  <option key={b.id} value={b.id}>
                    {b.name} ({b.id})
                  </option>
                ))}
              </select>
            </div>
            <div style={{ display: 'flex', gap: 12 }}>
              <div style={{ flex: 1 }}>
                <label className="field-label">Amount (USD)</label>
                <input className="field-input" value={amount} onChange={(e) => setAmount(e.target.value)} inputMode="decimal" />
              </div>
              <div style={{ flex: 2 }}>
                <label className="field-label">Note</label>
                <input className="field-input" value={note} onChange={(e) => setNote(e.target.value)} placeholder="Optional" />
              </div>
            </div>
            {error && <div className="flash error" style={{ marginBottom: 0 }}>{error}</div>}
          </div>
        )}
        <div className="modal-foot">
          {created ? (
            <>
              <Button variant="secondary" size="md" onClick={() => window.open(payUrl, '_blank')} icon="external-link">
                Open pay page
              </Button>
              <Button variant="primary" size="md" onClick={onClose}>
                Done
              </Button>
            </>
          ) : (
            <>
              <Button variant="ghost" size="md" onClick={onClose}>
                Cancel
              </Button>
              <Button variant="primary" size="md" icon="plus" onClick={submit} disabled={busy}>
                {busy ? 'Creating…' : 'Create session'}
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
