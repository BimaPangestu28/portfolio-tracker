import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { DashboardTodoCard } from "./DashboardTodoCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardTodoCard /></MemoryRouter>);
}

describe("DashboardTodoCard", () => {
  const completeMutate = vi.fn();
  const createMutate = vi.fn();
  beforeEach(() => {
    completeMutate.mockReset();
    createMutate.mockReset();
    vi.mocked(hooks.useCompleteTodo).mockReturnValue({ mutate: completeMutate, isPending: false } as any);
    vi.mocked(hooks.useCreateTodo).mockReturnValue({ mutate: createMutate, isPending: false } as any);
  });

  it("completes a todo when its check button is clicked", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({
      data: [{ id: 7, title: "Bayar internet", notes: null, due_at: null, status: "open", created_at: "2026-06-15T08:00:00+07:00", completed_at: null, priority: null, estimate_minutes: null }],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    fireEvent.click(screen.getByLabelText("Selesaikan Bayar internet"));
    expect(completeMutate).toHaveBeenCalledWith(7, expect.anything());
  });

  it("creates a todo from the quick-add form", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    fireEvent.change(screen.getByLabelText("Tambah todo"), { target: { value: "Beli kopi" } });
    fireEvent.click(screen.getByLabelText("Simpan todo baru"));
    expect(createMutate).toHaveBeenCalledWith({ title: "Beli kopi" }, expect.anything());
  });

  it("renders open todos", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({
      data: [
        { id: 1, title: "Bayar internet", notes: null, due_at: null, status: "open", created_at: "2026-06-15T08:00:00+07:00", completed_at: null, priority: "high", estimate_minutes: null },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Bayar internet")).toBeInTheDocument());
  });

  it("shows an empty state when there are no todos", async () => {
    vi.mocked(hooks.useTodos).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/tidak ada todo/i)).toBeInTheDocument());
  });
});
