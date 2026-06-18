import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "@/test/server";
import { HyperliquidCard } from "@/components/HyperliquidCard";

/**
 * Renders HyperliquidCard with a QueryClient configured for testing.
 *
 * Seeds the auth token in localStorage so any auth-aware fetch headers
 * are populated (matches the pattern in App.test.tsx).
 */
function renderCard() {
  localStorage.setItem("pt-auth-token", "test-token");
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={queryClient}>
      <HyperliquidCard />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  localStorage.clear();
});

test("shows current equity from the API", async () => {
  server.use(
    http.get("*/portfolio/hyperliquid", () =>
      HttpResponse.json({
        points: [
          { date: "2026-06-01", cum_return: 0, nav: 1000 },
          { date: "2026-06-02", cum_return: 0.1, nav: 1100 },
        ],
        metrics: {
          total_return: 0.1,
          annualized: null,
          max_drawdown: -0.05,
          volatility: 0.2,
        },
        current_value_usd: "1100",
        insufficient_data: false,
      }),
    ),
  );
  renderCard();
  await waitFor(() => expect(screen.getByText("$1100")).toBeInTheDocument());
});
