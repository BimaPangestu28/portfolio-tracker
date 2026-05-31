import { useSummary, useInstruments } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, formatUSD, formatPct } from "../lib/format";

export default function HoldingsPage() {
  const summary = useSummary();
  const instruments = useInstruments();
  const nameOf = (id: number) => instruments.data?.find((i) => i.id === id)?.symbol ?? `#${id}`;

  return (
    <div className="space-y-4">
      <h1 className="text-xl font-semibold">Holdings</h1>
      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <div className="overflow-x-auto rounded border bg-white">
          <table className="w-full text-sm">
            <thead className="bg-gray-50 text-left text-xs uppercase text-gray-500">
              <tr>
                <th className="p-2">Instrument</th>
                <th className="p-2">Qty</th>
                <th className="p-2">Avg cost</th>
                <th className="p-2">Price</th>
                <th className="p-2">Value (IDR)</th>
                <th className="p-2">Unrealized</th>
              </tr>
            </thead>
            <tbody>
              {(summary.data?.positions ?? []).map((p) => (
                <tr key={p.instrument_id} className="border-t">
                  <td className="p-2 font-medium">
                    {nameOf(p.instrument_id)}
                    {p.price_stale && <span className="ml-1 text-xs text-amber-600" title="Price may be outdated">⚠ stale</span>}
                  </td>
                  <td className="p-2">{p.quantity}</td>
                  <td className="p-2">{formatUSD(p.avg_cost)}</td>
                  <td className="p-2">{formatUSD(p.latest_price)}</td>
                  <td className="p-2">{formatIDR(p.market_value_idr)}</td>
                  <td className={`p-2 ${Number(p.unrealized_pnl) >= 0 ? "text-green-600" : "text-red-600"}`}>
                    {formatUSD(p.unrealized_pnl)} ({formatPct(((Number(p.unrealized_pnl) / (Number(p.cost_basis_total) || 1)) * 100).toString())})
                  </td>
                </tr>
              ))}
              {(summary.data?.positions ?? []).length === 0 && (
                <tr><td className="p-3 text-gray-500" colSpan={6}>No positions yet. Add transactions to see holdings.</td></tr>
              )}
            </tbody>
          </table>
        </div>
      </QueryState>
    </div>
  );
}
