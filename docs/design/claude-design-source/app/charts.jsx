/* ============================================================
   Charts — hand-built SVG (donut, area history, drift bars)
   ============================================================ */
const { useState: cUseState, useEffect: cUseEffect, useRef: cUseRef } = React;

/* ---------------- Donut ---------------- */
function Donut({ data, size = 196, thickness = 26, center, onHover }) {
  const [active, setActive] = cUseState(null);
  const total = data.reduce((s, d) => s + d.value, 0) || 1;
  const r = (size - thickness) / 2;
  const cx = size / 2, cy = size / 2;
  const C = 2 * Math.PI * r;
  const gap = 1.4; // degrees
  let offset = -90; // start at top
  const segs = data.map((d, i) => {
    const frac = d.value / total;
    const angle = frac * 360;
    const seg = { ...d, start: offset, sweep: angle, i };
    offset += angle;
    return seg;
  });
  const polar = (deg) => {
    const rad = (deg * Math.PI) / 180;
    return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
  };
  return (
    <div className="relative" style={{ width: size, height: size }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {segs.map((s) => {
          const a0 = s.start + gap / 2;
          const a1 = s.start + s.sweep - gap / 2;
          const [x0, y0] = polar(a0);
          const [x1, y1] = polar(a1);
          const large = (a1 - a0) > 180 ? 1 : 0;
          const isActive = active === s.i;
          return (
            <path key={s.id || s.i}
              d={`M ${x0} ${y0} A ${r} ${r} 0 ${large} 1 ${x1} ${y1}`}
              fill="none" stroke={s.color} strokeWidth={isActive ? thickness + 5 : thickness}
              strokeLinecap="round"
              style={{ transition: 'stroke-width 0.15s', opacity: active == null || isActive ? 1 : 0.4, cursor: 'pointer' }}
              onMouseEnter={() => { setActive(s.i); onHover && onHover(s); }}
              onMouseLeave={() => { setActive(null); onHover && onHover(null); }}
            />
          );
        })}
      </svg>
      <div style={{ position: 'absolute', inset: 0, display: 'grid', placeItems: 'center', textAlign: 'center', pointerEvents: 'none' }}>
        {active != null ? (
          <div>
            <div className="t-xs t-muted" style={{ marginBottom: 2 }}>{segs[active].label}</div>
            <div className="t-h2 num">{(segs[active].value / total * 100).toFixed(1).replace('.', ',')}%</div>
          </div>
        ) : center}
      </div>
    </div>
  );
}

/* ---------------- Area / line history ---------------- */
function AreaChart({ data, height = 240, formatY, formatTip }) {
  const wrapRef = cUseRef(null);
  const [w, setW] = cUseState(640);
  const [hover, setHover] = cUseState(null);
  cUseEffect(() => {
    if (!wrapRef.current) return;
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width));
    ro.observe(wrapRef.current);
    setW(wrapRef.current.clientWidth);
    return () => ro.disconnect();
  }, []);
  const padL = 56, padR = 14, padT = 14, padB = 28;
  const innerW = Math.max(10, w - padL - padR);
  const innerH = height - padT - padB;
  const vals = data.map(d => d.value);
  const min = Math.min(...vals), max = Math.max(...vals);
  const lo = min - (max - min) * 0.18, hi = max + (max - min) * 0.12;
  const x = (i) => padL + (data.length === 1 ? innerW / 2 : (i / (data.length - 1)) * innerW);
  const y = (v) => padT + innerH - ((v - lo) / (hi - lo || 1)) * innerH;
  const linePath = data.map((d, i) => `${i ? 'L' : 'M'} ${x(i).toFixed(1)} ${y(d.value).toFixed(1)}`).join(' ');
  const areaPath = linePath + ` L ${x(data.length - 1).toFixed(1)} ${padT + innerH} L ${x(0).toFixed(1)} ${padT + innerH} Z`;
  const ticks = 4;
  const gridVals = Array.from({ length: ticks + 1 }, (_, i) => lo + (hi - lo) * (i / ticks));

  const onMove = (e) => {
    const rect = e.currentTarget.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    let best = 0, bd = Infinity;
    data.forEach((d, i) => { const dx = Math.abs(x(i) - mx); if (dx < bd) { bd = dx; best = i; } });
    setHover(best);
  };

  return (
    <div className="relative" ref={wrapRef} style={{ width: '100%' }}>
      <svg width={w} height={height} onMouseMove={onMove} onMouseLeave={() => setHover(null)} style={{ display: 'block' }}>
        <defs>
          <linearGradient id="areaFill" x1="0" y1="0" x2="0" y2="1">
            <stop offset="0%" stopColor="hsl(var(--primary))" stopOpacity="0.28" />
            <stop offset="100%" stopColor="hsl(var(--primary))" stopOpacity="0.01" />
          </linearGradient>
        </defs>
        {gridVals.map((gv, i) => (
          <g key={i}>
            <line x1={padL} x2={w - padR} y1={y(gv)} y2={y(gv)} stroke="hsl(var(--border))" strokeWidth="1" />
            <text x={padL - 10} y={y(gv) + 4} textAnchor="end" fontSize="10.5" fill="hsl(var(--muted-foreground))" className="num">{formatY ? formatY(gv) : Math.round(gv)}</text>
          </g>
        ))}
        <path d={areaPath} fill="url(#areaFill)" />
        <path d={linePath} fill="none" stroke="hsl(var(--primary))" strokeWidth="2.4" strokeLinejoin="round" strokeLinecap="round" />
        {data.map((d, i) => (
          <text key={i} x={x(i)} y={height - 8} textAnchor="middle" fontSize="10.5" fill="hsl(var(--muted-foreground))">{d.label}</text>
        ))}
        {hover != null && (
          <g>
            <line x1={x(hover)} x2={x(hover)} y1={padT} y2={padT + innerH} stroke="hsl(var(--primary))" strokeWidth="1" strokeDasharray="3 3" opacity="0.6" />
            <circle cx={x(hover)} cy={y(data[hover].value)} r="5" fill="hsl(var(--primary))" stroke="hsl(var(--card))" strokeWidth="2.5" />
          </g>
        )}
      </svg>
      {hover != null && (
        <div className="chart-tip num" style={{ left: Math.min(Math.max(x(hover), 70), w - 70), top: y(data[hover].value) }}>
          <div className="t-xs t-muted" style={{ marginBottom: 2 }}>{data[hover].label} 2026</div>
          <div style={{ fontWeight: 620 }}>{formatTip ? formatTip(data[hover].value) : data[hover].value}</div>
        </div>
      )}
    </div>
  );
}

