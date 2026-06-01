import { formatIDR, formatPct } from "../lib/format";
import type { CategoryAllocation } from "../api/schemas";

export function DriftBars({ allocation }: { allocation: CategoryAllocation[] }) {
  if (allocation.length === 0) return <div className="text-sm text-muted-foreground">No categories yet.</div>;
  return (
    <div className="space-y-3">
      {allocation.map((c) => {
        const actual = Number(c.actual_pct);
        const target = Number(c.target_pct);
        const reb = Number(c.rebalance_idr);
        return (
          <div key={c.category_id} className="rounded-lg border bg-card p-3">
            <div className="flex items-center justify-between text-sm">
              <span className="font-medium">
                {c.name}
                {c.out_of_band && (
                  <span
                    data-testid={`oob-${c.category_id}`}
                    className="ml-2 rounded bg-destructive/10 px-1.5 py-0.5 text-xs text-destructive"
                  >
                    out of band
                  </span>
                )}
              </span>
              <span className="text-muted-foreground">
                {formatPct(c.actual_pct)} / target {formatPct(c.target_pct)} (drift {formatPct(c.drift_pct)})
              </span>
            </div>
            <div className="mt-2 h-2 w-full rounded bg-muted">
              <div
                className={`h-2 rounded ${c.out_of_band ? "bg-destructive" : "bg-primary"}`}
                style={{ width: `${Math.min(100, Math.max(0, actual))}%` }}
              />
            </div>
            <div className="mt-1 text-xs text-muted-foreground">
              {reb > 0
                ? `Buy ${formatIDR(c.rebalance_idr)} to reach target`
                : reb < 0
                  ? `Trim ${formatIDR(Math.abs(reb))} to reach target`
                  : "On target"}
              {` · target marker at ${target.toFixed(0)}%`}
            </div>
          </div>
        );
      })}
    </div>
  );
}
