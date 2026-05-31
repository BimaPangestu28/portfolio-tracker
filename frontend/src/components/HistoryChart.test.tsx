import { render } from "@testing-library/react";
import { HistoryChart } from "./HistoryChart";
import type { Snapshot } from "../api/schemas";

test("renders without crashing for empty and non-empty data", () => {
  const empty: Snapshot[] = [];
  const { rerender, container } = render(<HistoryChart snapshots={empty} />);
  expect(container).toBeTruthy();
  const data: Snapshot[] = [
    { as_of: "2026-05-30", total_idr: "1000", total_usd: "0.06", breakdown_json: "[]" },
    { as_of: "2026-05-31", total_idr: "1100", total_usd: "0.07", breakdown_json: "[]" },
  ];
  rerender(<HistoryChart snapshots={data} />);
  expect(container).toBeTruthy();
});
