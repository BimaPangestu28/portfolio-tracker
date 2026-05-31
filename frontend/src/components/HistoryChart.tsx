import { LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from "recharts";
import { formatIDR } from "../lib/format";
import type { Snapshot } from "../api/schemas";

export function HistoryChart({ snapshots }: { snapshots: Snapshot[] }) {
  const data = snapshots.map((s) => ({ date: s.as_of, idr: Number(s.total_idr) }));
  if (data.length === 0) return <div className="text-sm text-gray-500">No history yet — snapshots accumulate daily.</div>;
  return (
    <div className="h-64 w-full rounded border bg-white p-2">
      <ResponsiveContainer width="100%" height="100%">
        <LineChart data={data} margin={{ top: 10, right: 20, bottom: 0, left: 0 }}>
          <CartesianGrid strokeDasharray="3 3" />
          <XAxis dataKey="date" fontSize={11} />
          <YAxis tickFormatter={(v) => formatIDR(v)} width={90} fontSize={11} />
          <Tooltip formatter={(v: number) => formatIDR(v)} />
          <Line type="monotone" dataKey="idr" stroke="#2563eb" dot={false} />
        </LineChart>
      </ResponsiveContainer>
    </div>
  );
}
