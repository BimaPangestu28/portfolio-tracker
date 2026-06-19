import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import ImportPage from "./ImportPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

test("shows upload control and info banner", async () => {
  render(<ImportPage />, { wrapper });
  // Info banner
  expect(screen.getByText(/tidak ada yang otomatis/i)).toBeInTheDocument();
  // Upload label
  expect(screen.getByText(/pilih berkas/i)).toBeInTheDocument();
  // File input type=file
  expect(screen.getByLabelText(/pilih berkas/i)).toHaveAttribute("type", "file");
});

test("shows empty pending state with queue header", async () => {
  render(<ImportPage />, { wrapper });
  await waitFor(() => expect(screen.getByText(/No pending items/i)).toBeInTheDocument());
  expect(screen.getByText(/Antrian Review/)).toBeInTheDocument();
});

test("amount-only fund item shows amount, hint, and confirms with amount_native", async () => {
  const reviewItem = {
    id: 54, batch_id: "b", source_kind: "image", source_filename: "asdc.jpeg",
    source_path: "p", doc_type: "txn_history", status: "pending", needs_attention: 0,
    payload_json: JSON.stringify({
      entry_type: "buy",
      instrument_name: "Sucorinvest Bond Fund",
      amount_native: "13000000",
      currency: "IDR",
      account_hint: "Pendidikan Noah",
      confidence: 0.72,
    }),
    raw_llm_json: "{}",
    suggested_instrument_id: 7,
    suggested_account_id: 3,
    created_txn_id: null,
    created_at: "2026-06-05T06:37:50Z",
    confirmed_at: null,
  };
  let confirmBody: Record<string, unknown> | null = null;
  server.use(
    http.get("/api/ingest/review", () => HttpResponse.json([reviewItem])),
    http.get("/api/instruments", () => HttpResponse.json([{
      id: 7, symbol: "SBF", name: "Sucorinvest Bond Fund", instrument_type: "mutual_fund",
      native_currency: "IDR", category_id: null, price_source: "manual", decimals: 4, note: null,
    }])),
    http.get("/api/accounts", () => HttpResponse.json([{
      id: 3, name: "Pendidikan Noah", account_type: "manual", institution: null,
      native_currency: "IDR", note: null, created_at: "2026-01-01T00:00:00Z",
    }])),
    http.post("/api/ingest/review/:id/confirm", async ({ request }) => {
      confirmBody = (await request.json()) as Record<string, unknown>;
      return HttpResponse.json({ created_txn_id: 99 });
    }),
  );

  render(<ImportPage />, { wrapper });

  // Amount field prefilled from the payload — displayed with thousand separators
  expect(await screen.findByLabelText("Amount")).toHaveValue("13.000.000");
  // amount-only hint shown because quantity & price are empty
  expect(screen.getByText(/amount-only/i)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: /konfirmasi item ini/i }));

  await waitFor(() => expect(confirmBody).not.toBeNull());
  expect(confirmBody!.amount_native).toBe("13000000");
  expect(confirmBody!.quantity).toBe("");
  expect(confirmBody!.price_native).toBe("");
});
