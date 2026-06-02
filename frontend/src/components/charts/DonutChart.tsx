/**
 * DonutChart — custom SVG donut matching claude-design-source/app/charts.jsx Donut.
 * Arc segments with gap, active hover, center label.
 * Replaces AllocationDonutChart for the fidelity pass.
 */
import { useState } from "react";

interface Segment {
  id: string | number;
  label: string;
  value: number;
  color: string;
}

interface Props {
  data: Segment[];
  size?: number;
  thickness?: number;
  centerLabel?: string;
  centerValue?: string;
}

function polar(cx: number, cy: number, r: number, deg: number): [number, number] {
  const rad = (deg * Math.PI) / 180;
  return [cx + r * Math.cos(rad), cy + r * Math.sin(rad)];
}

export function DonutChart({
  data,
  size = 180,
  thickness = 26,
  centerLabel,
  centerValue,
}: Props) {
  const [active, setActive] = useState<number | null>(null);

  const total = data.reduce((s, d) => s + d.value, 0) || 1;
  const r = (size - thickness) / 2;
  const cx = size / 2;
  const cy = size / 2;
  const gap = 1.4;

  let offset = -90;
  const segs = data.map((d, i) => {
    const sweep = (d.value / total) * 360;
    const seg = { ...d, start: offset, sweep, i };
    offset += sweep;
    return seg;
  });

  if (data.length === 0) {
    return (
      <div
        style={{
          width: size,
          height: size,
          display: "grid",
          placeItems: "center",
          flexShrink: 0,
        }}
      >
        <div
          style={{
            width: size - thickness * 2,
            height: size - thickness * 2,
            borderRadius: "50%",
            background: "hsl(var(--muted))",
          }}
        />
      </div>
    );
  }

  return (
    <div style={{ position: "relative", width: size, height: size, flexShrink: 0 }}>
      <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`}>
        {segs.map((s) => {
          const a0 = s.start + gap / 2;
          const a1 = s.start + s.sweep - gap / 2;
          const [x0, y0] = polar(cx, cy, r, a0);
          const [x1, y1] = polar(cx, cy, r, a1);
          const large = a1 - a0 > 180 ? 1 : 0;
          const isActive = active === s.i;
          const sw = isActive ? thickness + 5 : thickness;
          return (
            <path
              key={s.id}
              d={`M ${x0.toFixed(2)} ${y0.toFixed(2)} A ${r} ${r} 0 ${large} 1 ${x1.toFixed(2)} ${y1.toFixed(2)}`}
              fill="none"
              stroke={s.color}
              strokeWidth={sw}
              strokeLinecap="round"
              style={{
                transition: "stroke-width 0.15s",
                opacity: active == null || isActive ? 1 : 0.4,
                cursor: "pointer",
              }}
              onMouseEnter={() => setActive(s.i)}
              onMouseLeave={() => setActive(null)}
            />
          );
        })}
      </svg>
      <div
        style={{
          position: "absolute",
          inset: 0,
          display: "grid",
          placeItems: "center",
          textAlign: "center",
          pointerEvents: "none",
        }}
      >
        {active != null ? (
          <div>
            <div className="t-xs t-muted" style={{ marginBottom: 2 }}>
              {segs[active].label}
            </div>
            <div className="t-h2 num">
              {((segs[active].value / total) * 100).toFixed(1).replace(".", ",")}%
            </div>
          </div>
        ) : (
          <div>
            <div className="t-xs t-muted">{centerLabel}</div>
            <div className="t-h3 num">{centerValue}</div>
          </div>
        )}
      </div>
    </div>
  );
}
