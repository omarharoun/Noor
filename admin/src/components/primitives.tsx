import type { ReactNode, CSSProperties } from 'react';
import { Icon } from './Icon';

/* ---- Button ---- */
type Variant = 'primary' | 'secondary' | 'ghost' | 'success' | 'danger';
export function Button({
  variant = 'secondary',
  icon,
  children,
  onClick,
  size = 'md',
  disabled,
  style,
  pulse,
}: {
  variant?: Variant;
  icon?: string;
  children?: ReactNode;
  onClick?: () => void;
  size?: 'sm' | 'md';
  disabled?: boolean;
  style?: CSSProperties;
  pulse?: boolean;
}) {
  const cls = ['nbtn', `nbtn-${variant}`, `nbtn-${size}`, pulse ? 'nbtn-pulse' : ''].join(' ').trim();
  return (
    <button className={cls} onClick={onClick} disabled={disabled} style={style}>
      {icon && <Icon name={icon} size={size === 'sm' ? 14 : 15} />}
      {children}
    </button>
  );
}

/* ---- Status badge ---- */
const STATUS_MAP: Record<string, [string, string]> = {
  completed: ['success', 'Completed'],
  processing: ['info', 'Processing'],
  pending: ['warning', 'Pending'],
  authorized: ['neutral', 'Authorized'],
  expired: ['danger', 'Expired'],
  failed: ['danger', 'Failed'],
  active: ['success', 'Active'],
  review: ['warning', 'In review'],
  verified: ['success', 'Verified'],
  not_started: ['neutral', 'Not started'],
  delivered: ['success', 'Delivered'],
  sent: ['success', 'Sent'],
  retrying: ['warning', 'Retrying'],
  sending: ['info', 'Sending'],
  exhausted: ['danger', 'Exhausted'],
};
export function StatusBadge({ status, dot = true }: { status: string; dot?: boolean }) {
  const [tone, label] = STATUS_MAP[status] || ['neutral', status];
  return (
    <span className={`nbadge tone-${tone}`}>
      {dot && <span className="nbadge-dot" />}
      {label}
    </span>
  );
}

export function RailTag({ rail }: { rail?: string | null }) {
  if (!rail) return <span className="rail-empty">—</span>;
  return <span className="rail-tag">{rail.toUpperCase()}</span>;
}

export function RiskTag({ level }: { level?: string | null }) {
  if (!level) return <span className="rail-empty">—</span>;
  const tone = level === 'low' ? 'success' : level === 'high' ? 'danger' : 'warning';
  return (
    <span className={`nbadge tone-${tone}`} style={{ textTransform: 'capitalize' }}>
      {level}
    </span>
  );
}

/* ---- Card ---- */
export function Card({
  title,
  action,
  children,
  pad = true,
  live,
}: {
  title?: ReactNode;
  action?: ReactNode;
  children: ReactNode;
  pad?: boolean;
  live?: boolean;
}) {
  return (
    <section className="ncard">
      {title && (
        <header className="ncard-head">
          <h2 className="ncard-title">
            {title}
            {live && (
              <span className="live-badge">
                <span className="live-dot" />
                Live
              </span>
            )}
          </h2>
          {action}
        </header>
      )}
      <div className={pad ? 'ncard-body' : ''}>{children}</div>
    </section>
  );
}

/* ---- Stat tile ---- */
export function StatTile({
  icon,
  label,
  value,
  delta,
  deltaTone = 'success',
  sub,
}: {
  icon: string;
  label: string;
  value: ReactNode;
  delta?: string;
  deltaTone?: string;
  sub?: string;
}) {
  return (
    <div className="stat-tile">
      <div className="stat-ico">
        <Icon name={icon} size={16} color="var(--clay-600)" />
      </div>
      <div className="stat-label">{label}</div>
      <div className="stat-value">{value}</div>
      {delta && <div className={`stat-delta tone-${deltaTone}`}>{delta}</div>}
      {sub && <div className="stat-sub">{sub}</div>}
    </div>
  );
}

/* ---- Avatar (initials) ---- */
const TINTS = ['var(--clay-100)', 'var(--info-fill)', 'var(--success-fill)', 'var(--warning-fill)'];
const INKS = ['var(--clay-700)', 'var(--info)', 'var(--success)', 'var(--warning)'];
export function Avatar({ name, size = 30 }: { name: string; size?: number }) {
  const initials = (name || '?')
    .split(' ')
    .map((w) => w[0])
    .slice(0, 2)
    .join('');
  const idx = (name || '?').charCodeAt(0) % 4;
  return (
    <span
      className="avatar"
      style={{
        width: size,
        height: size,
        background: TINTS[idx],
        color: INKS[idx],
        fontSize: size * 0.4,
        display: 'grid',
        placeItems: 'center',
        borderRadius: '50%',
        fontWeight: 700,
        flex: 'none',
      }}
    >
      {initials}
    </span>
  );
}

/* ---- Empty / error states ---- */
export function Empty({ children }: { children: ReactNode }) {
  return (
    <div style={{ textAlign: 'center', padding: '40px 20px', color: 'var(--ink-3)', fontSize: 13.5 }}>
      {children}
    </div>
  );
}

export function Spinner({ dark = true }: { dark?: boolean }) {
  return <span className={`spinner ${dark ? 'spinner-dark' : ''}`} style={{ display: 'inline-block', width: 20, height: 20 }} />;
}
