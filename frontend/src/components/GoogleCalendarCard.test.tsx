import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import GoogleCalendarCard from "./GoogleCalendarCard";
import { api } from "../api/client";

vi.mock("../api/client", () => ({
  api: { googleStatus: vi.fn(), googleStart: vi.fn(), googleDisconnect: vi.fn() },
}));

describe("GoogleCalendarCard", () => {
  it("shows Connect when disconnected", async () => {
    (api.googleStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "disconnected", last_error: null });
    render(<GoogleCalendarCard />);
    await waitFor(() => expect(screen.getByRole("button", { name: /hubungkan/i })).toBeInTheDocument());
  });

  it("shows the error reason when status is error", async () => {
    (api.googleStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "error", last_error: "invalid_grant" });
    render(<GoogleCalendarCard />);
    await waitFor(() => expect(screen.getByText(/invalid_grant/i)).toBeInTheDocument());
  });
});
