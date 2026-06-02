/**
 * StackedAreaChart — stacked area composition over time.
 * Matches claude-design-source/app/charts.jsx StackedArea:
 *   - SVG hand-built, ResizeObserver for responsive width
 *   - padL=56, padR=14, padT=14, padB=28
 *   - fillOpacity=0.82, solid color bands
 *   - hover crosshair + tooltip
 *   - color legend below chart
 */
import { useState, useEffect, useRef } from "react";
import { formatIDR } from "../../lib/format";

function categoryColor(name: string): string {
  const key = (name ?? "").toLowerCase();
  if (key.includes("saham") || key.includes("equity") || key.includes("stock"))
    return "hsl(var(--cat-saham))";
  if (key.includes("crypto") || key.includes("kripto")) return "hsl(var(--cat-crypto))";
  if (key.includes("reksa") || key.includes("mutual") || key.includes("fund"))
    return "hsl(var(--cat-reksadana))";
  if (key.includes("kas") || key.includes("cash") || key.includes("liquid"))
    return "hsl(var(--cat-kas))";
  if (key.includes("obligasi") || key.includes("bond")) return "hsl(var(--cat-obligasi))";
  if (key.includes("emas") || key.includes("gold")) return "hsl(var(--cat-emas))";
  if (
    key.includes("properti") ||
    key.includes("real estate") ||
    key.includes("property")
  )
    return "hsl(var(--cat-properti))";
  return "hsl(var(--cat-lainnya))";
}

interface CompositionPoint {
  as_of: string;
  parts: Array<{ category: string; value_idr: string }>;
}

interface Props {
  composition: CompositionPoint[];
  height?: number;
}

