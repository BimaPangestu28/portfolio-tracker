import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { TodoTab } from "./TodoTab";
import * as hooks from "../../api/hooks";

vi.mock("../../api/hooks");

describe("TodoTab", () => {
  const completeMutate = vi.fn();
  const reopenMutate = vi.fn();
  const createMutate = vi.fn();
  const updateMutate = vi.fn();
  beforeEach(() => {
    [completeMutate, reopenMutate, createMutate, updateMutate].forEach((m) => m.mockReset());
    vi.mocked(hooks.useCompleteTodo).mockReturnValue({ mutate: completeMutate, isPending: false } as any);
    vi.mocked(hooks.useReopenTodo).mockReturnValue({ mutate: reopenMutate, isPending: false } as any);
    vi.mocked(hooks.useCreateTodo).mockReturnValue({ mutate: createMutate, isPending: false } as any);
    vi.mocked(hooks.useUpdateTodo).mockReturnValue({ mutate: updateMutate, isPending: false } as any);
    vi.mocked(hooks.useTodos).mockReturnValue({
      data: [{ id: 7, title: "Bayar internet", notes: null, due_at: null, status: "open", created_at: "2026-06-15T08:00:00+07:00", completed_at: null, priority: null, estimate_minutes: null }],
      isLoading: false, isError: false,
    } as any);
  });

  it("changes the status filter (calls useTodos with the chosen status)", () => {
    render(<TodoTab />);
    fireEvent.click(screen.getByRole("button", { name: "Selesai" }));
    expect(hooks.useTodos).toHaveBeenCalledWith("done");
  });

  it("completes a todo", () => {
    render(<TodoTab />);
    fireEvent.click(screen.getByLabelText("Selesaikan Bayar internet"));
    expect(completeMutate).toHaveBeenCalledWith(7, expect.anything());
  });
});
