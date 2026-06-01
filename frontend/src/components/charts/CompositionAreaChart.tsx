/**
 * CompositionAreaChart — Stacked area showing portfolio composition over time.
 * Data comes from insights.composition[] (per-category value_idr per as_of date).
 * If < 2 data points → empty state.
 */
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
  Legend,
} from "recharts";
import type { Insights } from "../../api/schemas";
import { formatIDR } from "../../lib/format";
import { categoryColor } from "./AllocationDonutChart";

interface Props {
  composition: Insights["composition"];
}

interface ChartRow {
  date: string;
  [category: string]: string | number;
}

export function CompositionAreaChart({ composition }: Props) {
  if (composition.length < 2) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 py-10 text-center">
        <div className="text-2xl">📈</div>
        <p className="text-sm text-muted-foreground">
          History terkumpul harian.
          <br />
          <span className="text-xs">Cek lagi besok untuk melihat tren komposisi.</span>
        </p>
      </div>
    );
  }

  // Collect all category names in order of first appearance
  const categories: string[] = [];
  for (const point of composition) {
    for (const part of point.parts) {
      if (!categories.includes(part.category)) categories.push(part.category);
    }
  }

  // Build flat rows for Recharts
  const data: ChartRow[] = composition.map((point) => {
    const row: ChartRow = { date: point.as_of.slice(0, 10) };
    for (const cat of categories) {
      const part = point.parts.find((p) => p.category === cat);
      row[cat] = part ? Number(part.value_idr) : 0;
    }
    return row;
  });

  return (
    <div className="h-56 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <AreaChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
          <defs>
            {categories.map((cat) => (
              <linearGradient key={cat} id={`grad-${cat}`} x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor={categoryColor(cat)} stopOpacity={0.5} />
                <stop offset="100%" stopColor={categoryColor(cat)} stopOpacity={0.05} />
              </linearGradient>
            ))}
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
          <XAxis
            dataKey="date"
            fontSize={10}
            stroke="hsl(var(--muted-foreground))"
            tickLine={false}
            axisLine={false}
          />
          <YAxis
            tickFormatter={(v: number) => {
              if (v >= 1_000_000_000) return `${(v / 1_000_000_000).toFixed(1)}M`;
              if (v >= 1_000_000) return `${(v / 1_000_000).toFixed(0)}jt`;
              if (v >= 1_000) return `${(v / 1_000).toFixed(0)}rb`;
              return String(v);
            }}
            width={48}
            fontSize={10}
            stroke="hsl(var(--muted-foreground))"
            tickLine={false}
            axisLine={false}
          />
          <Tooltip
            contentStyle={{
              background: "hsl(var(--popover))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "var(--radius-sm)",
              color: "hsl(var(--popover-foreground))",
              fontSize: 12,
              padding: "6px 10px",
            }}
            formatter={(v: number, name: string) => [formatIDR(v), name]}
            labelStyle={{ fontSize: 11, color: "hsl(var(--muted-foreground))" }}
          />
          <Legend
            iconType="circle"
            iconSize={7}
            wrapperStyle={{ fontSize: 11, paddingTop: 4 }}
          />
          {categories.map((cat) => (
            <Area
              key={cat}
              type="monotone"
              dataKey={cat}
              stackId="1"
              stroke={categoryColor(cat)}
              strokeWidth={1.5}
              fill={`url(#grad-${cat})`}
              dot={false}
              activeDot={{ r: 3 }}
            />
          ))}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}
