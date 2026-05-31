import { render, screen } from "@testing-library/react";
import { DriftBars } from "./DriftBars";
import type { CategoryAllocation } from "../api/schemas";

const cats: CategoryAllocation[] = [
  { category_id: 1, name: "USD ETF", target_pct: "50", tolerance_band_pct: "5", actual_pct: "40", actual_value_idr: "400", drift_pct: "-10", out_of_band: true, rebalance_idr: "100" },
  { category_id: 2, name: "Saham ID", target_pct: "50", tolerance_band_pct: "5", actual_pct: "60", actual_value_idr: "600", drift_pct: "10", out_of_band: false, rebalance_idr: "-100" },
];

test("renders each category name and flags out-of-band", () => {
  render(<DriftBars allocation={cats} />);
  expect(screen.getByText("USD ETF")).toBeInTheDocument();
  expect(screen.getByText("Saham ID")).toBeInTheDocument();
  // out-of-band category shows a warning marker
  expect(screen.getByTestId("oob-1")).toBeInTheDocument();
  expect(screen.queryByTestId("oob-2")).not.toBeInTheDocument();
});
