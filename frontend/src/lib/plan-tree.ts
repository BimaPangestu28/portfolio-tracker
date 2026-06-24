import type { PlanNodeAllocation, PlanNodeRow, CategoryAllocation } from "../api/schemas";
import { parseNum } from "./format";

/** Synthetic remainder nodes ("Lainnya") use negative ids; they are display-only. */
export function isSyntheticNode(id: number): boolean {
  return id < 0;
}

/** Sum of user-set target percents among siblings, excluding synthetic remainders. */
export function siblingTargetSum(nodes: PlanNodeAllocation[]): number {
  return nodes
    .filter((n) => !isSyntheticNode(n.id))
    .reduce((acc, n) => acc + parseNum(n.target_pct), 0);
}

/**
 * Map top-level tree nodes to the CategoryAllocation shape the donut/drift charts
 * already consume. The synthetic root "Lainnya" keeps id -1 (== UNCATEGORIZED_CATEGORY_ID),
 * which those charts special-case. Children are NOT flattened — only the top level
 * feeds the dashboard, mirroring the old flat category allocation.
 */
export function treeRootsToAllocation(tree: PlanNodeAllocation[]): CategoryAllocation[] {
  return tree.map((n) => ({
    category_id: n.id,
    name: n.name,
    target_pct: n.target_pct,
    tolerance_band_pct: n.tolerance_band_pct ?? null,
    actual_pct: n.actual_pct,
    actual_value_idr: n.actual_value_idr,
    drift_pct: n.drift_pct,
    out_of_band: n.out_of_band,
    rebalance_idr: n.rebalance_idr,
  }));
}

/** Category ids already represented by a node anywhere in the tree (to prevent double-count). */
export function boundCategoryIds(nodes: PlanNodeRow[]): Set<number> {
  return new Set(nodes.flatMap((n) => (n.category_id != null ? [n.category_id] : [])));
}

/** Instrument ids already represented by a leaf anywhere in the tree. */
export function boundInstrumentIds(nodes: PlanNodeRow[]): Set<number> {
  return new Set(nodes.flatMap((n) => (n.instrument_id != null ? [n.instrument_id] : [])));
}
