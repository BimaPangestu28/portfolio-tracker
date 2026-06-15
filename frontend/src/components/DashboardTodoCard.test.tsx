import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardTodoCard } from "./DashboardTodoCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardTodoCard /></MemoryRouter>);
}

describe("DashboardTodoCard", () => {
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
