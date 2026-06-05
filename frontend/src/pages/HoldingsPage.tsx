import { useState } from "react";
import { ArrowUp, ArrowDown, Clock, Filter, Plus } from "lucide-react";
import { useSummary, useInstruments } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { AddTransactionDialog } from "../components/AddTransactionDialog";
import { formatIDR, formatCurrency, formatPct, formatQty, parseNum } from "../lib/format";

type SortKey = "instrument_id" | "quantity" | "avg_cost" | "latest_price" | "market_value_idr" | "unrealized_pnl";
type SortDir = "asc" | "desc";

export default function HoldingsPage() {
  const summary = useSummary();
  const instruments = useInstruments();
  const [sort, setSort] = useState<{ key: SortKey; dir: SortDir }>({ key: "market_value_idr", dir: "desc" });
  const [addOpen, setAddOpen] = useState(false);

  const instrumentOf = (id: number) => instruments.data?.find((i) => i.id === id);
  const nameOf = (id: number) => instrumentOf(id)?.symbol ?? `#${id}`;
  // avg_cost, latest_price and unrealized_pnl are computed by the backend in the
  // instrument's native currency (see domain/valuation.rs + domain/cost_basis.rs),
  // so they must be formatted with that currency — not hardcoded to USD. Default to
  // IDR while instruments are still loading, matching the rest of the IDR-first UI.
  const currencyOf = (id: number) => instrumentOf(id)?.native_currency ?? "IDR";
  const positions = summary.data?.positions ?? [];

  const sorted = [...positions].sort((a, b) => {
    let r: number;
    if (sort.key === "instrument_id") {
      r = nameOf(a.instrument_id).localeCompare(nameOf(b.instrument_id));
    } else {
      r = parseNum(a[sort.key]) - parseNum(b[sort.key]);
    }
    return sort.dir === "asc" ? r : -r;
  });

  const toggleSort = (key: SortKey) =>
    setSort((s) => ({ key, dir: s.key === key && s.dir === "desc" ? "asc" : "desc" }));

  const th = (key: SortKey, label: string, right?: boolean) => (
    <th
      className={"sortable" + (right ? " r" : "")}
      onClick={() => toggleSort(key)}
      aria-sort={sort.key === key ? (sort.dir === "asc" ? "ascending" : "descending") : "none"}
    >
      <span style={{ display: "inline-flex", alignItems: "center", gap: 4, flexDirection: right ? "row-reverse" : "row" }}>
        {label}
        {sort.key === key && (
          sort.dir === "asc"
            ? <ArrowUp size={12} strokeWidth={2.6} />
            : <ArrowDown size={12} strokeWidth={2.6} />
        )}
      </span>
    </th>
  );

  const totalMv = summary.data?.net_worth_idr ?? "0";
  const sub = `${positions.length} instrumen · nilai pasar ${formatIDR(totalMv)}`;

  return (
    <div>
      {/* Page header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 12 }}>
        <div>
          <h1 className="t-h1">Holdings</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>{sub}</div>
        </div>
        <div className="flex items-center" style={{ gap: 8 }}>
          <button type="button" className="btn btn-outline btn-sm" aria-label="Filter holdings">
            <Filter size={15} />
            Filter
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setAddOpen(true)}
            aria-label="Tambah holding"
          >
            <Plus size={15} />
            Tambah
          </button>
        </div>
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <div className="card">
          {sorted.length === 0 ? (
            <div className="empty">
              <div className="empty-icon">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M21 12V7H5a2 2 0 0 1 0-4h14v4"/><path d="M3 5v14a2 2 0 0 0 2 2h16v-5"/><path d="M18 12a2 2 0 0 0 0 4h4v-4Z"/>
                </svg>
              </div>
              <div>
                <div className="t-h3">Belum ada posisi</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Tambahkan transaksi untuk mulai melacak holdings.
                </div>
              </div>
            </div>
          ) : (
            <div className="table-wrap">
              <table className="tbl">
                <thead>
                  <tr>
                    {th("instrument_id", "Instrumen")}
                    {th("quantity", "Jumlah", true)}
                    {th("avg_cost", "Avg Cost", true)}
                    {th("latest_price", "Harga Terakhir", true)}
                    {th("market_value_idr", "Nilai Pasar", true)}
                    {th("unrealized_pnl", "Unrealized P&L", true)}
                  </tr>
                </thead>
                <tbody>
                  {sorted.map((p) => {
                    const symbol = nameOf(p.instrument_id);
                    const currency = currencyOf(p.instrument_id);
                    const pnl = parseNum(p.unrealized_pnl);
                    const costBasis = parseNum(p.cost_basis_total);
                    const pnlPct = costBasis !== 0 ? (pnl / costBasis) * 100 : 0;
                    // Primary display is always IDR. For non-IDR instruments convert at the
                    // position's implied current FX (market_value_idr / market_value_native)
                    // and keep the native figure visible as secondary context.
                    const mvNative = parseNum(p.market_value_native);
                    const fx = currency !== "IDR" && mvNative !== 0 ? parseNum(p.market_value_idr) / mvNative : null;
                    const cell = (value: string | number) => {
                      const v = typeof value === "number" ? value : parseNum(value);
                      if (fx == null) return <span className="num">{formatCurrency(v, currency)}</span>;
                      return (
                        <>
                          <div className="num">{formatIDR(v * fx)}</div>
                          <div className="t-xs t-muted num">{formatCurrency(v, currency)}</div>
                        </>
                      );
                    };
                    return (
                      <tr key={p.instrument_id}>
                        <td>
                          <div className="flex items-center" style={{ gap: 12 }}>
                            <div
                              className="dot"
                              style={{ background: "hsl(var(--primary))", width: 10, height: 10, flexShrink: 0 }}
                            />
                            <div>
                              <div style={{ fontWeight: 600 }}>{symbol}</div>
                            </div>
                          </div>
                        </td>
                        <td className="r num">{formatQty(p.quantity)}</td>
                        <td className="r num t-muted">{cell(p.avg_cost)}</td>
                        <td className="r">
                          <div className="flex items-center justify-end" style={{ gap: 8 }}>
                            {p.price_stale && (
                              <span className="badge badge-warn" title="Harga mungkin sudah usang">
                                <Clock size={11} />
                                stale
                              </span>
                            )}
                            <div>{cell(p.latest_price)}</div>
                          </div>
                        </td>
                        <td className="r num" style={{ fontWeight: 500 }}>{formatIDR(p.market_value_idr)}</td>
                        <td className="r">
                          <div className={"num " + (pnl >= 0 ? "gain" : "loss")} style={{ fontWeight: 500 }}>
                            {pnl >= 0 ? "+" : "−"}
                            {fx == null ? formatCurrency(Math.abs(pnl), currency) : formatIDR(Math.abs(pnl) * fx)}
                          </div>
                          {fx != null && (
                            <div className={"t-xs num " + (pnl >= 0 ? "gain" : "loss")}>
                              {pnl >= 0 ? "+" : "−"}{formatCurrency(Math.abs(pnl), currency)}
                            </div>
                          )}
                          <div className={"t-xs num " + (pnl >= 0 ? "gain" : "loss")}>
                            {formatPct(pnlPct)}
                          </div>
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </QueryState>

      {/* Add Transaction Dialog */}
      <AddTransactionDialog open={addOpen} onClose={() => setAddOpen(false)} />
    </div>
  );
}
