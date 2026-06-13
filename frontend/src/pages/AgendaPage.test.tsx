import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import AgendaPage from "./AgendaPage";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

const mockEvents = [
  { id: 1, title: "Meeting vendor", location: "kantor", notes: null, start_at: "2026-06-13T02:00:00Z", status: "scheduled", source: "local", google_event_id: null },
  { id: 2, title: "Dokter gigi", location: null, notes: null, start_at: "2026-06-13T07:00:00Z", status: "scheduled", source: "google", google_event_id: "g-1" },
];

beforeEach(() => {
  vi.mocked(hooks.useEvents).mockReturnValue({ data: mockEvents, isLoading: false, isError: false } as any);
  vi.mocked(hooks.useCreateEvent).mockReturnValue({ mutate: vi.fn() } as any);
  vi.mocked(hooks.useUpdateEvent).mockReturnValue({ mutate: vi.fn() } as any);
  vi.mocked(hooks.useCancelEvent).mockReturnValue({ mutate: vi.fn() } as any);
});

describe("AgendaPage", () => {
  it("shows a day's events when its grid cell is clicked, with a Google badge and no edit on google events", async () => {
    render(<AgendaPage initialDay="2026-06-13" />);
    fireEvent.click(screen.getByText("13"));
    await waitFor(() => expect(screen.getByText("Meeting vendor")).toBeInTheDocument());
    expect(screen.getByText("Dokter gigi")).toBeInTheDocument();
    expect(screen.getByText("Google")).toBeInTheDocument();
    expect(screen.getAllByLabelText("Edit").length).toBe(1);
  });
});
