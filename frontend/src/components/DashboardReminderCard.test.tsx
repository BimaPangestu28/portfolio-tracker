import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { DashboardReminderCard } from "./DashboardReminderCard";
import * as hooks from "../api/hooks";

vi.mock("../api/hooks");

function renderCard() {
  return render(<MemoryRouter><DashboardReminderCard /></MemoryRouter>);
}

describe("DashboardReminderCard", () => {
  const cancelMutate = vi.fn();
  beforeEach(() => {
    cancelMutate.mockReset();
    vi.mocked(hooks.useCancelReminder).mockReturnValue({ mutate: cancelMutate, isPending: false } as any);
  });

  it("cancels a reminder when its cancel button is clicked", async () => {
    vi.mocked(hooks.useReminders).mockReturnValue({
      data: [{ id: 9, todo_id: null, message: "Meeting jam 3", remind_at: "2026-06-15T15:00:00+07:00", recurrence: "none", status: "pending", sent_at: null, event_id: null }],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    fireEvent.click(screen.getByLabelText("Batalkan Meeting jam 3"));
    expect(cancelMutate).toHaveBeenCalledWith(9, expect.anything());
  });

  it("renders pending reminders", async () => {
    vi.mocked(hooks.useReminders).mockReturnValue({
      data: [
        { id: 1, todo_id: null, message: "Meeting jam 3", remind_at: "2026-06-15T15:00:00+07:00", recurrence: "none", status: "pending", sent_at: null, event_id: null },
      ],
      isLoading: false, isError: false,
    } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText("Meeting jam 3")).toBeInTheDocument());
  });

  it("shows an empty state when there are no reminders", async () => {
    vi.mocked(hooks.useReminders).mockReturnValue({ data: [], isLoading: false, isError: false } as any);
    renderCard();
    await waitFor(() => expect(screen.getByText(/tidak ada reminder/i)).toBeInTheDocument());
  });
});
