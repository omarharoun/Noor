// Formatting helpers — money in serif tabular, references truncated mono,
// dates human in detail views and compact in tables (see SYSTEM_README).

export const fmtMoney = (cents: number, withSign = true): string =>
  (withSign ? '$' : '') +
  (cents / 100).toLocaleString('en-US', { minimumFractionDigits: 2, maximumFractionDigits: 2 });

export const fmtCompact = (cents: number): string => {
  const d = cents / 100;
  if (d >= 1000) return '$' + (d / 1000).toFixed(d >= 10000 ? 0 : 1) + 'k';
  return '$' + d.toFixed(0);
};

export const short = (s: string | null | undefined, n = 8): string =>
  (s || '').slice(0, n) + '…';

// Relative time from an ISO timestamp.
export const ago = (iso: string | null | undefined): string => {
  if (!iso) return '—';
  const ms = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(ms / 60000);
  if (mins < 1) return 'just now';
  if (mins < 60) return mins + 'm ago';
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return hrs + 'h ago';
  return Math.floor(hrs / 24) + 'd ago';
};

export const fmtDate = (iso: string | null | undefined): string =>
  iso ? new Date(iso).toLocaleDateString('en-US', { year: 'numeric', month: 'short', day: 'numeric' }) : '—';

export const fmtDateTime = (iso: string | null | undefined): string =>
  iso ? new Date(iso).toLocaleString('en-US', { month: 'short', day: 'numeric', hour: 'numeric', minute: '2-digit' }) : '—';
