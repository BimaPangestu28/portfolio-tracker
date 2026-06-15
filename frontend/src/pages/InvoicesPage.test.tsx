import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import InvoicesPage from "./InvoicesPage";
import * as hooks from "../api/hooks";
import { api } from "../api/client";

vi.mock("../api/hooks");
vi.mock("../api/client", () => ({ api: { getBlob: vi.fn() } }));

const invoice = {
  id: 1, number: "INV/2026/VI/001", client_id: 3, issue_date: "2026-06-11", due_date: "2026-06-25",
  subtotal: "Rp 12.000.000", total: "Rp 12.000.000",
  line_items_json: '[{"title":"Landing","body":null,"qty":1,"amount":12000000}]',
  created_at: "2026-06-11T08:00:00Z",
};

describe("InvoicesPage", () => {
  beforeEach(() => {
    vi.mocked(hooks.useInvoices).mockReturnValue({ data: [invoice], isLoading: false, isError: false } as any);
    vi.mocked(hooks.useClients).mockReturnValue({ data: [{ id: 3, name: "PT AIS", sub_name: null, website: null, created_at: "" }], isLoading: false, isError: false } as any);
    vi.mocked(hooks.useInvoice).mockReturnValue({ data: invoice, isLoading: false, isError: false } as any);
    vi.mocked(api.getBlob).mockReset();
    vi.mocked(api.getBlob).mockResolvedValue(new Blob(["%PDF"], { type: "application/pdf" }));
    // jsdom does not implement the object-URL APIs the download path relies on.
    URL.createObjectURL = vi.fn(() => "blob:mock");
    URL.revokeObjectURL = vi.fn();
  });

  it("lists invoices with the client name", () => {
    render(<InvoicesPage />);
    expect(screen.getByText("INV/2026/VI/001")).toBeInTheDocument();
    expect(screen.getAllByText("PT AIS").length).toBeGreaterThan(0);
  });

  it("shows detail and downloads the PDF", async () => {
    render(<InvoicesPage />);
    fireEvent.click(screen.getByText("INV/2026/VI/001"));
    fireEvent.click(screen.getByRole("button", { name: /download pdf/i }));
    await waitFor(() => expect(api.getBlob).toHaveBeenCalledWith("/invoices/1/pdf"));
  });
});
