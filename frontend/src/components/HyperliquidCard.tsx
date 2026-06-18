import { Area, AreaChart, ResponsiveContainer, Tooltip, XAxis, YAxis } from "recharts";
import { useHyperliquid } from "@/api/hooks";

export function HyperliquidCard() {
  const q = useHyperliquid();
  const view = q.data;
  const chartData = (view?.points ?? []).map((p) => ({ date: p.date, returnPct: p.cum_return * 100 }));
  const totalPct = view ? view.metrics.total_return * 100 : 0;

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Hyperliquid</div>
          <div className="card-sub">equity &amp; return</div>
        </div>
      </div>
      <div className="card-pad flex col" style={{ paddingTop: 6 }}>
        {q.isLoading ? (
          <div className="skeleton" style={{ width: "100%", height: 120 }} />
        ) : !view || view.insufficient_data ? (
          <div className="empty">
            <div className="t-h3">Belum ada data equity</div>
            <div className="t-sm t-muted">Kurva muncul setelah ada dua hari data equity.</div>
          </div>
        ) : (
          <>
            <div className="flex items-baseline gap-3">
              <span className="num t-h2">${view.current_value_usd}</span>
              <span className={totalPct >= 0 ? "gain" : "loss"}>
                {totalPct >= 0 ? "▲" : "▼"} {Math.abs(totalPct).toFixed(2)}%
              </span>
            </div>
            <div style={{ width: "100%", height: 140 }}>
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 10, right: 8, left: 0, bottom: 0 }}>
                  <XAxis dataKey="date" fontSize={10} tickLine={false} axisLine={false}
                    stroke="hsl(var(--muted-foreground))" minTickGap={28} />
                  <YAxis tickFormatter={(v: number) => `${v.toFixed(0)}%`} width={40} fontSize={10}
                    tickLine={false} axisLine={false} stroke="hsl(var(--muted-foreground))" />
                  <Tooltip formatter={(v: number) => `${v.toFixed(2)}%`}
                    contentStyle={{ background: "hsl(var(--popover))", border: "1px solid hsl(var(--border))",
                      borderRadius: "var(--radius)", color: "hsl(var(--popover-foreground))", fontSize: 12 }} />
                  <Area type="monotone" dataKey="returnPct" stroke="hsl(var(--chart-1))" strokeWidth={1.5}
                    fill="hsl(var(--chart-1))" fillOpacity={0.15} dot={false} />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
