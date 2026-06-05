import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip, Legend } from "recharts";
import type { CategoryAllocation } from "../api/schemas";
import { formatIDR } from "../lib/format";

const COLORS = [
  "hsl(var(--chart-1))",
  "hsl(var(--chart-2))",
  "hsl(var(--chart-3))",
  "hsl(var(--chart-4))",
  "hsl(var(--chart-5))",
];

export function AllocationDonut({ allocation }: { allocation: CategoryAllocation[] }) {
  const data = allocation
    .map((c) => ({ name: c.name, value: Number(c.actual_value_idr) }))
    .filter((d) => d.value > 0);
  if (data.length === 0) return <div className="text-sm text-muted-foreground">No holdings to allocate.</div>;
  return (
    <div className="h-64 w-full rounded-lg border bg-card p-2">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie data={data} dataKey="value" nameKey="name" innerRadius="55%" outerRadius="80%">
            {data.map((_, i) => (
              <Cell key={i} fill={COLORS[i % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip
            formatter={(value: number) => formatIDR(value)}
            contentStyle={{
              background: "hsl(var(--popover))",
              border: "1px solid hsl(var(--border))",
              borderRadius: "var(--radius)",
              color: "hsl(var(--popover-foreground))",
            }}
          />
          <Legend />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}
