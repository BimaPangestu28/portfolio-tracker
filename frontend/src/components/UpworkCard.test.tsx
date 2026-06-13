import { render, screen, waitFor } from "@testing-library/react";
import { describe, it, expect, vi } from "vitest";
import UpworkCard from "./UpworkCard";
import { api } from "../api/client";

vi.mock("../api/client", () => ({
  api: {
    upworkStatus: vi.fn(),
    upworkStart: vi.fn(),
    upworkSync: vi.fn(),
    upworkDisconnect: vi.fn(),
  },
}));

describe("UpworkCard", () => {
  it("shows Connect when disconnected", async () => {
    (api.upworkStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "disconnected", last_error: null });
    render(<UpworkCard />);
    await waitFor(() => expect(screen.getByRole("button", { name: /hubungkan upwork/i })).toBeInTheDocument());
  });

  it("shows the error reason when status is error", async () => {
    (api.upworkStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "error", last_error: "invalid_token" });
    render(<UpworkCard />);
    await waitFor(() => expect(screen.getByText(/invalid_token/i)).toBeInTheDocument());
  });

  it("shows Sync now and Disconnect when connected", async () => {
    (api.upworkStatus as ReturnType<typeof vi.fn>).mockResolvedValue({ status: "connected", last_error: null });
    render(<UpworkCard />);
    await waitFor(() => {
      expect(screen.getByRole("button", { name: /sync now/i })).toBeInTheDocument();
      expect(screen.getByRole("button", { name: /putuskan/i })).toBeInTheDocument();
    });
  });
});
