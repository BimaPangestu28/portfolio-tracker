import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import SettingsPage from "./SettingsPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(<QueryClientProvider client={qc}><SettingsPage /></QueryClientProvider>);
}

test("renders settings sections", () => {
  renderPage();
  expect(screen.getByText("Accounts")).toBeInTheDocument();
  expect(screen.getByText("Instruments")).toBeInTheDocument();
  expect(screen.getByText("USD → IDR FX rate")).toBeInTheDocument();
});

test("assigning a category to an instrument PATCHes it", async () => {
  const patches: Array<{ id: string; body: Record<string, unknown> }> = [];
  const bmri = {
    id: 1, symbol: "BMRI", name: "Bank Mandiri", instrument_type: "stock_id",
    native_currency: "IDR", category_id: null, price_source: "yahoo:BMRI.JK", decimals: 0, note: null,
  };
  server.use(
    http.get("/api/instruments", () => HttpResponse.json([bmri])),
    http.get("/api/categories", () =>
      HttpResponse.json([
        { id: 5, name: "Saham IDX", target_pct: "40", tolerance_band_pct: null, sort_order: 0, color: null },
      ]),
    ),
    http.patch("/api/instruments/:id", async ({ params, request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      patches.push({ id: String(params.id), body });
      return HttpResponse.json({ ...bmri, category_id: (body.category_id as number | null) ?? null });
    }),
  );
  renderPage();
  const select = await screen.findByLabelText("Kategori BMRI");
  fireEvent.change(select, { target: { value: "5" } });
  await waitFor(() => expect(patches).toHaveLength(1));
  expect(patches[0]).toEqual({ id: "1", body: { category_id: 5 } });
});

test("editing price source PATCHes on save", async () => {
  const patches: Array<Record<string, unknown>> = [];
  const gold = {
    id: 2, symbol: "GOLD", name: "Gold", instrument_type: "gold",
    native_currency: "IDR", category_id: null, price_source: "manual", decimals: 4, note: null,
  };
  server.use(
    http.get("/api/instruments", () => HttpResponse.json([gold])),
    http.patch("/api/instruments/:id", async ({ request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      patches.push(body);
      return HttpResponse.json({ ...gold, price_source: String(body.price_source ?? gold.price_source) });
    }),
  );
  renderPage();
  const input = await screen.findByLabelText("Price source GOLD");
  fireEvent.change(input, { target: { value: "gold_spot_idr" } });
  fireEvent.blur(input);
  await waitFor(() => expect(patches).toHaveLength(1));
  expect(patches[0]).toEqual({ price_source: "gold_spot_idr" });
});
