import { describe, it, expect } from "vitest";
import {
  isSyntheticNode, siblingTargetSum, treeRootsToAllocation,
  boundCategoryIds, boundInstrumentIds,
} from "./plan-tree";
import type { PlanNodeAllocation, PlanNodeRow } from "../api/schemas";

function alloc(partial: Partial<PlanNodeAllocation> & { id: number; name: string }): PlanNodeAllocation {
  return {
    bind_kind: "group", target_pct: "0", tolerance_band_pct: null,
    actual_pct: "0", actual_value_idr: "0", target_value_idr: "0",
    drift_pct: "0", out_of_band: false, rebalance_idr: "0", color: null,
    children: [], ...partial,
  };
}

describe("isSyntheticNode", () => {
  it("treats negative ids as synthetic", () => {
    expect(isSyntheticNode(-1)).toBe(true);
    expect(isSyntheticNode(-3)).toBe(true);
    expect(isSyntheticNode(1)).toBe(false);
  });
});

describe("siblingTargetSum", () => {
  it("sums user targets and excludes synthetic remainder nodes", () => {
    const nodes = [
      alloc({ id: 1, name: "A", target_pct: "60" }),
      alloc({ id: 2, name: "B", target_pct: "30" }),
      alloc({ id: -1, name: "Lainnya", target_pct: "0" }),
    ];
    expect(siblingTargetSum(nodes)).toBe(90);
  });
});

describe("treeRootsToAllocation", () => {
  it("maps top-level nodes to the CategoryAllocation shape", () => {
    const tree = [
      alloc({ id: 1, name: "Saham", target_pct: "60", actual_pct: "50", actual_value_idr: "100", drift_pct: "-10", out_of_band: true, rebalance_idr: "20", tolerance_band_pct: "5",
        children: [alloc({ id: 2, name: "BBCA", actual_value_idr: "60" })] }),
      alloc({ id: -1, name: "Lainnya", actual_value_idr: "80", actual_pct: "40" }),
    ];
    const out = treeRootsToAllocation(tree);
    expect(out).toHaveLength(2);          // roots only, children not flattened
    expect(out[0]).toEqual({
      category_id: 1, name: "Saham", target_pct: "60", tolerance_band_pct: "5",
      actual_pct: "50", actual_value_idr: "100", drift_pct: "-10",
      out_of_band: true, rebalance_idr: "20",
    });
    expect(out[1].category_id).toBe(-1);  // synthetic root keeps -1 (UNCATEGORIZED)
  });
});

describe("boundCategoryIds / boundInstrumentIds", () => {
  it("collects bound category and instrument ids", () => {
    const rows: PlanNodeRow[] = [
      { id: 1, parent_id: null, name: "Saham", target_pct: "60", tolerance_band_pct: null, bind_kind: "category", category_id: 1, instrument_id: null, sort_order: 0, color: null },
      { id: 2, parent_id: 1, name: "BBCA", target_pct: "40", tolerance_band_pct: null, bind_kind: "instrument", category_id: null, instrument_id: 7, sort_order: 0, color: null },
      { id: 3, parent_id: null, name: "Grup", target_pct: "10", tolerance_band_pct: null, bind_kind: "group", category_id: null, instrument_id: null, sort_order: 0, color: null },
    ];
    expect([...boundCategoryIds(rows)]).toEqual([1]);
    expect([...boundInstrumentIds(rows)]).toEqual([7]);
  });
});
