// Lightweight hand-rolled SVG charts, ported from the Depost console UI kit.

export function BarChart({ data }: { data: { rail: string; count: number; volume: number }[] }) {
  const w = 460, h = 200, padL = 8, padB = 26, padT = 10;
  if (!data.length) return <Empty />;
  const max = Math.max(...data.map((d) => d.volume)) || 1;
  const bw = (w - padL) / data.length;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} width="100%" height="200" style={{ overflow: 'visible' }}>
      {[0.25, 0.5, 0.75, 1].map((t, i) => (
        <line
          key={i}
          x1={padL}
          x2={w}
          y1={padT + (h - padT - padB) * (1 - t)}
          y2={padT + (h - padT - padB) * (1 - t)}
          stroke="var(--viz-grid)"
          strokeWidth="1"
        />
      ))}
      {data.map((d, i) => {
        const bh = (d.volume / max) * (h - padT - padB);
        const x = padL + i * bw + bw * 0.22;
        const bWidth = bw * 0.56;
        return (
          <g key={i}>
            <rect x={x} y={h - padB - bh} width={bWidth} height={bh} rx="6" fill="var(--clay)" opacity={0.9} />
            <text x={x + bWidth / 2} y={h - padB + 16} textAnchor="middle" fontSize="11" fontFamily="var(--font-mono)" fill="var(--ink-3)">
              {d.rail.toUpperCase()}
            </text>
            <text x={x + bWidth / 2} y={h - padB - bh - 6} textAnchor="middle" fontSize="11" fontWeight="600" fill="var(--ink-2)">
              {d.count}
            </text>
          </g>
        );
      })}
    </svg>
  );
}

export function LineChart({ data }: { data: { date: string; count: number; volume: number }[] }) {
  const w = 460, h = 200, padL = 8, padR = 8, padB = 26, padT = 14;
  if (data.length < 2) return <Empty />;
  const max = (Math.max(...data.map((d) => d.volume)) || 1) * 1.1;
  const innerW = w - padL - padR, innerH = h - padT - padB;
  const pts = data.map((d, i) => [
    padL + innerW * (i / (data.length - 1)),
    padT + innerH * (1 - d.volume / max),
  ]);
  const path = pts.map((p, i) => (i === 0 ? 'M' : 'L') + p[0].toFixed(1) + ' ' + p[1].toFixed(1)).join(' ');
  const area = path + ` L${pts[pts.length - 1][0].toFixed(1)} ${padT + innerH} L${pts[0][0].toFixed(1)} ${padT + innerH} Z`;
  return (
    <svg viewBox={`0 0 ${w} ${h}`} width="100%" height="200" style={{ overflow: 'visible' }}>
      <defs>
        <linearGradient id="lg" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="var(--clay)" stopOpacity="0.16" />
          <stop offset="100%" stopColor="var(--clay)" stopOpacity="0" />
        </linearGradient>
      </defs>
      {[0.25, 0.5, 0.75, 1].map((t, i) => (
        <line key={i} x1={padL} x2={w - padR} y1={padT + innerH * (1 - t)} y2={padT + innerH * (1 - t)} stroke="var(--viz-grid)" strokeWidth="1" />
      ))}
      <path d={area} fill="url(#lg)" />
      <path d={path} fill="none" stroke="var(--clay)" strokeWidth="2.25" strokeLinecap="round" strokeLinejoin="round" />
      {pts.map((p, i) => (
        <g key={i}>
          <circle cx={p[0]} cy={p[1]} r="3.25" fill="var(--surface)" stroke="var(--clay)" strokeWidth="2" />
          <text x={p[0]} y={h - padB + 16} textAnchor="middle" fontSize="10" fill="var(--ink-3)">
            {data[i].date}
          </text>
        </g>
      ))}
    </svg>
  );
}

function Empty() {
  return (
    <div style={{ height: 200, display: 'grid', placeItems: 'center', color: 'var(--ink-4)', fontSize: 13 }}>
      Not enough data yet
    </div>
  );
}
