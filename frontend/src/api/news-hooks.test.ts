import { renderHook, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { http, HttpResponse } from "msw";
import React from "react";
import { server } from "../test/server";
import { useNewsDates, useNewsDigest } from "./hooks";

function wrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: React.ReactNode }) =>
    React.createElement(QueryClientProvider, { client: qc }, children);
}

test("useNewsDates fetches the date list", async () => {
  server.use(http.get("*/api/news/dates", () => HttpResponse.json(
    [{ date: "2026-06-18", article_count: 3, created_at: "2026-06-18T00:00:00Z" }],
  )));
  const { result } = renderHook(() => useNewsDates(30), { wrapper: wrapper() });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.[0].date).toBe("2026-06-18");
  expect(result.current.data?.[0].article_count).toBe(3);
});

test("useNewsDigest is disabled when date is undefined", () => {
  const { result } = renderHook(() => useNewsDigest(undefined), { wrapper: wrapper() });
  expect(result.current.fetchStatus).toBe("idle");
});
