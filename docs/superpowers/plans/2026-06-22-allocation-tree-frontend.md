# Allocation Tree — Frontend (Phase 2) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the flat category cards on the Planner page with an expandable allocation **tree** (drill-down, inline target edit, add/delete nodes, per-level target-sum indicator), and drive the dashboard's donut + drift cards from the tree's top level so there is a single source of allocation truth.

**Architecture:** Phase 1 shipped the backend (`/plan/*`). This phase is frontend-only. A new data layer (recursive zod schema + React Query hooks) wraps `/plan/tree`, `/plan/nodes`, and node CRUD. Pure helpers in `lib/plan-tree.ts` map tree roots to the existing `CategoryAllocation` shape and compute sibling sums. A recursive `PlanTreeNode` component renders rows; an `AddPlanNodeDialog` creates nodes. `PlannerPage` is rewritten around the tree; `DashboardPage` switches its allocation cards to the tree top level.

**Tech Stack:** React, TypeScript (strict), Vite, @tanstack/react-query, zod, MSW + Vitest + @testing-library/react, lucide-react, sonner.

**Spec:** `docs/superpowers/specs/2026-06-22-planner-tree-and-goals-design.md`

## Global Constraints

- **Strict TypeScript, no `any`** except in test files where the existing convention casts mocks `as any`.
- **Decimals are strings.** All money/percent values from the API are strings; parse for arithmetic with `parseNum` from `../lib/format`, never `parseFloat` ad hoc. Numeric inputs use the `NumberInput` component (Indonesian thousand-format) which emits a clean canonical string.
- **Zod-validate every response.** All API calls go through `api.get/post/patch/del` with a zod schema (`frontend/src/api/client.ts`). New endpoints get new schemas.
- **Query-key + invalidation convention** (`frontend/src/api/hooks.ts`): `useQuery({ queryKey: ["x"], queryFn: () => api.get("/x", Schema) })`; mutations use the `useInvalidatingMutation(fn, keys)` helper. New plan keys: `"plan-tree"`, `"plan-nodes"`. Plan mutations must also invalidate `"summary"` (the dashboard still reads `summary` for other cards).
- **Synthetic "Lainnya" nodes use negative ids** (`-1` root remainder, `-2 - category_id` per-category remainder). They are display-only: never editable, deletable, or given an "add child" affordance.
- **`bind_kind` is immutable** (rebind = delete + recreate). The edit path never changes a node's binding.
- **Indonesian UI copy**, matching existing tone ("Tambah", "Hapus", "Target", "Lainnya", toast messages like the current Planner).
- **No new dependencies.** Reuse `NumberInput`, `categoryColor`, `formatIDR`, `parseNum`, `QueryState`, the local `Dialog`/`ProgressBar` patterns already in the codebase.
- **Test command (from `frontend/`):** `npm run test` (Vitest, single run). Typecheck/build: `npm run build` (runs `tsc -b` then Vite). Scope a single file: `npm run test -- src/path/to/file.test.tsx`.
- **Conventional commits** (`feat:`/`refactor:`). Commit after every green test cycle.

## Endpoint → response contract (from Phase 1 backend)

- `GET /plan/tree` → `PlanNodeAllocation[]` (recursive `children`). Node fields: `id, name, bind_kind, target_pct, tolerance_band_pct, actual_pct, actual_value_idr, target_value_idr, drift_pct, out_of_band, rebalance_idr, color, children`. All decimals are strings; `tolerance_band_pct`/`color` may be null.
- `GET /plan/nodes` → `PlanNodeRow[]`. Fields: `id, parent_id, name, target_pct, tolerance_band_pct, bind_kind, category_id, instrument_id, sort_order, color`.
- `POST /plan/nodes` (body `NewPlanNode`) → `PlanNodeRow`. Unknown `parent_id` → 400; bind-kind validation errors → 400.
- `PATCH /plan/nodes/:id` (body `UpdatePlanNode`) → `PlanNodeRow`. Missing id → 404.
- `DELETE /plan/nodes/:id` → `{}` (idempotent; cascades children).

## File structure

- **Modify** `frontend/src/api/schemas.ts` — add `PlanNodeAllocationSchema` (recursive), `PlanNodeRowSchema`, exported types.
- **Modify** `frontend/src/api/hooks.ts` — add `usePlanTree`, `usePlanNodes`, `useCreatePlanNode`, `useUpdatePlanNode`, `useDeletePlanNode`, `NewPlanNode` type, type re-exports.
- **Modify** `frontend/src/test/server.ts` — add MSW handlers for the plan endpoints.
- **Create** `frontend/src/lib/plan-tree.ts` — pure helpers (`isSyntheticNode`, `siblingTargetSum`, `treeRootsToAllocation`, `boundCategoryIds`, `boundInstrumentIds`).
- **Create** `frontend/src/components/planner/PlanTreeNode.tsx` — recursive node row.
- **Create** `frontend/src/components/planner/AddPlanNodeDialog.tsx` — create-node dialog.
- **Modify** `frontend/src/pages/PlannerPage.tsx` — rewrite around the tree.
- **Modify** `frontend/src/pages/DashboardPage.tsx` — drive allocation cards from the tree.
- **Create** test files alongside: `lib/plan-tree.test.ts`, `components/planner/PlanTreeNode.test.tsx`, `components/planner/AddPlanNodeDialog.test.tsx`, `pages/PlannerPage.test.tsx`; extend `api/hooks.test.tsx`.

---

## Task 1: Data layer — schemas + hooks + MSW handlers

**Files:**
- Modify: `frontend/src/api/schemas.ts`
- Modify: `frontend/src/api/hooks.ts`
- Modify: `frontend/src/test/server.ts`
- Test: `frontend/src/api/hooks.test.tsx`

**Interfaces:**
- Produces (schemas.ts):
  - `type PlanNodeAllocation` (recursive, `children: PlanNodeAllocation[]`) + `PlanNodeAllocationSchema: z.ZodType<PlanNodeAllocation>`.
  - `type PlanNodeRow` + `PlanNodeRowSchema`.
- Produces (hooks.ts): `usePlanTree()`, `usePlanNodes()`, `useCreatePlanNode()`, `useUpdatePlanNode()`, `useDeletePlanNode()`, `type NewPlanNode`; re-exports `PlanNodeAllocation`, `PlanNodeRow`.

- [ ] **Step 1: Add the schemas**

