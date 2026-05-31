import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip, Legend } from "recharts";
import type { CategoryAllocation } from "../api/schemas";

const COLORS = ["#2563eb", "#16a34a", "#f59e0b", "#dc2626", "#7c3aed", "#0891b2", "#db2777"];

export function AllocationDonut({ allocation }: { allocation: CategoryAllocation[] }) {
  const data = allocation
    .map((c) => ({ name: c.name, value: Number(c.actual_value_idr) }))
    .filter((d) => d.value > 0);
  if (data.length === 0) return <div className="text-sm text-gray-500">No holdings to allocate.</div>;
  return (
    <div className="h-64 w-full rounded border bg-white p-2">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie data={data} dataKey="value" nameKey="name" innerRadius="55%" outerRadius="80%">
            {data.map((_, i) => (
              <Cell key={i} fill={COLORS[i % COLORS.length]} />
            ))}
          </Pie>
          <Tooltip />
          <Legend />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}
