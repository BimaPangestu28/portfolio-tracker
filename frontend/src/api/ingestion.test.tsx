import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { useReviewItems } from "./hooks";
import { ReviewItemSchema } from "./schemas";

function wrapper({ children }: { children: ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("useReviewItems fetches pending list", async () => {
  const { result } = renderHook(() => useReviewItems(), { wrapper });
  await waitFor(() => expect(result.current.isSuccess).toBe(true));
  expect(Array.isArray(result.current.data)).toBe(true);
});

test("ReviewItemSchema parses a row", () => {
  const row = ReviewItemSchema.parse({
    id: 1, batch_id: "b", source_kind: "image", source_filename: "f.png", source_path: "p",
    doc_type: "holdings_snapshot", status: "pending", needs_attention: 1,
    payload_json: "{\"entry_type\":\"opening_balance\",\"symbol\":\"BTC\"}", raw_llm_json: "{}",
    suggested_instrument_id: null, suggested_account_id: null, created_txn_id: null,
    created_at: "2026-06-01T00:00:00Z", confirmed_at: null,
  });
  expect(row.doc_type).toBe("holdings_snapshot");
});
