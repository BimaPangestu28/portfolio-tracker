/**
 * NetWorthSparkline — a compact 12-point area line used in the Hero section.
 * Uses the last N snapshots from useHistory, renders a smooth area with no axes.
 */
import { AreaChart, Area, ResponsiveContainer, Tooltip } from "recharts";
import type { Snapshot } from "../../api/schemas";
import { formatIDR } from "../../lib/format";

interface Props {
  snapshots: Snapshot[];
  /** How many trailing data points to use (default 12) */
  points?: number;
}

export function NetWorthSparkline({ snapshots, points = 12 }: Props) {
  const data = snapshots
    .slice(-points)
    .map((s) => ({ date: s.as_of.slice(0, 10), idr: Number(s.total_idr) }));

  if (data.length < 2) return null;

  return (
    <div className="h-16 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 2, right: 2, left: 2, bottom: 2 }}>
          <defs>
            <linearGradient id="sparkGrad" x1="0" y1="0" x2="0" y2="1">
              <stop offset="0%" stopColor="hsl(var(--primary))" stopOpacity={0.3} />
              <stop offset="100%" stopColor="hsl(var(--primary))" stopOpacity={0} />
            </linearGradient>
          </defs>
          <Tooltip
            contentStyle={{
              background: "hsl(var(--popover))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "var(--radius-sm)",
              color: "hsl(var(--popover-foreground))",
              fontSize: 11,
              padding: "4px 8px",
            }}
            formatter={(v: number) => [formatIDR(v), "Net Worth"]}
            labelStyle={{ fontSize: 10, color: "hsl(var(--muted-foreground))" }}
          />
          <Area
            type="monotone"
            dataKey="idr"
            stroke="hsl(var(--primary))"
            strokeWidth={1.5}
            fill="url(#sparkGrad)"
            dot={false}
            activeDot={{ r: 3, fill: "hsl(var(--primary))" }}
          />
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
