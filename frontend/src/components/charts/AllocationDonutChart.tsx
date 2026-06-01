/**
 * AllocationDonutChart — Recharts donut using the categorical CSS palette.
 * Renders actual_value_idr slices per CategoryAllocation.
 */
import { PieChart, Pie, Cell, ResponsiveContainer, Tooltip, Legend } from "recharts";
import type { CategoryAllocation } from "../../api/schemas";
import { formatIDR, formatPct } from "../../lib/format";

/** Maps a category name (lowercased) to its CSS var hue */
function categoryColor(name: string): string {
  const key = name.toLowerCase();
  if (key.includes("saham") || key.includes("equity") || key.includes("stock")) return "hsl(var(--cat-saham))";
  if (key.includes("crypto") || key.includes("kripto")) return "hsl(var(--cat-crypto))";
  if (key.includes("reksa") || key.includes("mutual") || key.includes("fund")) return "hsl(var(--cat-reksadana))";
  if (key.includes("kas") || key.includes("cash") || key.includes("liquid")) return "hsl(var(--cat-kas))";
  if (key.includes("obligasi") || key.includes("bond")) return "hsl(var(--cat-obligasi))";
  if (key.includes("emas") || key.includes("gold")) return "hsl(var(--cat-emas))";
  if (key.includes("properti") || key.includes("real estate") || key.includes("property")) return "hsl(var(--cat-properti))";
  return "hsl(var(--cat-lainnya))";
}

interface Props {
  allocation: CategoryAllocation[];
}

interface SliceEntry {
  name: string;
  value: number;
  color: string;
}

export function AllocationDonutChart({ allocation }: Props) {
  const data: SliceEntry[] = allocation
    .map((c) => ({ name: c.name, value: Number(c.actual_value_idr), color: categoryColor(c.name) }))
    .filter((d) => d.value > 0);

  if (data.length === 0) {
    return (
      <div className="flex h-48 flex-col items-center justify-center gap-2 text-center">
        <div className="text-2xl">📊</div>
        <p className="text-sm text-muted-foreground">Belum ada alokasi.<br />Tambah kategori di halaman Planner.</p>
      </div>
    );
  }

  return (
    <div className="h-56 w-full">
      <ResponsiveContainer width="100%" height="100%">
        <PieChart>
          <Pie
            data={data}
            dataKey="value"
            nameKey="name"
            cx="50%"
            cy="50%"
            innerRadius="52%"
            outerRadius="76%"
            strokeWidth={2}
            stroke="hsl(var(--card))"
          >
            {data.map((entry, i) => (
              <Cell key={i} fill={entry.color} />
            ))}
          </Pie>
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
          />
          <Legend
            iconType="circle"
            iconSize={8}
            wrapperStyle={{ fontSize: 12, paddingTop: 4 }}
          />
        </PieChart>
      </ResponsiveContainer>
    </div>
  );
}

export { categoryColor };
export type { SliceEntry };

/**
 * AllocationDriftBars — target vs actual horizontal bars with rebalance hints.
 * Amber when out_of_band.
 */
export function AllocationDriftBars({ allocation }: Props) {
  if (allocation.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center gap-2 py-8 text-center">
        <p className="text-sm text-muted-foreground">Belum ada kategori alokasi.</p>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {allocation.map((c) => {
        const actual = Math.max(0, Math.min(100, Number(c.actual_pct)));
        const target = Math.max(0, Math.min(100, Number(c.target_pct)));
        const reb = Number(c.rebalance_idr);
        const color = categoryColor(c.name);
        const oob = c.out_of_band;

        return (
          <div key={c.category_id}>
            <div className="mb-1 flex items-center justify-between text-xs">
              <div className="flex items-center gap-1.5">
                <span
                  className="inline-block h-2 w-2 rounded-full flex-shrink-0"
                  style={{ background: color }}
                />
                <span className="font-medium text-foreground">{c.name}</span>
                {oob && (
                  <span
                    data-testid={`oob-${c.category_id}`}
                    className="rounded-full bg-warn-soft px-1.5 py-0.5 text-[10px] font-semibold text-warn"
                  >
                    drift
                  </span>
                )}
              </div>
              <span className="num text-muted-foreground">
                {formatPct(c.actual_pct)} / {formatPct(c.target_pct)}
              </span>
            </div>

            {/* Bar track */}
            <div className="relative h-1.5 w-full rounded-full bg-muted">
              {/* Actual fill */}
              <div
                className="h-1.5 rounded-full transition-all"
                style={{
                  width: `${actual}%`,
                  background: oob ? "hsl(var(--warn))" : color,
                }}
              />
              {/* Target marker */}
              <div
                className="absolute top-1/2 h-3 w-0.5 -translate-y-1/2 rounded-full bg-muted-foreground/50"
                style={{ left: `${target}%` }}
              />
            </div>

            {/* Rebalance hint */}
            <div className="mt-0.5 text-[10px] text-muted-foreground">
              {reb > 0
                ? <span className="text-gain">Beli {formatIDR(String(reb))}</span>
                : reb < 0
                  ? <span className="text-loss">Pangkas {formatIDR(String(Math.abs(reb)))}</span>
                  : <span>On target</span>}
              <span className="text-muted-foreground/60"> · target {formatPct(c.target_pct)}</span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
