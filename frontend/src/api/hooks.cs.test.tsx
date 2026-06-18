import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor, act } from "@testing-library/react";
import type { ReactNode } from "react";
import { expect, test } from "vitest";
import { useCsProducts, useCsEscalations, useReplyConversation } from "./hooks";

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("useCsProducts fetches and validates the product list", async () => {
  const { result } = renderHook(() => useCsProducts(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.[0].name).toBe("Paket A");
});

test("useCsEscalations validates the escalation list", async () => {
  const { result } = renderHook(() => useCsEscalations(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(result.current.data?.[0].reason).toBe("cannot_answer");
});

test("useReplyConversation posts to the reply endpoint and succeeds", async () => {
  const { result } = renderHook(() => useReplyConversation(), { wrapper });
  await act(async () => {
    result.current.mutate({ id: 3, text: "Halo, ada yang bisa dibantu?" });
  });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
});
