import { formatIDR } from "../lib/format";
import { StatCard } from "./StatCard";
import type { PortfolioSummary } from "../api/schemas";

function pnlTone(v: string): "pos" | "neg" | "neutral" {
  const n = Number(v);
  if (n > 0) return "pos";
  if (n < 0) return "neg";
  return "neutral";
}

export function PerformanceCards({ s }: { s: PortfolioSummary }) {
  const xirr = s.xirr == null ? "—" : `${(s.xirr * 100).toFixed(1)}%`;
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
      <StatCard label="XIRR (annualized)" value={xirr} tone={s.xirr != null && s.xirr >= 0 ? "pos" : s.xirr != null ? "neg" : "neutral"} />
      <StatCard label="Unrealized P&L (IDR)" value={formatIDR(s.total_unrealized_pnl_idr)} tone={pnlTone(s.total_unrealized_pnl_idr)} />
      <StatCard label="Realized P&L (IDR)" value={formatIDR(s.total_realized_pnl_idr)} tone={pnlTone(s.total_realized_pnl_idr)} />
    </div>
  );
}