export function StackedAreaChart({ composition, height = 260 }: Props) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const [w, setW] = useState(640);
  const [hover, setHover] = useState<number | null>(null);

  useEffect(() => {
    if (!wrapRef.current) return;
    const ro = new ResizeObserver((entries) => {
      if (entries[0]) setW(entries[0].contentRect.width);
    });
    ro.observe(wrapRef.current);
    setW(wrapRef.current.clientWidth);
    return () => ro.disconnect();
  }, []);

  if (composition.length < 2) {
    return (
      <div className="empty">
        <div className="empty-icon" style={{ fontSize: "24px" }}>📈</div>
        <div>
          <div className="t-h3">History terkumpul harian</div>
          <div className="t-sm t-muted" style={{ marginTop: 4 }}>
            Cek lagi besok untuk melihat tren komposisi.
          </div>
        </div>
      </div>
    );
  }

  // Collect ordered categories
  const categories: string[] = [];
  for (const point of composition) {
    for (const part of point.parts) {
      if (!categories.includes(part.category)) categories.push(part.category);
    }
  }

  // Build data with totals
  const data = composition.map((p) => {
    const byCategory: Record<string, number> = {};
    for (const part of p.parts) {
      byCategory[part.category] = Number(part.value_idr);
    }
    const total = categories.reduce((s, k) => s + (byCategory[k] ?? 0), 0);
    return {
      label: p.as_of.slice(0, 7), // "YYYY-MM"
      total,
      byCategory,
    };
  });

  const padL = 56, padR = 14, padT = 14, padB = 28;
  const innerW = Math.max(10, w - padL - padR);
  const innerH = height - padT - padB;
  const maxTotal = Math.max(...data.map((d) => d.total)) * 1.06 || 1;

  const xOf = (i: number) =>
    padL + (i / (data.length - 1)) * innerW;
  const yOf = (v: number) =>
    padT + innerH - (v / maxTotal) * innerH;

  // Build cumulative stacked bands
  const bands = categories.map((id, oi) => {
    const lowerArr = data.map((d) =>
      categories.slice(0, oi).reduce((s, k) => s + (d.byCategory[k] ?? 0), 0),
    );
    const upperArr = data.map(
      (d, i) => lowerArr[i] + (d.byCategory[id] ?? 0),
    );
    const top = upperArr
      .map((v, i) => `${i ? "L" : "M"} ${xOf(i).toFixed(1)} ${yOf(v).toFixed(1)}`)
      .join(" ");
    const bot = lowerArr
      .slice()
      .reverse()
      .map((v, ri) => {
        const i = data.length - 1 - ri;
        return `L ${xOf(i).toFixed(1)} ${yOf(v).toFixed(1)}`;
      })
      .join(" ");
    return { id, d: `${top} ${bot} Z`, upperArr };
  });

  // Grid vals (5 lines)
  const gridVals = Array.from({ length: 5 }, (_, i) => (maxTotal / 4) * i);

  function formatY(v: number): string {
    if (v >= 1_000_000_000) return `${(v / 1_000_000_000).toFixed(1)}M`;
    if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(0)}jt`;
    if (v >= 1_000) return `${(v / 1_000).toFixed(0)}rb`;
    return String(Math.round(v));
  }

  function onMove(e: React.MouseEvent<SVGSVGElement>) {
    const rect = e.currentTarget.getBoundingClientRect();
    const mx = e.clientX - rect.left;
    let best = 0;
    let bd = Infinity;
    data.forEach((_, i) => {
      const dx = Math.abs(xOf(i) - mx);
      if (dx < bd) {
        bd = dx;
        best = i;
      }
    });
    setHover(best);
  }

  const tipX =
    hover != null
      ? Math.min(Math.max(xOf(hover), 90), w - 90)
      : 0;
  const tipY =
    hover != null ? yOf(data[hover].total) - 6 : 0;

  return (
    <div>
      <div className="relative" ref={wrapRef} style={{ width: "100%" }}>
        <svg
          width={w}
          height={height}
          onMouseMove={onMove}
          onMouseLeave={() => setHover(null)}
          style={{ display: "block" }}
        >
          {/* Grid lines + y-axis labels */}
          {gridVals.map((gv, i) => (
            <g key={i}>
              <line
                x1={padL}
                x2={w - padR}
                y1={yOf(gv)}
                y2={yOf(gv)}
                stroke="hsl(var(--border))"
                strokeWidth="1"
              />
              <text
                x={padL - 10}
                y={yOf(gv) + 4}
                textAnchor="end"
                fontSize="10.5"
                fill="hsl(var(--muted-foreground))"
                className="num"
              >
                {formatY(gv)}
              </text>
            </g>
          ))}

          {/* Stacked area bands */}
          {bands.map((b) => (
            <path
              key={b.id}
              d={b.d}
              fill={categoryColor(b.id)}
              fillOpacity="0.82"
              stroke={categoryColor(b.id)}
              strokeWidth="0.5"
            />
          ))}

          {/* X-axis labels */}
          {data.map((d, i) => (
            <text
              key={i}
              x={xOf(i)}
              y={height - 8}
              textAnchor="middle"
              fontSize="10.5"
              fill="hsl(var(--muted-foreground))"
            >
              {d.label}
            </text>
          ))}

          {/* Hover crosshair */}
          {hover != null && (
            <line
              x1={xOf(hover)}
              x2={xOf(hover)}
              y1={padT}
              y2={padT + innerH}
              stroke="hsl(var(--foreground))"
              strokeWidth="1"
              strokeDasharray="3 3"
              opacity="0.4"
            />
          )}
        </svg>

        {/* Hover tooltip */}
        {hover != null && (
          <div
            className="chart-tip num"
            style={{ left: tipX, top: tipY, minWidth: 150 }}
          >
            <div className="t-xs t-muted" style={{ marginBottom: 4 }}>
              {data[hover].label} · {formatIDR(String(data[hover].total))}
            </div>
            {categories
              .slice()
              .reverse()
              .map((id) => {
                const val = data[hover].byCategory[id] ?? 0;
                const pct =
                  data[hover].total > 0
                    ? ((val / data[hover].total) * 100).toFixed(0)
                    : "0";
                return (
                  <div key={id} className="flex items-center gap-2" style={{ fontSize: 11 }}>
                    <span
                      className="dot"
                      style={{
                        width: 7,
                        height: 7,
                        background: categoryColor(id),
                      }}
                    />
                    <span className="flex-1 t-muted">{id}</span>
                    <span>{pct}%</span>
                  </div>
                );
              })}
          </div>
        )}
      </div>

      {/* Legend */}
      <div
        className="flex gap-4 flex-wrap"
        style={{ marginTop: 10, paddingLeft: 56 }}
      >
        {categories
          .slice()
          .reverse()
          .map((id) => (
            <span
              key={id}
              className="flex items-center gap-2 t-xs t-muted"
            >
              <span
                className="dot"
                style={{
                  width: 8,
                  height: 8,
                  background: categoryColor(id),
                }}
              />
              {id}
            </span>
          ))}
      </div>
    </div>
  );
}
