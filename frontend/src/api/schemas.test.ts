import { PortfolioSummarySchema, AccountSchema } from "./schemas";

test("parses a portfolio summary", () => {
  const json = {
    net_worth_idr: "4875000", net_worth_usd: "300",
    total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0",
    total_unrealized_price_pnl_idr: "80", total_unrealized_fx_pnl_idr: "20",
    total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
    fx_incomplete: false,
    xirr: 1.68,
    positions: [{
      instrument_id: 1, quantity: "2", avg_cost: "100", cost_basis_total: "200",
      latest_price: "150", price_stale: false, market_value_native: "300",
      market_value_idr: "4875000", market_value_usd: "300",
      unrealized_pnl: "100", realized_pnl: "0", income: "0",
      cost_basis_idr_total: "200", unrealized_pnl_idr: "100",
      unrealized_price_pnl_idr: "80", unrealized_fx_pnl_idr: "20",
      realized_pnl_idr: "0", realized_price_pnl_idr: "0", realized_fx_pnl_idr: "0",
      fx_incomplete: false,
    }],
    allocation: [{
      category_id: 1, name: "Crypto", target_pct: "100", tolerance_band_pct: "5",
      actual_pct: "100", actual_value_idr: "4875000", drift_pct: "0",
      out_of_band: false, rebalance_idr: "0",
    }],
  };
  const parsed = PortfolioSummarySchema.parse(json);
  expect(parsed.xirr).toBe(1.68);
  expect(parsed.positions[0].quantity).toBe("2");
  expect(parsed.allocation[0].out_of_band).toBe(false);
});

test("xirr may be null", () => {
  const parsed = PortfolioSummarySchema.parse({
    net_worth_idr: "0", net_worth_usd: "0", total_unrealized_pnl_idr: "0",
    total_realized_pnl_idr: "0",
    total_unrealized_price_pnl_idr: "0", total_unrealized_fx_pnl_idr: "0",
    total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
    fx_incomplete: false,
    xirr: null, positions: [], allocation: [],
  });
  expect(parsed.xirr).toBeNull();
});

test("account schema requires core fields", () => {
  const a = AccountSchema.parse({ id: 1, name: "M", account_type: "manual", institution: null, native_currency: "USD", note: null, created_at: "2026-01-01T00:00:00Z" });
  expect(a.name).toBe("M");
});
