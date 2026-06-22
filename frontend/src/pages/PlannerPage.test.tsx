import { render, screen, fireEvent, within } from "@testing-library/react";
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
    expect(within(screen.getByRole("dialog")).getByText("Tambah Kelas Aset")).toBeInTheDocument();
  });
});