In `frontend/src/api/schemas.ts`, append (after the existing `CategoryAllocationSchema` block):

```typescript
// Recursive allocation tree node (GET /plan/tree). Decimals are strings.
export type PlanNodeAllocation = {
  id: number;
  name: string;
  bind_kind: string;
  target_pct: string;
  tolerance_band_pct?: string | null;
  actual_pct: string;
  actual_value_idr: string;
  target_value_idr: string;
  drift_pct: string;
  out_of_band: boolean;
  rebalance_idr: string;
  color?: string | null;
  children: PlanNodeAllocation[];
};

export const PlanNodeAllocationSchema: z.ZodType<PlanNodeAllocation> = z.lazy(() =>
  z.object({
    id: z.number(),
    name: z.string(),
    bind_kind: z.string(),
    target_pct: z.string(),
    tolerance_band_pct: z.string().nullable().optional(),
    actual_pct: z.string(),
    actual_value_idr: z.string(),
    target_value_idr: z.string(),
    drift_pct: z.string(),
    out_of_band: z.boolean(),
    rebalance_idr: z.string(),
    color: z.string().nullable().optional(),
    children: z.array(PlanNodeAllocationSchema),
  }),
);

// Raw plan node row (GET /plan/nodes, POST/PATCH responses).
export const PlanNodeRowSchema = z.object({
  id: z.number(),
  parent_id: z.number().nullable().optional(),
  name: z.string(),
  target_pct: z.string(),
  tolerance_band_pct: z.string().nullable().optional(),
  bind_kind: z.string(),
  category_id: z.number().nullable().optional(),
  instrument_id: z.number().nullable().optional(),
  sort_order: z.number(),
  color: z.string().nullable().optional(),
});
export type PlanNodeRow = z.infer<typeof PlanNodeRowSchema>;
```

- [ ] **Step 2: Add MSW handlers (so the hook test has data)**

In `frontend/src/test/server.ts`, add these handlers to the `handlers` array (near the other `/api/...` GET handlers):

```typescript
  http.get("/api/plan/tree", () =>
    HttpResponse.json([
      {
        id: 1, name: "Saham IDX", bind_kind: "category",
        target_pct: "60", tolerance_band_pct: "5",
        actual_pct: "50", actual_value_idr: "2437500", target_value_idr: "2925000",
        drift_pct: "-10", out_of_band: true, rebalance_idr: "487500", color: null,
        children: [
          {
            id: 2, name: "BBCA", bind_kind: "instrument",
            target_pct: "40", tolerance_band_pct: null,
            actual_pct: "60", actual_value_idr: "1462500", target_value_idr: "1170000",
            drift_pct: "20", out_of_band: false, rebalance_idr: "-292500", color: null,
            children: [],
          },
        ],
      },
      {
        id: -1, name: "Lainnya", bind_kind: "lainnya",
        target_pct: "0", tolerance_band_pct: null,
        actual_pct: "50", actual_value_idr: "2437500", target_value_idr: "0",
        drift_pct: "50", out_of_band: false, rebalance_idr: "0", color: null,
        children: [],
      },
    ]),
  ),
  http.get("/api/plan/nodes", () =>
    HttpResponse.json([
      { id: 1, parent_id: null, name: "Saham IDX", target_pct: "60", tolerance_band_pct: "5", bind_kind: "category", category_id: 1, instrument_id: null, sort_order: 0, color: null },
      { id: 2, parent_id: 1, name: "BBCA", target_pct: "40", tolerance_band_pct: null, bind_kind: "instrument", category_id: null, instrument_id: 7, sort_order: 0, color: null },
    ]),
  ),
```

- [ ] **Step 3: Write the failing hook test**

In `frontend/src/api/hooks.test.tsx`, add `usePlanTree` to the import from `./hooks`, then add:

```typescript
test("usePlanTree fetches and validates the recursive tree", async () => {
  const { result } = renderHook(() => usePlanTree(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.length).toBe(2);
  const saham = result.current.data?.[0];
  expect(saham?.name).toBe("Saham IDX");
  expect(saham?.children[0]?.name).toBe("BBCA");
  expect(result.current.data?.[1]?.id).toBe(-1); // synthetic root "Lainnya"
});
```

- [ ] **Step 4: Run the test to verify it fails**

Run: `npm run test -- src/api/hooks.test.tsx`
Expected: FAIL — `usePlanTree` is not exported from `./hooks`.

- [ ] **Step 5: Add the hooks**

In `frontend/src/api/hooks.ts`, add the schema imports to the existing `from "./schemas"` import (`PlanNodeAllocationSchema`, `PlanNodeRowSchema`, and the types `PlanNodeAllocation`, `PlanNodeRow`). Then add:

```typescript
export const usePlanTree = () =>
  useQuery({ queryKey: ["plan-tree"], queryFn: () => api.get("/plan/tree", z.array(PlanNodeAllocationSchema)) });

export const usePlanNodes = () =>
  useQuery({ queryKey: ["plan-nodes"], queryFn: () => api.get("/plan/nodes", z.array(PlanNodeRowSchema)) });

export type NewPlanNode = {
  parent_id?: number | null;
  name: string;
  target_pct: string;
  tolerance_band_pct?: string | null;
  bind_kind: string;
  category_id?: number | null;
  instrument_id?: number | null;
  sort_order?: number | null;
  color?: string | null;
};

export const useCreatePlanNode = () =>
  useInvalidatingMutation((b: NewPlanNode) => api.post("/plan/nodes", PlanNodeRowSchema, b), ["plan-tree", "plan-nodes", "summary"]);

export const useUpdatePlanNode = () =>
  useInvalidatingMutation(
    (args: { id: number; patch: { name?: string; target_pct?: string; tolerance_band_pct?: string | null; sort_order?: number; color?: string | null } }) =>
      api.patch(`/plan/nodes/${args.id}`, PlanNodeRowSchema, args.patch),
    ["plan-tree", "plan-nodes", "summary"],
  );

export const useDeletePlanNode = () =>
  useInvalidatingMutation((id: number) => api.del(`/plan/nodes/${id}`), ["plan-tree", "plan-nodes", "summary"]);
```

Also re-export the types so components import them from `../api/hooks` (matching the existing `type Category` convention). Add to the type re-exports in hooks.ts:

```typescript
export type { PlanNodeAllocation, PlanNodeRow } from "./schemas";
```

