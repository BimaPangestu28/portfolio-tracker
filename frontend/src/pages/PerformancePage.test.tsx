import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { server } from "../test/server";
import PerformancePage from "./PerformancePage";

function wrap() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter><PerformancePage /></MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders metric cards from the API", async () => {
  server.use(
    http.get("/api/portfolio/performance", () =>
      HttpResponse.json({
        base: "idr",
        points: [
          { date: "2026-01-01", cum_return: 0, nav: 1000000 },
          { date: "2026-02-01", cum_return: 0.1, nav: 1100000 },
        ],
        metrics: { total_return: 0.1, annualized: 0.12, max_drawdown: -0.05, volatility: 0.2 },
        insufficient_data: false,
      }),
    ),
  );
  wrap();
  await waitFor(() => expect(screen.getByText(/Total return/i)).toBeInTheDocument());
  expect(screen.getByText(/Max drawdown/i)).toBeInTheDocument();
});

test("shows empty state when insufficient data", async () => {
  server.use(
    http.get("/api/portfolio/performance", () =>
      HttpResponse.json({
        base: "idr", points: [],
        metrics: { total_return: 0, annualized: 0, max_drawdown: 0, volatility: 0 },
        insufficient_data: true,
      }),
    ),
  );
  wrap();
  await waitFor(() => expect(screen.getByText(/belum cukup data/i)).toBeInTheDocument());
});

test("toggling period refetches with the new period param", async () => {
  const seenPeriods: string[] = [];
  server.use(
    http.get("/api/portfolio/performance", ({ request }) => {
      seenPeriods.push(new URL(request.url).searchParams.get("period") ?? "");
      return HttpResponse.json({
        base: "idr",
        points: [],
        metrics: { total_return: 0, annualized: 0, max_drawdown: 0, volatility: 0 },
        insufficient_data: true,
      });
    }),
  );
  wrap();
  await waitFor(() => expect(seenPeriods.length).toBeGreaterThan(0));

  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Semua" }));

  await waitFor(() => expect(seenPeriods).toContain("all"));
});
