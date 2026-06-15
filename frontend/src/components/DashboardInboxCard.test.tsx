import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { DashboardInboxCard } from "./DashboardInboxCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardInboxCard /></MemoryRouter>);
}

describe("DashboardInboxCard", () => {
  const resolveMutate = vi.fn();
  beforeEach(() => {
    resolveMutate.mockReset();
    vi.mocked(hooks.useResolveInbox).mockReturnValue({ mutate: resolveMutate, isPending: false } as any);
  });

  it("renders pending inbox items", async () => {
    vi.mocked(hooks.useInbox).mockReturnValue({
      data: [
        { id: 1, content: "Ide produk baru", status: "pending", created_at: "2026-06-15T08:00:00+07:00", resolved_at: null },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Ide produk baru")).toBeInTheDocument());
  });

  it("shows an empty state when the inbox is clear", async () => {
    vi.mocked(hooks.useInbox).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/inbox kosong/i)).toBeInTheDocument());
  });

  it("resolves an inbox item when its done button is clicked", async () => {
    vi.mocked(hooks.useInbox).mockReturnValue({
      data: [{ id: 4, content: "Ide produk baru", status: "pending", created_at: "2026-06-15T08:00:00+07:00", resolved_at: null }],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    fireEvent.click(screen.getByLabelText("Selesaikan Ide produk baru"));
    expect(resolveMutate).toHaveBeenCalledWith(4, expect.anything());
  });
});
