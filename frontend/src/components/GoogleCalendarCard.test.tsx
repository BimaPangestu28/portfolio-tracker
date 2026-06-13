import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import GoogleCalendarCard from "./GoogleCalendarCard";
import { api } from "../api/client";

vi.mock("../api/client", () => ({
  api: { googleStatus: vi.fn(), googleStart: vi.fn(), googleSync: vi.fn(), googleDisconnect: vi.fn() },
}));

const mockFn = (f: unknown) => f as ReturnType<typeof vi.fn>;

describe("GoogleCalendarCard", () => {
  it("shows Connect when disconnected", async () => {
    mockFn(api.googleStatus).mockResolvedValue({ status: "disconnected", last_error: null, last_synced_at: null });
    render(<GoogleCalendarCard />);
    await waitFor(() => expect(screen.getByRole("button", { name: /hubungkan/i })).toBeInTheDocument());
  });

  it("shows the error reason when status is error", async () => {
    mockFn(api.googleStatus).mockResolvedValue({ status: "error", last_error: "invalid_grant", last_synced_at: null });
    render(<GoogleCalendarCard />);
    await waitFor(() => expect(screen.getByText(/invalid_grant/i)).toBeInTheDocument());
  });

  it("when connected, shows last-synced + a Sync now button that runs a sync", async () => {
    mockFn(api.googleStatus).mockResolvedValue({ status: "connected", last_error: null, last_synced_at: "2026-06-13T03:00:00Z" });
    mockFn(api.googleSync).mockResolvedValue({ status: "connected", last_error: null, last_synced_at: "2026-06-13T04:00:00Z", pushed: 2, imported: 1 });
    render(<GoogleCalendarCard />);
    const btn = await screen.findByRole("button", { name: /sync sekarang/i });
    expect(screen.getByText(/terakhir sync/i)).toBeInTheDocument();
    btn.click();
    await waitFor(() => expect(api.googleSync).toHaveBeenCalled());
  });
});
