import { render, screen } from "@testing-library/react";
import { PerformanceCards } from "./PerformanceCards";
import type { PortfolioSummary } from "../api/schemas";

const base: PortfolioSummary = {
  net_worth_idr: "4875000", net_worth_usd: "300",
  total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0",
  xirr: 0.168, positions: [], allocation: [],
};

test("shows XIRR as a percentage", () => {
  render(<PerformanceCards s={base} />);
  expect(screen.getByText("16.8%")).toBeInTheDocument();
});

test("shows dash when XIRR is null", () => {
  render(<PerformanceCards s={{ ...base, xirr: null }} />);
  expect(screen.getByText("—")).toBeInTheDocument();
});
