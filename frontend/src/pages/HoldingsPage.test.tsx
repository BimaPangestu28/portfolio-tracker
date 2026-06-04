import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import HoldingsPage from "./HoldingsPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("shows empty state when no positions", async () => {
  render(<HoldingsPage />, { wrapper });
  await waitFor(() => expect(screen.getByText(/Belum ada posisi/)).toBeInTheDocument());
});

test("opens add transaction dialog when Tambah is clicked", async () => {
  render(<HoldingsPage />, { wrapper });
  fireEvent.click(screen.getByRole("button", { name: /tambah holding/i }));
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  expect(screen.getByRole("dialog")).toHaveTextContent(/Tambah Transaksi/);
});

test("formats native-currency fields (avg cost, latest price, P&L) in IDR for an IDX stock", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "39600000",
        net_worth_usd: "2475",
        total_unrealized_pnl_idr: "0",
        total_realized_pnl_idr: "0",
        xirr: null,
        positions: [
          {
            instrument_id: 1,
            quantity: "10000",
            avg_cost: "3960",
            cost_basis_total: "39600000",
            latest_price: "4200",
            price_stale: false,
            market_value_native: "42000000",
            market_value_idr: "42000000",
            market_value_usd: "2625",
            unrealized_pnl: "2400000",
            realized_pnl: "0",
            income: "0",
          },
        ],
        allocation: [],
      }),
    ),
    http.get("/api/instruments", () =>
      HttpResponse.json([
        {
          id: 1,
          symbol: "BMRI",
          name: "Bank Mandiri",
          instrument_type: "stock",
          native_currency: "IDR",
          category_id: null,
          price_source: "idx",
          decimals: 0,
          note: null,
        },
      ]),
    ),
  );

  render(<HoldingsPage />, { wrapper });

  // avg_cost 3960 in native IDR should render as "Rp 3.960", not "$3,960.00"
  const avgCostCell = await screen.findByText(/Rp\s*3\.960/);
  expect(avgCostCell).toBeInTheDocument();
  // latest_price and unrealized P&L are also native-currency (IDR here)
  expect(screen.getByText(/Rp\s*4\.200/)).toBeInTheDocument();
  expect(screen.getByText(/Rp\s*2\.400\.000/)).toBeInTheDocument();
  // No USD formatting should leak into the native-currency columns
  expect(screen.queryByText(/\$3,960/)).not.toBeInTheDocument();
  expect(screen.queryByText(/\$4,200/)).not.toBeInTheDocument();
});

test("formats native-currency fields in USD for a USD-denominated instrument", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "16000000",
        net_worth_usd: "1000",
        total_unrealized_pnl_idr: "0",
        total_realized_pnl_idr: "0",
        xirr: null,
        positions: [
          {
            instrument_id: 2,
            quantity: "5",
            avg_cost: "150",
            cost_basis_total: "750",
            latest_price: "200",
            price_stale: false,
            market_value_native: "1000",
            market_value_idr: "16000000",
            market_value_usd: "1000",
            unrealized_pnl: "250",
            realized_pnl: "0",
            income: "0",
          },
        ],
        allocation: [],
      }),
    ),
    http.get("/api/instruments", () =>
      HttpResponse.json([
        {
          id: 2,
          symbol: "AAPL",
          name: "Apple Inc.",
          instrument_type: "stock",
          native_currency: "USD",
          category_id: null,
          price_source: "yahoo",
          decimals: 2,
          note: null,
        },
      ]),
    ),
  );

  render(<HoldingsPage />, { wrapper });

  expect(await screen.findByText("$150.00")).toBeInTheDocument();
  expect(screen.getByText("$200.00")).toBeInTheDocument();
});

test("renders the Holdings page header and table headers", async () => {
  render(<HoldingsPage />, { wrapper });
  expect(screen.getByText("Holdings")).toBeInTheDocument();
  await waitFor(() => {
    // Empty state OR table — both are valid when backend returns empty
    const emptyState = screen.queryByText(/Belum ada posisi/);
    const instrHeader = screen.queryByText("Instrumen");
    expect(emptyState || instrHeader).toBeTruthy();
  });
});
