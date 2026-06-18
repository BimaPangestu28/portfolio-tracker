/**
 * DriftBarsChart — target vs actual drift bars.
 * Matches claude-design-source/app/charts.jsx DriftBar:
 *   - drift-track (30px height, rounded)
 *   - drift-band (tolerance zone, semi-transparent)
 *   - drift-fill (actual %, colored with opacity tweak when out-of-band)
 *   - drift-target (vertical marker for target %)
 */
import type { CategoryAllocation } from "../../api/schemas";
import { UNCATEGORIZED_CATEGORY_ID } from "../../api/schemas";

function categoryColor(name: string): string {
  const key = (name ?? "").toLowerCase();
  if (key.includes("saham") || key.includes("equity") || key.includes("stock"))
    return "hsl(var(--cat-saham))";
  if (key.includes("crypto") || key.includes("kripto")) return "hsl(var(--cat-crypto))";
  if (key.includes("reksa") || key.includes("mutual") || key.includes("fund"))
    return "hsl(var(--cat-reksadana))";
  if (key.includes("kas") || key.includes("cash") || key.includes("liquid"))
    return "hsl(var(--cat-kas))";
  if (key.includes("obligasi") || key.includes("bond")) return "hsl(var(--cat-obligasi))";
  if (key.includes("emas") || key.includes("gold")) return "hsl(var(--cat-emas))";
  if (
    key.includes("properti") ||
    key.includes("real estate") ||
    key.includes("property")
  )
    return "hsl(var(--cat-properti))";
  return "hsl(var(--cat-lainnya))";
}

interface Props {
  allocation: CategoryAllocation[];
}

export function DriftBarsChart({ allocation }: Props) {
  if (allocation.length === 0) {
    return (
      <div className="flex items-center justify-center" style={{ padding: "20px 0" }}>
        <div className="t-sm t-muted">Belum ada kategori alokasi.</div>
      </div>
    );
  }

  // Scale max = max(actual, target+tol) * 1.05
  const scaleMax =
    Math.max(
      ...allocation.map((c) => {
        const tol = parseFloat(c.tolerance_band_pct ?? "5");
        return Math.max(Number(c.actual_pct), Number(c.target_pct) + tol);
      }),
    ) * 1.05 || 100;

  const pct = (v: number) => `${(v / scaleMax) * 100}%`;

  return (
    <div className="flex col gap-3">
      {allocation.map((c) => {
        const actual = Number(c.actual_pct);
        const target = Number(c.target_pct);
        const tol = parseFloat(c.tolerance_band_pct ?? "5");
        const oob = c.out_of_band;
        const color = categoryColor(c.name);
        const bandLo = Math.max(0, target - tol);
        const bandHi = target + tol;
        // The synthetic "Lainnya" bucket has no target, so the tolerance band and
        // target marker are meaningless — show just the actual fill.
        const isUncategorized = c.category_id === UNCATEGORIZED_CATEGORY_ID;

        return (
          <div key={c.category_id} className="flex col gap-1">
            <div className="flex items-center justify-between">
              <span className="t-sm flex items-center gap-2">
                <span
                  className="dot"
                  style={{ background: color }}
                  data-testid={oob ? `oob-${c.category_id}` : undefined}
                />
                {c.name}
              </span>
              <span className="t-xs num t-muted">
                {isUncategorized
                  ? `${actual.toFixed(1).replace(".", ",")}% · tanpa target`
                  : `${actual.toFixed(1).replace(".", ",")}% / ${target}%`}
              </span>
            </div>
            {/* Drift track */}
            <div className="drift-track">
              {!isUncategorized && (
                <div
                  className="drift-band"
                  style={{
                    left: pct(bandLo),
                    width: `${((bandHi - bandLo) / scaleMax) * 100}%`,
                  }}
                />
              )}
              <div
                className="drift-fill"
                style={{
                  width: pct(actual),
                  background: color,
                  opacity: oob ? 0.55 : 0.9,
                  boxShadow: oob ? "inset 0 0 0 1.5px hsl(var(--warn))" : "none",
                }}
              />
              {!isUncategorized && (
                <div
                  className="drift-target"
                  style={{
                    left: pct(target),
                    background: oob
                      ? "hsl(var(--warn))"
                      : "hsl(var(--foreground))",
                  }}
                />
              )}
            </div>
          </div>
        );
      })}
    </div>
  );
}

export { categoryColor };
