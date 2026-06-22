import { render, screen } from "@testing-library/react";
import { DriftBarsChart } from "./DriftBarsChart";
import type { CategoryAllocation } from "../../api/schemas";

// total = 1.000.000.000 → target nominal = total × target%
const cats: CategoryAllocation[] = [
  { category_id: 1, name: "ETF US", target_pct: "50", tolerance_band_pct: "5", actual_pct: "40", actual_value_idr: "400000000", drift_pct: "-10", out_of_band: true, rebalance_idr: "100000000" },
  { category_id: 2, name: "Saham IDX", target_pct: "50", tolerance_band_pct: "5", actual_pct: "60", actual_value_idr: "600000000", drift_pct: "10", out_of_band: false, rebalance_idr: "-100000000" },
];

test("shows the actual/target IDR pair alongside the percent line", () => {
  render(<DriftBarsChart allocation={cats} />);
  // existing percent line is preserved
  expect(screen.getByText("40,0% / 50%")).toBeInTheDocument();
  // new IDR pair: actual 400jt, target = 1.000jt × 50% = 500jt
  expect(screen.getByText("Rp 400 jt / Rp 500 jt")).toBeInTheDocument();
  expect(screen.getByText("Rp 600 jt / Rp 500 jt")).toBeInTheDocument();
});
