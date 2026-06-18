/**
 * HyperliquidPositions — Perp positions and closed-trades section.
 *
 * Renders aggregate stats (realized PnL, win rate), a table of open
 * positions (unrealized PnL), and a table of recent closed trades
 * (realized PnL + metadata). Data comes from useHyperliquid() which
 * hits GET /portfolio/hyperliquid.
 *
 * Table classes follow the repo convention: `table-wrap` + `tbl`,
 * matching HoldingsPage and TransactionsPage.
 */

import { useHyperliquid } from "@/api/hooks";

/**
 * Returns the CSS class for a PnL value string.
 * @param value - Stringified numeric PnL (e.g. "100" or "-50")
 * @returns "gain" for non-negative, "loss" for negative
 */
function pnlClass(value: string): string {
  return Number(value) >= 0 ? "gain" : "loss";
}

export function HyperliquidPositions() {
  const query = useHyperliquid();
  const view = query.data;

  if (query.isLoading) {
    return <div className="skeleton" style={{ width: "100%", height: 200 }} />;
  }

  if (!view) return null;

  return (
    <div className="card">
      <div className="card-head">
        <div className="card-title">Hyperliquid — posisi &amp; trade</div>
        <div className="card-sub">
          Realized PnL:{" "}
          <span className={pnlClass(view.realized_pnl_total)}>
            ${view.realized_pnl_total}
          </span>
          {view.win_rate != null && (
            <> · win rate {(view.win_rate * 100).toFixed(0)}%</>
          )}
        </div>
      </div>

      <div className="card-pad">
        <div className="t-h3">Posisi terbuka</div>

        {view.positions.length === 0 ? (
          <div className="t-sm t-muted">Tidak ada posisi terbuka.</div>
        ) : (
          <div className="table-wrap">
            <table className="tbl">
              <thead>
                <tr>
                  <th>Coin</th>
                  <th>Arah</th>
                  <th className="r">Size</th>
                  <th className="r">Entry</th>
                  <th className="r">Mark</th>
                  <th className="r">uPnL</th>
                  <th className="r">Lev</th>
                </tr>
              </thead>
              <tbody>
                {view.positions.map((position) => (
                  <tr key={position.coin}>
                    <td style={{ fontWeight: 500 }}>{position.coin}</td>
                    <td>{position.direction}</td>
                    <td className="r num">{position.size}</td>
                    <td className="r num">${position.entry_px}</td>
                    <td className="r num">${position.mark_px}</td>
                    <td className={`r num ${pnlClass(position.unrealized_pnl)}`}>
                      ${position.unrealized_pnl}
                    </td>
                    <td className="r num">{position.leverage}x</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}

        <div className="t-h3" style={{ marginTop: 16 }}>
          Trade terakhir
        </div>

        {view.trades.length === 0 ? (
          <div className="t-sm t-muted">Belum ada trade selesai.</div>
        ) : (
          <div className="table-wrap">
            <table className="tbl">
              <thead>
                <tr>
                  <th>Coin</th>
                  <th>Arah</th>
                  <th className="r">Entry</th>
                  <th className="r">Exit</th>
                  <th className="r">PnL</th>
                  <th>TF</th>
                  <th>Tutup</th>
                </tr>
              </thead>
              <tbody>
                {view.trades.map((trade) => (
                  <tr key={trade.external_id}>
                    <td style={{ fontWeight: 500 }}>{trade.coin}</td>
                    <td>{trade.direction}</td>
                    <td className="r num">${trade.entry_px}</td>
                    <td className="r num">${trade.exit_px}</td>
                    <td className={`r num ${pnlClass(trade.realized_pnl)}`}>
                      ${trade.realized_pnl}
                    </td>
                    <td>{trade.timeframe ?? "—"}</td>
                    <td className="num t-muted">{trade.closed_at.slice(0, 10)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
}
