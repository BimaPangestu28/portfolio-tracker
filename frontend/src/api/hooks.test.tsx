import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useSummary, useMonthSummary } from "./hooks";

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
