import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { ReminderTab } from "./ReminderTab";
import * as hooks from "../../api/hooks";

vi.mock("../../api/hooks");

describe("ReminderTab", () => {
  const cancelMutate = vi.fn();
  const createMutate = vi.fn();
  beforeEach(() => {
    cancelMutate.mockReset();
    createMutate.mockReset();
    vi.mocked(hooks.useCancelReminder).mockReturnValue({ mutate: cancelMutate, isPending: false } as any);
    vi.mocked(hooks.useCreateReminder).mockReturnValue({ mutate: createMutate, isPending: false } as any);
    vi.mocked(hooks.useReminders).mockReturnValue({
      data: [{ id: 9, todo_id: null, message: "Meeting jam 3", remind_at: "2026-06-15T15:00:00+07:00", recurrence: "none", status: "pending", sent_at: null, event_id: null }],
      isLoading: false, isError: false,
    } as any);
  });

  it("cancels a pending reminder", () => {
    render(<ReminderTab />);
    fireEvent.click(screen.getByLabelText("Batalkan Meeting jam 3"));
    expect(cancelMutate).toHaveBeenCalledWith(9, expect.anything());
  });

  it("creates a reminder from the form", () => {
    render(<ReminderTab />);
    fireEvent.change(screen.getByLabelText("Pesan reminder"), { target: { value: "Telepon klien" } });
    fireEvent.change(screen.getByLabelText("Waktu reminder"), { target: { value: "2026-06-20T09:00" } });
    fireEvent.click(screen.getByRole("button", { name: "Buat reminder" }));
    expect(createMutate).toHaveBeenCalled();
    expect(createMutate.mock.calls[0][0].message).toBe("Telepon klien");
  });
});
