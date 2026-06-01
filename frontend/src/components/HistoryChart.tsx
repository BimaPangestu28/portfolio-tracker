import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import { formatIDR } from "../lib/format";
import type { Snapshot } from "../api/schemas";

export function HistoryChart({ snapshots }: { snapshots: Snapshot[] }) {
  const data = snapshots.map((s) => ({ date: s.as_of, idr: Number(s.total_idr) }));
  if (data.length === 0)
    return <div className="text-sm text-muted-foreground">No history yet — snapshots accumulate daily.</div>;
  return (
    <div className="h-64 w-full rounded-lg border bg-card p-2">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 10, right: 20, bottom: 0, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" />
          <XAxis dataKey="date" fontSize={11} stroke="hsl(var(--muted-foreground))" />
          <YAxis tickFormatter={(v) => formatIDR(v)} width={90} fontSize={11} stroke="hsl(var(--muted-foreground))" />
          <Tooltip
            formatter={(v: number) => formatIDR(v)}
            contentStyle={{
              background: "hsl(var(--popover))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "var(--radius)",
              color: "hsl(var(--popover-foreground))",
            }}
          />
          <Line type="monotone" dataKey="idr" stroke="hsl(var(--chart-1))" dot={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
