import { render, screen, waitFor } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi } from "vitest";
import { DashboardAgendaCard } from "./DashboardAgendaCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardAgendaCard /></MemoryRouter>);
}

describe("DashboardAgendaCard", () => {
  it("renders upcoming events with a Google badge", async () => {
    vi.mocked(hooks.useEvents).mockReturnValue({
      data: [
        { id: 1, title: "Standup", location: null, notes: null, start_at: "2026-06-13T02:00:00Z", status: "scheduled", source: "local", google_event_id: null },
        { id: 2, title: "Dokter", location: null, notes: null, start_at: "2026-06-13T07:00:00Z", status: "scheduled", source: "google", google_event_id: "g-1" },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Standup")).toBeInTheDocument());
    expect(screen.getByText("Google")).toBeInTheDocument();
  });

  it("shows an empty state when there are no events", async () => {
    vi.mocked(hooks.useEvents).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/belum ada agenda/i)).toBeInTheDocument());
  });
});
