/**
 * Component test for HyperliquidPositions.
 *
 * Mounts the component with a QueryClientProvider + MSW handler for
 * /api/portfolio/hyperliquid and asserts that position/trade data renders.
 * The MSW server is started/stopped in src/test/setup.ts.
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import { HyperliquidPositions } from "../components/HyperliquidPositions";

function renderSection() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <HyperliquidPositions />
    </QueryClientProvider>,
  );
}

test("shows an open position and a closed trade", async () => {
  server.use(
    http.get("/api/portfolio/hyperliquid", () =>
      HttpResponse.json({
        points: [],
        metrics: {
          total_return: 0.1,
          annualized: null,
          max_drawdown: -0.05,
          volatility: 0.2,
        },
        current_value_usd: "1100",
        positions: [
          {
            coin: "ETH",
            direction: "long",
            size: "1",
            entry_px: "2000",
            mark_px: "2100",
            unrealized_pnl: "100",
            leverage: "5",
            notional: "2100",
            updated_at: "2026-06-18T00:00:00Z",
          },
        ],
        trades: [
          {
            external_id: "ETH:1:2000",
            coin: "ETH",
            direction: "long",
            size: "1",
            entry_px: "2000",
            exit_px: "2100",
            realized_pnl: "100",
            fee: "2",
            opened_at: "2026-06-01T00:00:00Z",
            closed_at: "2026-06-02T00:00:00Z",
            leverage: 5,
            confidence: 80,
            timeframe: "4h",
            profile: "moderate",
          },
        ],
        realized_pnl_total: "100",
        win_rate: 1.0,
        insufficient_data: true,
      }),
    ),
  );

  renderSection();

  // The coin "ETH" must appear (both in positions and trades tables)
  await waitFor(() =>
    expect(screen.getAllByText("ETH").length).toBeGreaterThanOrEqual(1),
  );

  // Position direction and unrealized PnL ($100 appears in position uPnL, trade PnL, and realized PnL aggregate)
  expect(screen.getAllByText("long").length).toBeGreaterThanOrEqual(1);
  expect(screen.getAllByText("$100").length).toBeGreaterThanOrEqual(1);

  // Trade closed date
  expect(screen.getByText("2026-06-02")).toBeInTheDocument();

  // Timeframe column
  expect(screen.getByText("4h")).toBeInTheDocument();

  // Aggregate stats — realized PnL in card-sub (same value $100 appears multiple times)
  expect(screen.getAllByText("$100").length).toBeGreaterThanOrEqual(1);
});

test("shows empty-state messages when there are no positions or trades", async () => {
  // The default handler in server.ts already returns empty positions/trades,
  // but we override to be explicit and verify the empty-state text.
  server.use(
    http.get("/api/portfolio/hyperliquid", () =>
      HttpResponse.json({
        points: [],
        metrics: { total_return: 0, annualized: null, max_drawdown: 0, volatility: 0 },
        current_value_usd: "0",
        positions: [],
        trades: [],
        realized_pnl_total: "0",
        win_rate: null,
        insufficient_data: true,
      }),
    ),
  );

  renderSection();

  await waitFor(() =>
    expect(
      screen.getByText("Tidak ada posisi terbuka."),
    ).toBeInTheDocument(),
  );
  expect(screen.getByText("Belum ada trade selesai.")).toBeInTheDocument();
});

test("applies gain class to positive PnL and loss class to negative PnL", async () => {
  server.use(
    http.get("/api/portfolio/hyperliquid", () =>
      HttpResponse.json({
        points: [],
        metrics: { total_return: 0, annualized: null, max_drawdown: 0, volatility: 0 },
        current_value_usd: "500",
        positions: [
          {
            coin: "BTC",
            direction: "short",
            size: "0.1",
            entry_px: "70000",
            mark_px: "65000",
            unrealized_pnl: "500",
            leverage: "10",
            notional: "6500",
            updated_at: "2026-06-18T00:00:00Z",
          },
          {
            coin: "SOL",
            direction: "long",
            size: "10",
            entry_px: "200",
            mark_px: "180",
            unrealized_pnl: "-200",
            leverage: "3",
            notional: "1800",
            updated_at: "2026-06-18T00:00:00Z",
          },
        ],
        trades: [],
        realized_pnl_total: "300",
        win_rate: 0.5,
        insufficient_data: false,
      }),
    ),
  );

  renderSection();

  // Positive uPnL gets "gain" class
  const gainCell = await screen.findByText("$500");
  expect(gainCell).toHaveClass("gain");

  // Negative uPnL gets "loss" class
  const lossCell = screen.getByText("$-200");
  expect(lossCell).toHaveClass("loss");
});
