import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useSummary, useMonthSummary, useConnectors, useChatHistory, useInsights, useGoals } from "./hooks";

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("useSummary fetches and validates summary", async () => {
  const { result } = renderHook(() => useSummary(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.net_worth_usd).toBe("300");
});

test("useMonthSummary parses month summary from API", async () => {
  const { result } = renderHook(() => useMonthSummary("2026-06"), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.month).toBe("2026-06");
  expect(result.current.data?.total_in).toBe("0");
  expect(result.current.data?.total_out).toBe("0");
  expect(result.current.data?.net).toBe("0");
  expect(result.current.data?.categories).toEqual([]);
});

test("useConnectors returns an array", async () => {
  const { result } = renderHook(() => useConnectors(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(Array.isArray(result.current.data)).toBe(true);
});

test("useChatHistory returns an array", async () => {
  const { result } = renderHook(() => useChatHistory(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(Array.isArray(result.current.data)).toBe(true);
});

// ── Phase 5A: Insights + Goals ─────────────────────────────────────────────

test("useInsights fetches and validates insights", async () => {
  const { result } = renderHook(() => useInsights(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  const data = result.current.data!;
  expect(data.net_worth_idr).toBe("1520000000");
  expect(data.day_delta_pct).toBe("0.56");
  expect(data.savings_rate).toBe("0.32");
  expect(data.runway_months).toBe("16");
  expect(data.concentration).toEqual({ symbol: "BBCA", pct: "18.5" });
  expect(data.monthly_income_idr).toBe("18000000");
  expect(data.monthly_expense_idr).toBe("12200000");
  expect(Array.isArray(data.composition)).toBe(true);
  expect(data.composition[0].parts[0].category).toBe("Saham");
});

test("useGoals returns an array", async () => {
  const { result } = renderHook(() => useGoals(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(Array.isArray(result.current.data)).toBe(true);
});
