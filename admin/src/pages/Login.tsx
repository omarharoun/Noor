import { useState, type FormEvent } from 'react';
import { useAuth } from '../lib/auth';

export function Login() {
  const { login } = useAuth();
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  async function handleSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      await login(email, password);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Sign in failed. Please try again.');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div style={styles.root}>
      <div style={styles.card}>
        {/* Noor logo mark */}
        <div style={styles.logoRow}>
          <div style={styles.glyph}>
            <svg viewBox="0 0 30 30" style={{ position: 'absolute', inset: 0 }}>
              <path d="M20 8.5a8.2 8.2 0 1 0 0 13 6.4 6.4 0 1 1 0-13Z" fill="var(--cream-deep)" />
            </svg>
          </div>
          <span style={styles.brandWord}>Noor</span>
        </div>

        <h1 style={styles.heading}>Sign in to Noor</h1>
        <p style={styles.sub}>Admin console — authorized personnel only.</p>

        {error && (
          <div className="flash error" role="alert">
            {error}
          </div>
        )}

        <form onSubmit={handleSubmit} style={styles.form}>
          <div>
            <label className="field-label" htmlFor="login-email">
              Email
            </label>
            <input
              id="login-email"
              type="email"
              className="field-input"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              placeholder="you@noor.com"
              autoComplete="email"
              required
              disabled={loading}
            />
          </div>

          <div>
            <label className="field-label" htmlFor="login-password">
              Password
            </label>
            <input
              id="login-password"
              type="password"
              className="field-input"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              placeholder="••••••••"
              autoComplete="current-password"
              required
              disabled={loading}
            />
          </div>

          <button
            type="submit"
            className="nbtn nbtn-primary nbtn-md"
            style={{ width: '100%', justifyContent: 'center', marginTop: 4 }}
            disabled={loading}
          >
            {loading ? 'Signing in…' : 'Sign in'}
          </button>
        </form>
      </div>
    </div>
  );
}

const styles = {
  root: {
    minHeight: '100vh',
    background: 'var(--cream)',
    display: 'grid',
    placeItems: 'center',
    padding: '20px',
  } as React.CSSProperties,
  card: {
    background: 'var(--surface)',
    border: '1px solid var(--border)',
    borderRadius: 'var(--r-xl)',
    boxShadow: 'var(--shadow-lg)',
    padding: '40px 36px',
    width: '100%',
    maxWidth: '400px',
  } as React.CSSProperties,
  logoRow: {
    display: 'flex',
    alignItems: 'center',
    gap: '10px',
    marginBottom: '28px',
  } as React.CSSProperties,
  glyph: {
    width: '32px',
    height: '32px',
    borderRadius: '10px',
    background: 'var(--clay)',
    position: 'relative',
    flexShrink: 0,
  } as React.CSSProperties,
  brandWord: {
    fontFamily: 'var(--font-serif)',
    fontSize: '22px',
    fontWeight: 500,
    letterSpacing: '-0.3px',
    color: 'var(--ink)',
  } as React.CSSProperties,
  heading: {
    fontFamily: 'var(--font-serif)',
    fontSize: '28px',
    fontWeight: 500,
    color: 'var(--ink)',
    letterSpacing: '-0.01em',
    lineHeight: 1.2,
    marginBottom: '6px',
  } as React.CSSProperties,
  sub: {
    fontSize: '13.5px',
    color: 'var(--ink-3)',
    marginBottom: '24px',
  } as React.CSSProperties,
  form: {
    display: 'flex',
    flexDirection: 'column',
    gap: '14px',
  } as React.CSSProperties,
} as const;
