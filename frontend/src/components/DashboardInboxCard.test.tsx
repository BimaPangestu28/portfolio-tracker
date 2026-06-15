import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardInboxCard } from "./DashboardInboxCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardInboxCard /></MemoryRouter>);
}

describe("DashboardInboxCard", () => {
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
});
