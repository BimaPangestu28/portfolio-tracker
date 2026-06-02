/**
 * HeroSparkline — inline SVG sparkline for the hero net-worth card.
 * Matches claude-design-source/app/page_dashboard.jsx hero sparkline:
 *   viewBox="0 0 100 30", gradient fill (gain color), stroke line.
 */
import type { Snapshot } from "../../api/schemas";

interface Props {
  snapshots: Snapshot[];
  points?: number;
}

export function HeroSparkline({ snapshots, points = 30 }: Props) {
  const data = snapshots
    .slice(-points)
    .map((s) => Number(s.total_idr))
    .filter((v) => isFinite(v));

  if (data.length < 2) return null;

  const hmin = Math.min(...data);
  const hmax = Math.max(...data);
  const range = hmax - hmin || 1;

  // Map to viewBox: x 0..100, y 0..30 (top=0, bottom=30)
  const pts = data.map((v, i) => ({
    x: (i / (data.length - 1)) * 100,
    y: 28 - ((v - hmin) / range) * 26, // leave 2px top/bottom padding
  }));

  const sLine = pts
    .map((p, i) => `${i ? "L" : "M"} ${p.x.toFixed(2)} ${p.y.toFixed(2)}`)
    .join(" ");
  const sArea = sLine + " L 100 30 L 0 30 Z";

  return (
    <svg
      viewBox="0 0 100 30"
      preserveAspectRatio="none"
      style={{ width: "100%", height: 46, display: "block", overflow: "visible" }}
      aria-hidden="true"
    >
      <defs>
        <linearGradient id="heroSpark" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="hsl(var(--gain))" stopOpacity="0.22" />
          <stop offset="100%" stopColor="hsl(var(--gain))" stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={sArea} fill="url(#heroSpark)" />
      <path
        d={sLine}
        fill="none"
        stroke="hsl(var(--gain))"
        strokeWidth="1.6"
        vectorEffect="non-scaling-stroke"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
    </svg>
  );
}
