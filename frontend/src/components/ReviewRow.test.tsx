import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import { ReviewRow } from "./ReviewRow";
import type { ReviewItem } from "../api/schemas";

const item: ReviewItem = {
  id: 1, batch_id: "b", source_kind: "image", source_filename: "binance.png", source_path: "p",
  doc_type: "holdings_snapshot", status: "pending", needs_attention: 1,
  payload_json: JSON.stringify({ entry_type: "opening_balance", symbol: "BTC", quantity: "0.5", price_native: "60000", currency: "USD", confidence: 0.5 }),
  raw_llm_json: "{}", suggested_instrument_id: null, suggested_account_id: null,
  created_txn_id: null, created_at: "2026-06-01T00:00:00Z", confirmed_at: null,
};

function renderRow() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <table><tbody><ReviewRow item={item} instruments={[]} accounts={[]} /></tbody></table>
    </QueryClientProvider>,
  );
}

test("shows doc_type and needs-attention badge and prefilled symbol", () => {
  renderRow();
  expect(screen.getByText("holdings_snapshot")).toBeInTheDocument();
  expect(screen.getByText(/needs attention/i)).toBeInTheDocument();
  expect(screen.getByDisplayValue("BTC")).toBeInTheDocument();
  expect(screen.getByDisplayValue("0.5")).toBeInTheDocument();
});

test("renders confirm and reject buttons", () => {
  renderRow();
  expect(screen.getByRole("button", { name: /confirm/i })).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /reject/i })).toBeInTheDocument();
});
