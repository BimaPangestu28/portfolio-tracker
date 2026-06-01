import { useState } from "react";
import { Trash2 } from "lucide-react";
import { useCategories, useCreateCategory, useDeleteCategory, useSummary } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatPct } from "../lib/format";
import { Button } from "@/components/ui/button";
import { Card, CardContent } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";

export default function PlannerPage() {
  const cats = useCategories();
  const summary = useSummary();
  const create = useCreateCategory();
  const del = useDeleteCategory();
  const [form, setForm] = useState({ name: "", target_pct: "", tolerance_band_pct: "" });

  const totalTarget = (cats.data ?? []).reduce((acc, c) => acc + Number(c.target_pct), 0);
  const offTarget = Math.abs(totalTarget - 100) > 0.01;

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

  return (
    <div className="space-y-6">
      <h1 className="text-xl font-semibold">Allocation Planner</h1>

      <Card>
        <CardContent className="pt-6">
          <form onSubmit={submit} className="grid grid-cols-2 gap-3 sm:grid-cols-4">
            <div className="space-y-1">
              <Label>Category name</Label>
              <Input aria-label="Category name" placeholder="Category name" value={form.name} onChange={(e) => setForm({ ...form, name: e.target.value })} required />
            </div>
            <div className="space-y-1">
              <Label>Target %</Label>
              <Input aria-label="Target percent" placeholder="Target %" value={form.target_pct} onChange={(e) => setForm({ ...form, target_pct: e.target.value })} required />
            </div>
            <div className="space-y-1">
              <Label>Tolerance band %</Label>
              <Input aria-label="Tolerance band percent" placeholder="Tolerance band % (optional)" value={form.tolerance_band_pct} onChange={(e) => setForm({ ...form, tolerance_band_pct: e.target.value })} />
            </div>
            <div className="flex items-end">
              <Button type="submit" className="w-full" disabled={create.isPending}>
                Add category
              </Button>
            </div>
            {create.error && (
              <div className="col-span-2 text-sm text-destructive sm:col-span-4">{(create.error as Error).message}</div>
            )}
          </form>
        </CardContent>
      </Card>

      <div className={cn("text-sm", offTarget ? "text-amber-600 dark:text-amber-400" : "text-muted-foreground")}>
        Total target: {totalTarget.toFixed(1)}% {offTarget ? "(should sum to 100%)" : "✓"}
      </div>

      <QueryState isLoading={cats.isLoading} error={cats.error}>
        <div className="space-y-2">
          {(cats.data ?? []).map((c) => {
            const a = summary.data?.allocation.find((x) => x.category_id === c.id);
            return (
              <Card key={c.id}>
                <CardContent className="flex items-center justify-between py-3 text-sm">
                  <div>
                    <span className="font-medium">{c.name}</span>
                    <span className="ml-2 text-muted-foreground">
                      target {formatPct(c.target_pct)}
                      {c.tolerance_band_pct ? ` ±${c.tolerance_band_pct}%` : ""}
                    </span>
                    {a && (
                      <span className={cn("ml-2", a.out_of_band ? "text-destructive" : "text-muted-foreground")}>
                        actual {formatPct(a.actual_pct)}
                      </span>
                    )}
                  </div>
                  <Button type="button" variant="ghost" size="icon" aria-label="delete" onClick={() => del.mutate(c.id)} className="text-destructive hover:text-destructive">
                    <Trash2 className="h-4 w-4" />
                  </Button>
                </CardContent>
              </Card>
            );
          })}
          {(cats.data ?? []).length === 0 && <div className="text-muted-foreground">No categories yet.</div>}
        </div>
      </QueryState>
    </div>
  );
}
