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
