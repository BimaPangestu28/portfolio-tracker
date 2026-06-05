import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent, within } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import TransactionsPage from "./TransactionsPage";

const tx = (id: number, instrument_id: number, txn_type: string, executed_at: string) => ({
  id, account_id: 1, instrument_id, txn_type, executed_at,
  quantity: "700", price_native: "4090", fee_native: "0",
  currency: "IDR", fx_to_idr: "1", fx_to_usd: "1", note: null,
});

function seedNamedData() {
  server.use(
    http.get("/api/transactions", () =>
      HttpResponse.json([
        tx(1, 7, "buy", "2026-06-03T17:00:00Z"),
        tx(2, 7, "dividend", "2026-05-07T00:00:00Z"),
        tx(3, 8, "buy", "2026-04-15T00:00:00Z"),
      ]),
    ),
    http.get("/api/instruments", () =>
      HttpResponse.json([
        { id: 7, symbol: "BBRI", name: "Bank Rakyat Indonesia", instrument_type: "stock_id", native_currency: "IDR", category_id: null, price_source: "yahoo:BBRI.JK", decimals: 0, note: null },
        { id: 8, symbol: "BBCA", name: "Bank Central Asia", instrument_type: "stock_id", native_currency: "IDR", category_id: null, price_source: "yahoo:BBCA.JK", decimals: 0, note: null },
      ]),
    ),
    http.get("/api/accounts", () =>
      HttpResponse.json([
        { id: 1, name: "Stockbit", account_type: "manual", institution: null, native_currency: "IDR", note: null, created_at: "2026-06-04T00:00:00Z" },
      ]),
    ),
  );
}

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("renders Tambah Transaksi button and empty state", async () => {
  render(<TransactionsPage />, { wrapper });
  // Header button should always be present
  expect(screen.getByRole("button", { name: /tambah transaksi/i })).toBeInTheDocument();
  // Wait for empty state
  await waitFor(() => expect(screen.getByText(/Belum ada transaksi/)).toBeInTheDocument());
});

test("shows instrument symbol and account name instead of raw ids", async () => {
  seedNamedData();
  render(<TransactionsPage />, { wrapper });
  await waitFor(() => expect(screen.getByRole("table")).toBeInTheDocument());
  const table = within(screen.getByRole("table"));
  expect(table.getAllByText("BBRI").length).toBeGreaterThan(0);
  expect(table.getAllByText("BBCA").length).toBeGreaterThan(0);
  expect(table.getAllByText("Stockbit").length).toBeGreaterThan(0);
  expect(table.queryByText("#7")).not.toBeInTheDocument();
  expect(table.queryByText("#1")).not.toBeInTheDocument();
});

test("filters rows by type and instrument", async () => {
  seedNamedData();
  render(<TransactionsPage />, { wrapper });
  await waitFor(() => expect(screen.getByRole("table")).toBeInTheDocument());
  const table = () => within(screen.getByRole("table"));

  // Filter tipe: Dividen -> only the dividend row remains
  fireEvent.change(screen.getByLabelText("Filter tipe"), { target: { value: "dividend" } });
  await waitFor(() => expect(table().queryByText("BBCA")).not.toBeInTheDocument());
  expect(table().getAllByText("BBRI").length).toBeGreaterThan(0);

  // Reset tipe, filter instrumen: BBCA -> only the BBCA row remains
  fireEvent.change(screen.getByLabelText("Filter tipe"), { target: { value: "" } });
  fireEvent.change(screen.getByLabelText("Filter instrumen"), { target: { value: "8" } });
  await waitFor(() => expect(table().queryByText("BBRI")).not.toBeInTheDocument());
  expect(table().getAllByText("BBCA").length).toBeGreaterThan(0);
});

test("opens dialog when Tambah Transaksi is clicked", async () => {
  render(<TransactionsPage />, { wrapper });
  const btn = screen.getByRole("button", { name: /tambah transaksi/i });
  fireEvent.click(btn);
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  // Dialog title appears inside the dialog element
  expect(screen.getByRole("dialog")).toHaveTextContent(/Tambah Transaksi/);
});

test("dialog has aria-label inputs", async () => {
  render(<TransactionsPage />, { wrapper });
  fireEvent.click(screen.getByRole("button", { name: /tambah transaksi/i }));
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  expect(screen.getByLabelText("Jumlah")).toBeInTheDocument();
  expect(screen.getByLabelText("Harga")).toBeInTheDocument();
});
