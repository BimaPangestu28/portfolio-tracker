import { useState } from "react";
import { useCategories, useCreateCategory, useDeleteCategory, useSummary } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatPct } from "../lib/format";

export default function PlannerPage() {
  const cats = useCategories();
  const summary = useSummary();
  const create = useCreateCategory();
  const del = useDeleteCategory();
  const [form, setForm] = useState({ name: "", target_pct: "", tolerance_band_pct: "" });

  const totalTarget = (cats.data ?? []).reduce((acc, c) => acc + Number(c.target_pct), 0);

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate({
      name: form.name,
      target_pct: form.target_pct,
      tolerance_band_pct: form.tolerance_band_pct || null,
      color: null,
    });
    setForm({ name: "", target_pct: "", tolerance_band_pct: "" });
  };
  const input = "rounded border px-2 py-1 text-sm";

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Allocation Planner</h1>

      <form onSubmit={submit} className="grid grid-cols-2 gap-2 rounded border bg-white p-4 sm:grid-cols-4">
        <input className={input} placeholder="Category name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} required />
        <input className={input} placeholder="Target %" value={form.target_pct} onChange={(e) => setForm({ ...form, target_pct: e.target.value })} required />
        <input className={input} placeholder="Tolerance band % (optional)" value={form.tolerance_band_pct} onChange={(e) => setForm({ ...form, tolerance_band_pct: e.target.value })} />
        <button className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50" disabled={create.isPending}>Add category</button>
        {create.error && <div className="col-span-2 text-sm text-red-600 sm:col-span-4">{(create.error as Error).message}</div>}
      </form>

      <div className={`text-sm ${Math.abs(totalTarget - 100) > 0.01 ? "text-amber-600" : "text-gray-500"}`}>
        Total target: {totalTarget.toFixed(1)}% {Math.abs(totalTarget - 100) > 0.01 ? "(should sum to 100%)" : "✓"}
      </div>

      <QueryState isLoading={cats.isLoading} error={cats.error}>
        <div className="space-y-2">
          {(cats.data ?? []).map((c) => {
            const a = summary.data?.allocation.find((x) => x.category_id === c.id);
            return (
              <div key={c.id} className="flex items-center justify-between rounded border bg-white p-3 text-sm">
                <div>
                  <span className="font-medium">{c.name}</span>
                  <span className="ml-2 text-gray-500">target {formatPct(c.target_pct)}{c.tolerance_band_pct ? ` ±${c.tolerance_band_pct}%` : ""}</span>
                  {a && <span className={`ml-2 ${a.out_of_band ? "text-red-600" : "text-gray-600"}`}>actual {formatPct(a.actual_pct)}</span>}
                </div>
                <button onClick={() => del.mutate(c.id)} className="text-xs text-red-600 hover:underline">delete</button>
              </div>
            );
          })}
          {(cats.data ?? []).length === 0 && <div className="text-gray-500">No categories yet.</div>}
        </div>
      </QueryState>
    </div>
  );
}