/* ---------------- Drift bar (target vs actual) ---------------- */
function DriftBar({ row, scaleMax }) {
  const fill = window.PT.catColor(row.id);
  const pct = (v) => (v / scaleMax) * 100;
  const bandLo = Math.max(0, row.target - row.tol);
  const bandHi = row.target + row.tol;
  return (
    <div className="drift-track">
      <div className="drift-band" style={{ left: pct(bandLo) + '%', width: (pct(bandHi) - pct(bandLo)) + '%' }}></div>
      <div className="drift-fill" style={{ width: pct(row.actual) + '%', background: fill, opacity: row.outOfBand ? 0.55 : 0.9, boxShadow: row.outOfBand ? 'inset 0 0 0 1.5px hsl(var(--warn))' : 'none' }}></div>
      <div className="drift-target" style={{ left: pct(row.target) + '%', background: row.outOfBand ? 'hsl(var(--warn))' : 'hsl(var(--foreground))' }}></div>
    </div>
  );
}

/* tiny sparkline */
function Spark({ data, w = 80, h = 28, color = 'hsl(var(--gain))' }) {
  const min = Math.min(...data), max = Math.max(...data);
  const x = (i) => (i / (data.length - 1)) * w;
  const y = (v) => h - ((v - min) / (max - min || 1)) * h;
  const path = data.map((v, i) => `${i ? 'L' : 'M'} ${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(' ');
  return <svg width={w} height={h} style={{ display: 'block' }}><path d={path} fill="none" stroke={color} strokeWidth="1.8" strokeLinejoin="round" strokeLinecap="round" /></svg>;
}

/* ---------------- Stacked area (composition over time) ---------------- */
function StackedArea({ data, order, height = 260, formatY, colorFn }) {
  const wrapRef = cUseRef(null);
  const [w, setW] = cUseState(640);
  const [hover, setHover] = cUseState(null);
  cUseEffect(() => {
    if (!wrapRef.current) return;
    const ro = new ResizeObserver((e) => setW(e[0].contentRect.width));
    ro.observe(wrapRef.current); setW(wrapRef.current.clientWidth);
    return () => ro.disconnect();
  }, []);
  const padL = 56, padR = 14, padT = 14, padB = 28;
  const innerW = Math.max(10, w - padL - padR), innerH = height - padT - padB;
  const max = Math.max(...data.map(d => d.total)) * 1.06;
  const x = (i) => padL + (i / (data.length - 1)) * innerW;
  const y = (v) => padT + innerH - (v / max) * innerH;
  // build cumulative bands
  const bands = order.map((id, oi) => {
    const lower = data.map(d => order.slice(0, oi).reduce((s, k) => s + (d.parts.find(p => p.id === k) || { value: 0 }).value, 0));
    const upper = data.map((d, i) => lower[i] + (d.parts.find(p => p.id === id) || { value: 0 }).value);
    const top = upper.map((v, i) => `${i ? 'L' : 'M'} ${x(i).toFixed(1)} ${y(v).toFixed(1)}`).join(' ');
    const botPath = lower.slice().reverse().map((v, ri) => { const i = data.length - 1 - ri; return `L ${x(i).toFixed(1)} ${y(v).toFixed(1)}`; }).join(' ');
    return { id, d: top + ' ' + botPath + ' Z' };
  });
  const gridVals = Array.from({ length: 5 }, (_, i) => (max / 4) * i);
  const onMove = (e) => {
    const r = e.currentTarget.getBoundingClientRect(); const mx = e.clientX - r.left;
    let best = 0, bd = Infinity; data.forEach((d, i) => { const dx = Math.abs(x(i) - mx); if (dx < bd) { bd = dx; best = i; } });
    setHover(best);
  };
  return (
    <div className="relative" ref={wrapRef} style={{ width: '100%' }}>
      <svg width={w} height={height} onMouseMove={onMove} onMouseLeave={() => setHover(null)} style={{ display: 'block' }}>
        {gridVals.map((gv, i) => (
          <g key={i}>
            <line x1={padL} x2={w - padR} y1={y(gv)} y2={y(gv)} stroke="hsl(var(--border))" strokeWidth="1" />
            <text x={padL - 10} y={y(gv) + 4} textAnchor="end" fontSize="10.5" fill="hsl(var(--muted-foreground))" className="num">{formatY ? formatY(gv) : Math.round(gv)}</text>
          </g>
        ))}
        {bands.map(b => <path key={b.id} d={b.d} fill={colorFn(b.id)} fillOpacity="0.82" stroke={colorFn(b.id)} strokeWidth="0.5" />)}
        {data.map((d, i) => <text key={i} x={x(i)} y={height - 8} textAnchor="middle" fontSize="10.5" fill="hsl(var(--muted-foreground))">{d.label}</text>)}
        {hover != null && <line x1={x(hover)} x2={x(hover)} y1={padT} y2={padT + innerH} stroke="hsl(var(--foreground))" strokeWidth="1" strokeDasharray="3 3" opacity="0.4" />}
      </svg>
      {hover != null && (
        <div className="chart-tip num" style={{ left: Math.min(Math.max(x(hover), 90), w - 90), top: y(data[hover].total) - 6, minWidth: 150 }}>
          <div className="t-xs t-muted" style={{ marginBottom: 4 }}>{data[hover].label} 2026 · {window.PT.formatIDR(data[hover].total, { compact: true })}</div>
          {order.slice().reverse().map(id => { const p = data[hover].parts.find(x => x.id === id); return (
            <div key={id} className="flex items-center gap-2" style={{ fontSize: 11 }}>
              <span className="dot" style={{ width: 7, height: 7, background: colorFn(id) }}></span>
              <span className="flex-1 t-muted">{window.PT.catLabel(id)}</span>
              <span>{((p.value / data[hover].total) * 100).toFixed(0)}%</span>
            </div>
          ); })}
        </div>
      )}
    </div>
  );
}

Object.assign(window, { Donut, AreaChart, DriftBar, Spark, StackedArea });
