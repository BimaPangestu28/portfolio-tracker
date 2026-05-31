export function StatCard({ label, value, sub, tone }: { label: string; value: string; sub?: string; tone?: "pos" | "neg" | "neutral" }) {
  const color = tone === "pos" ? "text-green-600" : tone === "neg" ? "text-red-600" : "text-gray-900";
  return (
    <div className="rounded-lg border bg-white p-4">
      <div className="text-xs uppercase tracking-wide text-gray-500">{label}</div>
      <div className={`mt-1 text-2xl font-semibold ${color}`}>{value}</div>
      {sub && <div className="mt-1 text-sm text-gray-500">{sub}</div>}
    </div>
  );
}
