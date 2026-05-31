import { formatIDR, formatUSD } from "../lib/format";
import { StatCard } from "./StatCard";
import type { PortfolioSummary } from "../api/schemas";

export function NetWorthCard({ s }: { s: PortfolioSummary }) {
  return (
    <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
      <StatCard label="Net Worth (IDR)" value={formatIDR(s.net_worth_idr)} />
      <StatCard label="Net Worth (USD)" value={formatUSD(s.net_worth_usd)} />
    </div>
  );
}
