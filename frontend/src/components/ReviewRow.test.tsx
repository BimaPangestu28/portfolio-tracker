import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { ReviewRow } from "./ReviewRow";
import { server } from "../test/server";
import type { ReviewItem, Instrument } from "../api/schemas";

const asiiInstrument: Instrument = {
  id: 7, symbol: "ASII", name: "Astra International", instrument_type: "stock",
  native_currency: "IDR", category_id: null, price_source: "manual", decimals: 2, note: null,
};

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

test("confirm with no account selected shows validation message", async () => {
  // Use an item with a real suggested_instrument_id so the instrument select has a
  // numeric id (skips the create-new branch). Account has no suggestion → starts blank
  // → accountId resolves to 0 → guard fires before any network call.
  const itemWithInstrument: ReviewItem = {
    ...item,
    suggested_instrument_id: 1,
    payload_json: JSON.stringify({ entry_type: "buy", quantity: "1", price_native: "100", currency: "USD" }),
  };
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <table><tbody>
        <ReviewRow item={itemWithInstrument} instruments={[{ id: 1, symbol: "BTC", name: "Bitcoin", instrument_type: "crypto", native_currency: "USD", category_id: null, price_source: "manual", decimals: 8, note: null }]} accounts={[]} />
      </tbody></table>
    </QueryClientProvider>,
  );
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /confirm/i }));
  expect(await screen.findByText(/select or create both/i)).toBeInTheDocument();
});

test("confirm with CREATE_NEW reuses an existing instrument with the same symbol instead of creating a duplicate", async () => {
  // Item extracted an ASII symbol with no suggested instrument → row defaults to CREATE_NEW.
  // But ASII already exists in the instruments list (e.g. created when a prior row was
  // confirmed). Confirming must NOT POST /api/instruments — it must reuse the existing id.
  const asiiItem: ReviewItem = {
    ...item,
    suggested_instrument_id: null,
    suggested_account_id: 3,
    payload_json: JSON.stringify({ entry_type: "buy", symbol: "asii", quantity: "10", price_native: "5000", currency: "IDR" }),
  };

  let createCalls = 0;
  let confirmedInstrumentId: number | null = null;
  server.use(
    http.post("/api/instruments", () => {
      createCalls += 1;
      return HttpResponse.json({ ...asiiInstrument, id: 999 });
    }),
    http.post("/api/ingest/review/:id/confirm", ({ request }) =>
      request.json().then((body) => {
        confirmedInstrumentId = (body as { instrument_id: number }).instrument_id;
        return HttpResponse.json({ created_txn_id: 1 });
      }),
    ),
  );

  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <table><tbody>
        <ReviewRow item={asiiItem} instruments={[asiiInstrument]} accounts={[{ id: 3, name: "BCA", account_type: "manual", institution: null, native_currency: "IDR", note: null, created_at: "2026-06-01T00:00:00Z" }]} />
      </tbody></table>
    </QueryClientProvider>,
  );

  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: /confirm/i }));

  await waitFor(() => expect(confirmedInstrumentId).toBe(asiiInstrument.id));
  expect(createCalls).toBe(0);
});
