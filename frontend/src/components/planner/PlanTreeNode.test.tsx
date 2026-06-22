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