(If hooks.ts has no existing `export type { ... } from "./schemas"` line, add this new one near the top-level exports.)

- [ ] **Step 6: Run the test to verify it passes**

Run: `npm run test -- src/api/hooks.test.tsx`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/src/api/schemas.ts frontend/src/api/hooks.ts frontend/src/test/server.ts frontend/src/api/hooks.test.tsx
git commit -m "feat(planner-web): plan-tree schemas + React Query hooks"
```

---

## Task 2: Pure helpers — `lib/plan-tree.ts`

**Files:**
- Create: `frontend/src/lib/plan-tree.ts`
- Test: `frontend/src/lib/plan-tree.test.ts`

**Interfaces:**
- Consumes: `PlanNodeAllocation`, `PlanNodeRow`, `CategoryAllocation` from `../api/schemas`; `parseNum` from `./format`.
- Produces:
  - `isSyntheticNode(id: number): boolean`
  - `siblingTargetSum(nodes: PlanNodeAllocation[]): number`
  - `treeRootsToAllocation(tree: PlanNodeAllocation[]): CategoryAllocation[]`
  - `boundCategoryIds(nodes: PlanNodeRow[]): Set<number>`
  - `boundInstrumentIds(nodes: PlanNodeRow[]): Set<number>`

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/lib/plan-tree.test.ts`:

```typescript
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm run test -- src/lib/plan-tree.test.ts`
Expected: FAIL — module `./plan-tree` not found.

- [ ] **Step 3: Implement the helpers**

Create `frontend/src/lib/plan-tree.ts`:

```typescript
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm run test -- src/lib/plan-tree.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/plan-tree.ts frontend/src/lib/plan-tree.test.ts
git commit -m "feat(planner-web): pure plan-tree helpers (roots→allocation, sibling sum)"
```

---

## Task 3: Recursive `PlanTreeNode` component

**Files:**
- Create: `frontend/src/components/planner/PlanTreeNode.tsx`
- Test: `frontend/src/components/planner/PlanTreeNode.test.tsx`

**Interfaces:**
- Consumes: `useUpdatePlanNode`, `useDeletePlanNode`, `type PlanNodeAllocation` from `../../api/hooks`; `NumberInput` from `@/components/ui/NumberInput`; `formatIDR`, `parseNum` from `../../lib/format`; `isSyntheticNode` from `../../lib/plan-tree`; `categoryColor` from `../charts/AllocationDonutChart`.
- Produces: `export function PlanTreeNode({ node, depth, onAddChild }: { node: PlanNodeAllocation; depth: number; onAddChild: (parent: PlanNodeAllocation) => void })`.
- Behavior: renders one node row indented by `depth`; expand/collapse when it has children; drift badge + actual% + editable target% (saves on blur/Enter via `useUpdatePlanNode`); rebalance hint when out of band; a per-level "anak Σ X%" chip when the node has non-synthetic children (the spec's per-level sibling-sum indicator); "+ child" (calls `onAddChild(node)`) and delete buttons. Synthetic nodes (`isSyntheticNode(node.id)`) render muted with no target input and no action buttons. Renders children recursively.

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/planner/PlanTreeNode.test.tsx`:

```typescript
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { PlanTreeNode } from "./PlanTreeNode";
import * as hooks from "../../api/hooks";
import type { PlanNodeAllocation } from "../../api/schemas";

vi.mock("../../api/hooks");

const updateMutate = vi.fn();
const deleteMutate = vi.fn();

beforeEach(() => {
  updateMutate.mockReset();
  deleteMutate.mockReset();
  vi.mocked(hooks.useUpdatePlanNode).mockReturnValue({ mutate: updateMutate, isPending: false } as any);
  vi.mocked(hooks.useDeletePlanNode).mockReturnValue({ mutate: deleteMutate, isPending: false } as any);
});

function node(partial: Partial<PlanNodeAllocation> & { id: number; name: string }): PlanNodeAllocation {
  return {
    bind_kind: "category", target_pct: "60", tolerance_band_pct: "5",
    actual_pct: "50", actual_value_idr: "100", target_value_idr: "120",
    drift_pct: "-10", out_of_band: true, rebalance_idr: "20", color: null,
    children: [], ...partial,
  };
}

describe("PlanTreeNode", () => {
  it("renders the node, its drift badge, and a child", () => {
    const tree = node({ id: 1, name: "Saham", children: [node({ id: 2, name: "BBCA", out_of_band: false })] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={() => {}} />);
    expect(screen.getByText("Saham")).toBeInTheDocument();
    expect(screen.getByText("BBCA")).toBeInTheDocument();
    expect(screen.getByText(/drift/i)).toBeInTheDocument();
  });

  it("collapses and expands children", () => {
    const tree = node({ id: 1, name: "Saham", children: [node({ id: 2, name: "BBCA" })] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={() => {}} />);
    expect(screen.getByText("BBCA")).toBeInTheDocument();
    fireEvent.click(screen.getByLabelText("Tutup Saham"));
    expect(screen.queryByText("BBCA")).not.toBeInTheDocument();
  });

  it("saves an edited target on blur", () => {
    const tree = node({ id: 1, name: "Saham", children: [] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={() => {}} />);
    const input = screen.getByLabelText("Target Saham");
    fireEvent.change(input, { target: { value: "40" } });
    fireEvent.blur(input);
    expect(updateMutate).toHaveBeenCalledWith(
      { id: 1, patch: { target_pct: "40" } },
      expect.anything(),
    );
  });

  it("deletes the node", () => {
    const tree = node({ id: 1, name: "Saham", children: [] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={() => {}} />);
    fireEvent.click(screen.getByLabelText("Hapus Saham"));
    expect(deleteMutate).toHaveBeenCalledWith(1, expect.anything());
  });

  it("calls onAddChild when the add button is clicked", () => {
    const onAdd = vi.fn();
    const tree = node({ id: 1, name: "Saham", children: [] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={onAdd} />);
    fireEvent.click(screen.getByLabelText("Tambah anak Saham"));
    expect(onAdd).toHaveBeenCalledWith(tree);
  });

  it("renders synthetic Lainnya without edit/delete/add controls", () => {
    const tree = node({ id: -1, name: "Lainnya", out_of_band: false, children: [] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={() => {}} />);
    expect(screen.getByText("Lainnya")).toBeInTheDocument();
    expect(screen.queryByLabelText("Target Lainnya")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Hapus Lainnya")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Tambah anak Lainnya")).not.toBeInTheDocument();
  });

  it("shows a per-level children target-sum chip on a parent", () => {
    const tree = node({ id: 1, name: "Saham", children: [
      node({ id: 2, name: "BBCA", target_pct: "40" }),
      node({ id: 3, name: "BBRI", target_pct: "30" }),
      node({ id: -3, name: "Lainnya", target_pct: "0" }), // synthetic excluded
    ] });
    render(<PlanTreeNode node={tree} depth={0} onAddChild={() => {}} />);
    expect(screen.getByText(/anak.*70/i)).toBeInTheDocument(); // 40 + 30
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm run test -- src/components/planner/PlanTreeNode.test.tsx`
Expected: FAIL — module `./PlanTreeNode` not found.

- [ ] **Step 3: Implement the component**

Create `frontend/src/components/planner/PlanTreeNode.tsx`:

```typescript
import { useState } from "react";
import { ChevronRight, ChevronDown, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useUpdatePlanNode, useDeletePlanNode, type PlanNodeAllocation } from "../../api/hooks";
import { NumberInput } from "@/components/ui/NumberInput";
import { formatIDR, parseNum } from "../../lib/format";
import { isSyntheticNode, siblingTargetSum } from "../../lib/plan-tree";
import { categoryColor } from "../charts/AllocationDonutChart";

interface Props {
  node: PlanNodeAllocation;
  depth: number;
  onAddChild: (parent: PlanNodeAllocation) => void;
}

export function PlanTreeNode({ node, depth, onAddChild }: Props) {
  const [expanded, setExpanded] = useState(true);
  const [target, setTarget] = useState(node.target_pct);
  const update = useUpdatePlanNode();
  const del = useDeletePlanNode();

  const synthetic = isSyntheticNode(node.id);
  const hasChildren = node.children.length > 0;
  const hasRealChildren = node.children.some((c) => !isSyntheticNode(c.id));
  const childTargetSum = siblingTargetSum(node.children);
  const actual = parseNum(node.actual_pct);
  const drift = parseNum(node.drift_pct);
  const rebalance = parseNum(node.rebalance_idr);
  const color = categoryColor(node.name);

  const saveTarget = () => {
    if (target === node.target_pct) return;
    update.mutate(
      { id: node.id, patch: { target_pct: target } },
      {
        onSuccess: () => toast.success(`Target ${node.name} disimpan`),
        onError: (err) => { toast.error((err as Error).message); setTarget(node.target_pct); },
      },
    );
  };

  const remove = () => {
    del.mutate(node.id, {
      onSuccess: () => toast.success(`"${node.name}" dihapus`),
      onError: (err) => toast.error((err as Error).message),
    });
  };

  return (
    <div>
      <div
        className="flex items-center gap-2"
        style={{ padding: "10px 0", paddingLeft: depth * 22, borderBottom: "1px solid hsl(var(--border))" }}
        data-testid={`plan-node-${node.id}`}
      >
        <button
          type="button"
          className="icon-btn"
          style={{ width: 22, height: 22, visibility: hasChildren ? "visible" : "hidden" }}
          onClick={() => setExpanded((v) => !v)}
          aria-label={expanded ? `Tutup ${node.name}` : `Buka ${node.name}`}
        >
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <span className="dot" style={{ background: color, width: 10, height: 10, flexShrink: 0 }} />
        <span style={{ fontWeight: synthetic ? 400 : 600, color: synthetic ? "hsl(var(--muted-foreground))" : "inherit" }}>
          {node.name}
        </span>

        {!synthetic && (
          node.out_of_band ? (
            <span className="badge badge-warn">drift {drift > 0 ? "+" : ""}{drift.toFixed(1)}%</span>
          ) : (
            <span className="badge badge-gain">on target</span>
          )
        )}

        {hasRealChildren && (
          <span className="t-xs t-muted" style={{ whiteSpace: "nowrap" }}>
            anak Σ {childTargetSum.toFixed(0)}%
          </span>
        )}

        <div className="flex items-center" style={{ marginLeft: "auto", gap: 10 }}>
          <span className="t-sm num t-muted">{actual.toFixed(1)}%</span>
          {synthetic ? (
            <span className="t-sm num t-muted" style={{ width: 64, textAlign: "right" }}>tanpa target</span>
          ) : (
            <span className="t-sm num" style={{ display: "inline-flex", alignItems: "baseline", gap: 2 }}>
              <NumberInput
                className=""
                style={{ width: 56, height: 26, textAlign: "right", fontSize: "inherit", padding: "0 4px" }}
                aria-label={`Target ${node.name}`}
                value={target}
                onChange={(v) => setTarget(v)}
                onBlur={saveTarget}
                onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
              />
              %
            </span>
          )}
          {!synthetic && (
            <>
              <button
                type="button"
                className="icon-btn"
                style={{ width: 26, height: 26 }}
                onClick={() => onAddChild(node)}
                aria-label={`Tambah anak ${node.name}`}
              >
                <Plus size={13} />
              </button>
              <button
                type="button"
                className="icon-btn"
                style={{ width: 26, height: 26 }}
                onClick={remove}
                aria-label={`Hapus ${node.name}`}
              >
                <Trash2 size={13} />
              </button>
            </>
          )}
        </div>
      </div>

      {!synthetic && node.out_of_band && rebalance !== 0 && (
        <div className="t-xs warn num" style={{ paddingLeft: depth * 22 + 32, padding: "0 0 6px", fontWeight: 500 }}>
          {rebalance > 0 ? "Beli " : "Pangkas "}{formatIDR(Math.abs(rebalance))}
        </div>
      )}

      {expanded && hasChildren && node.children.map((child) => (
        <PlanTreeNode key={child.id} node={child} depth={depth + 1} onAddChild={onAddChild} />
      ))}
    </div>
  );
}
```

> Note for the implementer: the "saves an edited target on blur" test drives `NumberInput` with a plain integer (`"40"`), which it emits unchanged as the clean value. If `NumberInput`'s internal formatting prevents the change event from propagating in jsdom, assert via firing `keyDown` Enter instead of `blur` — but try `blur` first; it matches the existing `TargetEditor` pattern.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm run test -- src/components/planner/PlanTreeNode.test.tsx`
Expected: PASS (6 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/planner/PlanTreeNode.tsx frontend/src/components/planner/PlanTreeNode.test.tsx
git commit -m "feat(planner-web): recursive PlanTreeNode row with inline target edit"
```

---

## Task 4: `AddPlanNodeDialog`

**Files:**
- Create: `frontend/src/components/planner/AddPlanNodeDialog.tsx`
- Test: `frontend/src/components/planner/AddPlanNodeDialog.test.tsx`

**Interfaces:**
- Consumes: `useCreatePlanNode`, `useCategories`, `useInstruments`, `usePlanNodes` from `../../api/hooks`; `boundCategoryIds`, `boundInstrumentIds` from `../../lib/plan-tree`; `NumberInput`; `toast`.
- Produces: `export function AddPlanNodeDialog({ open, parent, onClose }: { open: boolean; parent: PlanNodeAllocation | null; onClose: () => void })`.
- Behavior: when `parent` is `null` the new node is a **root** and binds a **category** (existing unbound category, or a new one created inline). When `parent` is set the new node is a **child** and binds an **instrument** (unbound) or is a **group**. Submitting calls `useCreatePlanNode` with the right `NewPlanNode`; for a brand-new category it first creates the category (`useCreateCategory`, `target_pct: "0"`) then the node. The bind-kind selector is constrained by level (root → category; child → instrument/group); `bind_kind` is never editable after creation (immutable by design).

- [ ] **Step 1: Write the failing tests**

Create `frontend/src/components/planner/AddPlanNodeDialog.test.tsx`:

```typescript
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { AddPlanNodeDialog } from "./AddPlanNodeDialog";
import * as hooks from "../../api/hooks";
import type { PlanNodeAllocation } from "../../api/schemas";

vi.mock("../../api/hooks");

const createNode = vi.fn();
const createCategory = vi.fn();

beforeEach(() => {
  createNode.mockReset();
  createCategory.mockReset();
  vi.mocked(hooks.useCreatePlanNode).mockReturnValue({ mutate: createNode, isPending: false } as any);
  vi.mocked(hooks.useCreateCategory).mockReturnValue({ mutate: createCategory, isPending: false } as any);
  vi.mocked(hooks.useCategories).mockReturnValue({ data: [{ id: 3, name: "Reksadana", target_pct: "0", tolerance_band_pct: null, sort_order: 0, color: null }] } as any);
  vi.mocked(hooks.useInstruments).mockReturnValue({ data: [{ id: 7, symbol: "BBCA", name: "Bank BCA", instrument_type: "stock", native_currency: "IDR", category_id: 1, price_source: "manual", decimals: 0, note: null }] } as any);
  vi.mocked(hooks.usePlanNodes).mockReturnValue({ data: [] } as any);
});

const parentNode: PlanNodeAllocation = {
  id: 1, name: "Saham IDX", bind_kind: "category", target_pct: "60", tolerance_band_pct: "5",
  actual_pct: "50", actual_value_idr: "100", target_value_idr: "120", drift_pct: "-10",
  out_of_band: false, rebalance_idr: "0", color: null, children: [],
};

describe("AddPlanNodeDialog", () => {
  it("creates a child instrument node under a parent", () => {
    render(<AddPlanNodeDialog open parent={parentNode} onClose={() => {}} />);
    // child level defaults to instrument bind; pick BBCA, set target, submit
    fireEvent.change(screen.getByLabelText("Pilih instrumen"), { target: { value: "7" } });
    fireEvent.change(screen.getByLabelText("Target persen"), { target: { value: "40" } });
    fireEvent.click(screen.getByRole("button", { name: /simpan/i }));
    expect(createNode).toHaveBeenCalledWith(
      expect.objectContaining({ parent_id: 1, bind_kind: "instrument", instrument_id: 7, target_pct: "40", name: "BBCA" }),
      expect.anything(),
    );
  });

  it("creates a root node bound to an existing category", () => {
    render(<AddPlanNodeDialog open parent={null} onClose={() => {}} />);
    fireEvent.change(screen.getByLabelText("Pilih kategori"), { target: { value: "3" } });
    fireEvent.change(screen.getByLabelText("Target persen"), { target: { value: "25" } });
    fireEvent.click(screen.getByRole("button", { name: /simpan/i }));
    expect(createNode).toHaveBeenCalledWith(
      expect.objectContaining({ parent_id: null, bind_kind: "category", category_id: 3, target_pct: "25", name: "Reksadana" }),
      expect.anything(),
    );
  });

  it("renders nothing when closed", () => {
    const { container } = render(<AddPlanNodeDialog open={false} parent={null} onClose={() => {}} />);
    expect(container).toBeEmptyDOMElement();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `npm run test -- src/components/planner/AddPlanNodeDialog.test.tsx`
Expected: FAIL — module `./AddPlanNodeDialog` not found.

- [ ] **Step 3: Implement the dialog**

Create `frontend/src/components/planner/AddPlanNodeDialog.tsx`:

```typescript
import { useEffect, useMemo, useState } from "react";
import ReactDOM from "react-dom";
import { Check, X } from "lucide-react";
import { toast } from "sonner";
import {
  useCreatePlanNode, useCreateCategory, useCategories, useInstruments, usePlanNodes,
  type NewPlanNode, type PlanNodeAllocation,
} from "../../api/hooks";
import { NumberInput } from "@/components/ui/NumberInput";
import { boundCategoryIds, boundInstrumentIds } from "../../lib/plan-tree";

interface Props {
  open: boolean;
  parent: PlanNodeAllocation | null;
  onClose: () => void;
}

// Root nodes bind an asset-class category; child nodes bind an instrument or are a group.
type RootBind = "category";
type ChildBind = "instrument" | "group";

export function AddPlanNodeDialog({ open, parent, onClose }: Props) {
  const isRoot = parent === null;
  const createNode = useCreatePlanNode();
  const createCategory = useCreateCategory();
  const cats = useCategories();
  const instruments = useInstruments();
  const rawNodes = usePlanNodes();

  const [childKind, setChildKind] = useState<ChildBind>("instrument");
  const [categoryId, setCategoryId] = useState("");
  const [newCategoryName, setNewCategoryName] = useState("");
  const [instrumentId, setInstrumentId] = useState("");
  const [groupName, setGroupName] = useState("");
  const [target, setTarget] = useState("");
  const [tolerance, setTolerance] = useState("");

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  // Reset fields whenever the dialog (re)opens for a different parent.
  useEffect(() => {
    if (open) {
      setChildKind("instrument");
      setCategoryId(""); setNewCategoryName(""); setInstrumentId(""); setGroupName("");
      setTarget(""); setTolerance("");
    }
  }, [open, parent]);

  const usedCats = useMemo(() => boundCategoryIds(rawNodes.data ?? []), [rawNodes.data]);
  const usedInstruments = useMemo(() => boundInstrumentIds(rawNodes.data ?? []), [rawNodes.data]);
  const availableCats = (cats.data ?? []).filter((c) => !usedCats.has(c.id));
  const availableInstruments = (instruments.data ?? []).filter((i) => !usedInstruments.has(i.id));

  if (!open) return null;

  const bindKind: RootBind | ChildBind = isRoot ? "category" : childKind;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();

    const afterCreate = (payload: NewPlanNode) =>
      createNode.mutate(payload, {
        onSuccess: () => { toast.success(`"${payload.name}" ditambahkan`); onClose(); },
        onError: (err) => toast.error((err as Error).message),
      });

    const base = {
      parent_id: isRoot ? null : parent!.id,
      target_pct: target || "0",
      tolerance_band_pct: tolerance || null,
      sort_order: null,
      color: null,
    };

    if (bindKind === "category") {
      if (newCategoryName.trim()) {
        // Create the category first, then a root node bound to it.
        createCategory.mutate(
          { name: newCategoryName.trim(), target_pct: "0", tolerance_band_pct: null, color: null },
          {
            onSuccess: (cat) => afterCreate({ ...base, name: cat.name, bind_kind: "category", category_id: cat.id }),
            onError: (err) => toast.error((err as Error).message),
          },
        );
        return;
      }
      const id = Number(categoryId);
      const cat = (cats.data ?? []).find((c) => c.id === id);
      if (!cat) { toast.error("Pilih kategori dulu"); return; }
      afterCreate({ ...base, name: cat.name, bind_kind: "category", category_id: cat.id });
      return;
    }

    if (bindKind === "instrument") {
      const id = Number(instrumentId);
      const ins = (instruments.data ?? []).find((i) => i.id === id);
      if (!ins) { toast.error("Pilih instrumen dulu"); return; }
      afterCreate({ ...base, name: ins.symbol, bind_kind: "instrument", instrument_id: ins.id });
      return;
    }

    // group
    if (!groupName.trim()) { toast.error("Isi nama grup dulu"); return; }
    afterCreate({ ...base, name: groupName.trim(), bind_kind: "group" });
  };

  return ReactDOM.createPortal(
    <div className="dialog-scrim" onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }} role="presentation">
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="add-node-title">
        <div className="dialog-head">
          <div>
            <div className="t-h2" id="add-node-title">
              {isRoot ? "Tambah Kelas Aset" : `Tambah di bawah ${parent!.name}`}
            </div>
            <div className="card-sub" style={{ marginTop: 3 }}>
              {isRoot ? "Node akar terikat ke kategori aset" : "Pecah jadi instrumen atau sub-grup"}
            </div>
          </div>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Tutup dialog" style={{ width: 32, height: 32 }}>
            <X size={18} />
          </button>
        </div>

        <div className="dialog-body">
          <form id="add-node-form" onSubmit={submit}>
            {!isRoot && (
              <label className="field">
                <span className="field-label">Jenis</span>
                <select className="input" aria-label="Jenis node" value={childKind} onChange={(e) => setChildKind(e.target.value as ChildBind)}>
                  <option value="instrument">Instrumen</option>
                  <option value="group">Sub-grup</option>
                </select>
              </label>
            )}

            {bindKind === "category" && (
              <>
                <label className="field">
                  <span className="field-label">Kategori</span>
                  <select className="input" aria-label="Pilih kategori" value={categoryId} onChange={(e) => { setCategoryId(e.target.value); setNewCategoryName(""); }}>
                    <option value="">— pilih kategori —</option>
                    {availableCats.map((c) => <option key={c.id} value={c.id}>{c.name}</option>)}
                  </select>
                </label>
                <label className="field">
                  <span className="field-label">…atau buat kategori baru</span>
                  <input className="input" placeholder="mis. Properti" aria-label="Nama kategori baru" value={newCategoryName} onChange={(e) => { setNewCategoryName(e.target.value); setCategoryId(""); }} />
                </label>
              </>
            )}

            {bindKind === "instrument" && (
              <label className="field">
                <span className="field-label">Instrumen</span>
                <select className="input" aria-label="Pilih instrumen" value={instrumentId} onChange={(e) => setInstrumentId(e.target.value)}>
                  <option value="">— pilih instrumen —</option>
                  {availableInstruments.map((i) => <option key={i.id} value={i.id}>{i.symbol} — {i.name}</option>)}
                </select>
              </label>
            )}

            {bindKind === "group" && (
              <label className="field">
                <span className="field-label">Nama grup</span>
                <input className="input" placeholder="mis. Perbankan" aria-label="Nama grup" value={groupName} onChange={(e) => setGroupName(e.target.value)} />
              </label>
            )}

            <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
              <label className="field">
                <span className="field-label">Target %</span>
                <NumberInput className="input" placeholder="0" aria-label="Target persen" value={target} onChange={setTarget} required />
              </label>
              <label className="field">
                <span className="field-label">Toleransi ± %</span>
                <NumberInput className="input" placeholder="5" aria-label="Toleransi persen" value={tolerance} onChange={setTolerance} />
              </label>
            </div>
          </form>
        </div>

        <div className="dialog-foot">
          <button type="button" className="btn btn-outline" onClick={onClose}>Batal</button>
          <button type="button" className="btn btn-primary" onClick={submit as unknown as React.MouseEventHandler} disabled={createNode.isPending} aria-label="Simpan node">
            <Check size={16} /> Simpan
          </button>
        </div>
      </div>
    </div>,
    document.body,
  );
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `npm run test -- src/components/planner/AddPlanNodeDialog.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/planner/AddPlanNodeDialog.tsx frontend/src/components/planner/AddPlanNodeDialog.test.tsx
git commit -m "feat(planner-web): AddPlanNodeDialog (category root / instrument-group child)"
```

---

## Task 5: Rewrite `PlannerPage` around the tree

**Files:**
- Modify: `frontend/src/pages/PlannerPage.tsx`
- Test: `frontend/src/pages/PlannerPage.test.tsx`

**Interfaces:**
- Consumes: `usePlanTree`, `type PlanNodeAllocation` from `../api/hooks`; `PlanTreeNode`; `AddPlanNodeDialog`; `siblingTargetSum` from `../lib/plan-tree`; `QueryState`.
- Replaces the flat-category UI (`TargetEditor`, category cards, add-category dialog) entirely. Keeps the page header and the top-level "target sum" indicator (now computed from tree roots via `siblingTargetSum`). A "Tambah Kelas Aset" button opens `AddPlanNodeDialog` with `parent=null`; each node's "+" opens it with that node as parent.

- [ ] **Step 1: Write the failing test**

Create `frontend/src/pages/PlannerPage.test.tsx`:

```typescript
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import PlannerPage from "./PlannerPage";
import * as hooks from "../api/hooks";
import type { PlanNodeAllocation } from "../api/schemas";

vi.mock("../api/hooks");

function node(partial: Partial<PlanNodeAllocation> & { id: number; name: string }): PlanNodeAllocation {
  return {
    bind_kind: "category", target_pct: "60", tolerance_band_pct: "5",
    actual_pct: "50", actual_value_idr: "100", target_value_idr: "120",
    drift_pct: "-10", out_of_band: false, rebalance_idr: "0", color: null,
    children: [], ...partial,
  };
}

beforeEach(() => {
  vi.mocked(hooks.useUpdatePlanNode).mockReturnValue({ mutate: vi.fn(), isPending: false } as any);
  vi.mocked(hooks.useDeletePlanNode).mockReturnValue({ mutate: vi.fn(), isPending: false } as any);
  vi.mocked(hooks.useCreatePlanNode).mockReturnValue({ mutate: vi.fn(), isPending: false } as any);
  vi.mocked(hooks.useCreateCategory).mockReturnValue({ mutate: vi.fn(), isPending: false } as any);
  vi.mocked(hooks.useCategories).mockReturnValue({ data: [] } as any);
  vi.mocked(hooks.useInstruments).mockReturnValue({ data: [] } as any);
  vi.mocked(hooks.usePlanNodes).mockReturnValue({ data: [] } as any);
});

describe("PlannerPage", () => {
  it("renders the tree roots and a sibling target-sum indicator", () => {
    vi.mocked(hooks.usePlanTree).mockReturnValue({
      data: [node({ id: 1, name: "Saham IDX", target_pct: "60" }), node({ id: 2, name: "Kas", target_pct: "30" })],
      isLoading: false, error: null,
    } as any);
    render(<PlannerPage />);
    expect(screen.getByText("Saham IDX")).toBeInTheDocument();
    expect(screen.getByText("Kas")).toBeInTheDocument();
    // 60 + 30 = 90% sibling sum
    expect(screen.getByText(/90/)).toBeInTheDocument();
  });

  it("opens the add-root dialog from the header button", () => {
    vi.mocked(hooks.usePlanTree).mockReturnValue({ data: [], isLoading: false, error: null } as any);
    render(<PlannerPage />);
    fireEvent.click(screen.getByRole("button", { name: /tambah kelas aset/i }));
    expect(screen.getByText("Tambah Kelas Aset")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm run test -- src/pages/PlannerPage.test.tsx`
Expected: FAIL — `usePlanTree`/tree rendering not present in `PlannerPage` (old page renders categories).

- [ ] **Step 3: Rewrite the page**

Replace the entire contents of `frontend/src/pages/PlannerPage.tsx` with:

```typescript
import { useState } from "react";
import { Plus, CheckCircle, AlertTriangle } from "lucide-react";
import { usePlanTree, type PlanNodeAllocation } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { siblingTargetSum } from "../lib/plan-tree";
import { PlanTreeNode } from "../components/planner/PlanTreeNode";
import { AddPlanNodeDialog } from "../components/planner/AddPlanNodeDialog";

function ProgressBar({ value, color = "hsl(var(--primary))" }: { value: number; color?: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div className="progress">
      <span style={{ width: `${pct}%`, background: color }} />
    </div>
  );
}

export default function PlannerPage() {
  const tree = usePlanTree();
  const [addParent, setAddParent] = useState<PlanNodeAllocation | null>(null);
  const [addOpen, setAddOpen] = useState(false);

  const roots = tree.data ?? [];
  const totalTarget = siblingTargetSum(roots);
  const sumOk = Math.abs(totalTarget - 100) <= 0.01;

  const openAddRoot = () => { setAddParent(null); setAddOpen(true); };
  const openAddChild = (parent: PlanNodeAllocation) => { setAddParent(parent); setAddOpen(true); };

  return (
    <div>
      {/* Page header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 12 }}>
        <div>
          <h1 className="t-h1">Planner</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Struktur alokasi bertingkat &amp; batas toleransi</div>
        </div>
        <button type="button" className="btn btn-primary btn-sm" onClick={openAddRoot} aria-label="Tambah kelas aset">
          <Plus size={15} />
          Tambah Kelas Aset
        </button>
      </div>

      {/* Top-level target-sum indicator */}
      <div className="card card-pad flex items-center" style={{ marginBottom: 18, gap: 16, flexWrap: "wrap" }}>
        <div className="flex items-center" style={{ gap: 12, flex: 1 }}>
          <span
            className="flex items-center justify-center"
            style={{
              width: 40, height: 40, borderRadius: 11, flexShrink: 0,
              background: sumOk ? "hsl(var(--gain-soft))" : "hsl(var(--warn-soft))",
              color: sumOk ? "hsl(var(--gain))" : "hsl(var(--warn))",
            }}
          >
            {sumOk ? <CheckCircle size={20} /> : <AlertTriangle size={20} />}
          </span>
          <div>
            <div className="t-h3">Total target kelas aset {totalTarget.toFixed(1)}%</div>
            <div className="t-sm t-muted">
              {sumOk
                ? "Seimbang — target berjumlah tepat 100%."
                : `Perlu disesuaikan ${100 - totalTarget > 0 ? "+" : ""}${(100 - totalTarget).toFixed(1)}% agar mencapai 100%.`}
            </div>
          </div>
        </div>
        <div style={{ width: 200 }}>
          <ProgressBar value={Math.min(totalTarget, 100)} color={sumOk ? "hsl(var(--gain))" : "hsl(var(--warn))"} />
        </div>
      </div>

      <QueryState isLoading={tree.isLoading} error={tree.error}>
        {roots.length === 0 ? (
          <div className="card">
            <div className="empty">
              <div className="empty-icon">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="2"/>
                </svg>
              </div>
              <div>
                <div className="t-h3">Belum ada kelas aset</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Tambahkan kelas aset untuk menyusun struktur alokasi portofolio.
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="card card-pad">
            {roots.map((n) => (
              <PlanTreeNode key={n.id} node={n} depth={0} onAddChild={openAddChild} />
            ))}
          </div>
        )}
      </QueryState>

      <AddPlanNodeDialog open={addOpen} parent={addParent} onClose={() => setAddOpen(false)} />
    </div>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm run test -- src/pages/PlannerPage.test.tsx`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/src/pages/PlannerPage.tsx frontend/src/pages/PlannerPage.test.tsx
git commit -m "refactor(planner-web): rewrite PlannerPage around the allocation tree"
```

---

## Task 6: Drive `DashboardPage` allocation cards from the tree

**Files:**
- Modify: `frontend/src/pages/DashboardPage.tsx`
- Test: `frontend/src/pages/DashboardPage.test.tsx` (create; the existing `DashboardPage.test.tsx` may already exist — if so, ADD the case to it rather than overwriting)

**Interfaces:**
- Consumes: `usePlanTree` from `../api/hooks`; `treeRootsToAllocation` from `../lib/plan-tree`.
- Replaces the four `summary.data.allocation` reads (in `AlokasiCard`, `DriftCard`, `RebalancingCard`, and the `outOfBandCount`/`activeCategories` derivations) with `treeRootsToAllocation(planTree.data ?? [])`. The donut/drift loading state keys off `planTree.isLoading`. No other dashboard cards change.

- [ ] **Step 1: Write the failing test**

Check whether `frontend/src/pages/DashboardPage.test.tsx` exists. If it does, add the test below to it (adjusting the mock setup to match its style). If not, create it:

```typescript
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect } from "vitest";
import DashboardPage from "./DashboardPage";

// Relies on the MSW handlers in src/test/server.ts, including GET /api/plan/tree
// (added in Task 1) whose root "Saham IDX" has actual_value_idr 2437500.
function renderDashboard() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("DashboardPage allocation source", () => {
  it("renders the asset-allocation card driven by the plan tree", async () => {
    renderDashboard();
    expect(await screen.findByText("Alokasi Aset")).toBeInTheDocument();
    // "Saham IDX" comes from the plan-tree handler, not the flat summary allocation.
    expect(await screen.findByText("Saham IDX")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm run test -- src/pages/DashboardPage.test.tsx`
Expected: FAIL — the dashboard still reads `summary.data.allocation` (the MSW summary handler has no "Saham IDX" category), so "Saham IDX" is not found.

- [ ] **Step 3: Wire the dashboard to the tree**

In `frontend/src/pages/DashboardPage.tsx`:

1. Add `usePlanTree` to the existing `from "../api/hooks"` import and `treeRootsToAllocation` from `../lib/plan-tree`:

```typescript
import { useSummary, useHistory, useInsights, useGoals, useRefreshPrices,
  useMovers, useBenchmark, useReviewItems, useTransactions, useInstruments, usePlanTree } from "../api/hooks";
import { treeRootsToAllocation } from "../lib/plan-tree";
```

2. Inside `DashboardPage()`, after the other hook calls (e.g. after `const instruments = useInstruments();`), add:

```typescript
  const planTree = usePlanTree();
  const allocation = treeRootsToAllocation(planTree.data ?? []);
```

3. Replace the allocation derivations that currently read `summary.data?.allocation`:

```typescript
  const outOfBandCount = allocation.filter((c) => c.out_of_band).length;
  const activeCategories = allocation.filter(
    (c) => Number(c.actual_value_idr) > 0 && c.category_id !== UNCATEGORIZED_CATEGORY_ID,
  ).length;
```

4. In the JSX, replace the three card usages to use `allocation` + `planTree.isLoading`:

- Alokasi card block:
```typescript
        {planTree.isLoading ? (
          <CardSkeleton rows={3} height={12} />
        ) : (
          <AlokasiCard allocation={allocation} loading={false} />
        )}
```
- Drift card block:
```typescript
        {planTree.isLoading ? (
          <CardSkeleton rows={4} height={30} />
        ) : (
          <DriftCard allocation={allocation} loading={false} />
        )}
```
- Rebalancing card block:
```typescript
        {planTree.isLoading ? (
          <CardSkeleton rows={3} height={44} />
        ) : (
          <RebalancingCard allocation={allocation} />
        )}
```

Leave every other card (Hero, Health, Komposisi, Movers, TopHoldings, Activity, Benchmark, Goals) reading their existing sources — only the allocation-derived cards switch to the tree.

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm run test -- src/pages/DashboardPage.test.tsx`
Expected: PASS.

- [ ] **Step 5: Full frontend test suite + typecheck/build**

Run: `npm run test`
Expected: PASS (whole suite, including the pre-existing dashboard tests).

Run: `npm run build`
Expected: `tsc -b` typechecks with no errors, Vite build succeeds.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/pages/DashboardPage.tsx frontend/src/pages/DashboardPage.test.tsx
git commit -m "refactor(planner-web): drive dashboard allocation cards from the plan tree"
```

---

## Done criteria (Phase 2)

- The Planner page shows the allocation tree: expandable rows, per-node drift badge + rebalance hint, inline target editing, add (root = category, child = instrument/group) and delete, synthetic "Lainnya" rows non-editable, and a top-level target-sum indicator. The flat category cards are gone.
- The dashboard's Alokasi / Target-vs-Aktual / Rebalancing cards are driven by the tree's top level — a single source of allocation truth, so editing tree targets no longer diverges from the dashboard.
- `npm run test` and `npm run build` are green.

## Deliberately deferred (note to reviewer — not gaps)

- **Move/reorder UI** (drag or up/down): backend `POST /plan/nodes/:id/move` exists, but no UI ships this phase. YAGNI for the core "sub-planner + goals" value.
- **Category deletion / editing UI**: the flat page used to allow deleting categories; this phase only adds category creation (inline, via the add-root dialog). Category catalog management can return in a later phase if needed.
- **Over-allocation indicator** (an instrument/category bound by more than one node → double-count): the dialog filters already-bound categories/instruments to prevent it at creation time; a server-side warning is a future-phase nicety flagged by the Phase 1 final review.
- **Phase 3/4** (goals backend + goals UI) are separate plans.
```
