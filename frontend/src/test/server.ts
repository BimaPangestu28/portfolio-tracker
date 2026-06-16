import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";

export const handlers = [
  http.get("/api/portfolio/summary", () =>
    HttpResponse.json({
      net_worth_idr: "4875000", net_worth_usd: "300",
      total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0", xirr: 1.68,
      total_unrealized_price_pnl_idr: "80", total_unrealized_fx_pnl_idr: "20",
      total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
      fx_incomplete: false,
      positions: [], allocation: [],
    }),
  ),
  http.get("/api/portfolio/history", () => HttpResponse.json([])),
  http.post("/api/prices/refresh", () => HttpResponse.json(null)),
  http.get("/api/instruments", () => HttpResponse.json([])),
  http.get("/api/accounts", () => HttpResponse.json([])),
  http.get("/api/transactions", () => HttpResponse.json([])),
  http.post("/api/transactions", ({ request }) =>
    request.json().then((body) => {
      const b = body as Record<string, unknown>;
      return HttpResponse.json({
        id: 1,
        account_id: b.account_id ?? 1,
        instrument_id: b.instrument_id ?? 1,
        txn_type: b.txn_type ?? "buy",
        executed_at: b.executed_at ?? "2026-06-01T00:00:00Z",
        quantity: b.quantity ?? "1",
        price_native: b.price_native ?? "100",
        fee_native: b.fee_native ?? "0",
        currency: b.currency ?? "USD",
        fx_to_idr: b.fx_to_idr ?? "16000",
        fx_to_usd: b.fx_to_usd ?? "1",
        note: null,
      });
    }),
  ),
  http.get("/api/categories", () => HttpResponse.json([])),
  http.get("/api/ingest/review", () => HttpResponse.json([])),
  http.post("/api/ingest", () => HttpResponse.json({ batch_id: "b-test", items: [] })),
  http.post("/api/ingest/review/:id/confirm", () => HttpResponse.json({ created_txn_id: 1 })),
  http.post("/api/ingest/review/:id/reject", () => HttpResponse.json(null)),
  http.patch("/api/ingest/review/:id", ({ params }) =>
    HttpResponse.json({
      id: Number(params.id), batch_id: "b", source_kind: "image", source_filename: "f.png",
      source_path: "p", doc_type: "txn_history", status: "pending", needs_attention: 0,
      payload_json: "{}", raw_llm_json: "{}", created_at: "2026-06-01T00:00:00Z",
    })),
  http.get("/api/cashflow", () => HttpResponse.json([])),
  http.get("/api/cashflow/categories", () => HttpResponse.json([])),
  http.get("/api/cashflow/summary", () =>
    HttpResponse.json({ month: "2026-06", total_in: "0", total_out: "0", net: "0", categories: [] }),
  ),
  http.post("/api/cashflow", () => HttpResponse.json({
    id: 1, occurred_on: "2026-06-01", direction: "in", amount: "100000",
    currency: "IDR", created_at: "2026-06-01T00:00:00Z",
  })),
  http.delete("/api/cashflow/:id", () => HttpResponse.json(null)),
  http.post("/api/cashflow/categories", () => HttpResponse.json({
    id: 1, name: "Food", kind: "expense",
  })),
  http.delete("/api/cashflow/categories/:id", () => HttpResponse.json(null)),
  http.post("/api/ingest/csv", () => HttpResponse.json({ batch_id: "c", items: [] })),
  http.get("/api/connectors", () => HttpResponse.json([])),
  http.post("/api/connectors", ({ request }) =>
    request.json().then((body) => {
      const b = body as Record<string, unknown>;
      return HttpResponse.json({
        id: 1,
        account_id: b.account_id ?? 1,
        kind: b.kind ?? "evm_wallet",
        label: b.label ?? "test",
        config_json: b.config_json ?? "{}",
        cursor: null,
        last_synced_at: null,
        enabled: 1,
        created_at: "2026-06-01T00:00:00Z",
      });
    }),
  ),
  http.delete("/api/connectors/:id", () => HttpResponse.json(null)),
  http.post("/api/connectors/:id/sync", () =>
    HttpResponse.json({ inserted: 0, staged: 0, skipped: 0 }),
  ),
  http.get("/api/chat/history", () => HttpResponse.json([])),
  http.post("/api/chat", () => HttpResponse.json({ reply: "Your net worth is Rp 4,875,000." })),

  // ── Phase 5A endpoints ──────────────────────────────────────────────────
  http.get("/api/portfolio/insights", () =>
    HttpResponse.json({
      net_worth_idr: "1520000000",
      day_delta_idr: "8500000",
      day_delta_pct: "0.56",
      savings_rate: "32",
      dividend_ttm_idr: "24000000",
      yield_pct: "1.58",
      liquid_idr: "250000000",
      runway_months: "16",
      concentration: { symbol: "BBCA", pct: "18.5" },
      composition: [
        {
          as_of: "2026-05-01",
          total_idr: "1480000000",
          parts: [
            { category: "Saham", value_idr: "740000000" },
            { category: "Crypto", value_idr: "222000000" },
            { category: "Reksadana", value_idr: "296000000" },
            { category: "Kas", value_idr: "222000000" },
          ],
        },
        {
          as_of: "2026-06-01",
          total_idr: "1520000000",
          parts: [
            { category: "Saham", value_idr: "760000000" },
            { category: "Crypto", value_idr: "228000000" },
            { category: "Reksadana", value_idr: "304000000" },
            { category: "Kas", value_idr: "228000000" },
          ],
        },
      ],
      monthly_income_idr: "18000000",
      monthly_expense_idr: "12200000",
    }),
  ),
  http.get("/api/goals", () => HttpResponse.json([])),
  http.post("/api/goals", ({ request }) =>
    request.json().then((body) => {
      const b = body as Record<string, unknown>;
      return HttpResponse.json({
        id: 1,
        label: b.label ?? "Dana Darurat",
        note: b.note ?? null,
        target_idr: b.target_idr ?? "100000000",
        current_kind: b.current_kind ?? "savings",
        current_idr: b.current_idr ?? "0",
        sort_order: b.sort_order ?? 0,
        created_at: "2026-06-01T00:00:00Z",
      });
    }),
  ),
  http.delete("/api/goals/:id", () => HttpResponse.json(null)),

  // ── WhatsApp gateway ───────────────────────────────────────────────────────
  http.get("/api/whatsapp/status", () =>
    HttpResponse.json({ status: "disconnected", qr: null, number: null }),
  ),

  // ── Telegram ───────────────────────────────────────────────────────────────
  http.get("/api/telegram/status", () =>
    HttpResponse.json({ configured: true, linked: false, username: null }),
  ),

  // ── Dashboard: movers + benchmark ──────────────────────────────────────────
  http.get("/api/portfolio/movers", () => HttpResponse.json([])),
  http.get("/api/portfolio/benchmark", () => HttpResponse.json([])),
];

// ── CS Admin handlers ───────────────────────────────────────────────────────
const csHandlers = [
  http.get("/api/cs/admin/products", () =>
    HttpResponse.json([
      { id: 1, name: "Paket A", description: null, price: 150000, currency: "IDR", availability: "ready", active: 1, updated_at: "2026-06-16T00:00:00Z" },
    ]),
  ),
  http.get("/api/cs/admin/escalations", () =>
    HttpResponse.json([
      { id: 7, conversation_id: 3, reason: "cannot_answer", summary: "needs quote", status: "open", created_at: "2026-06-16T00:00:00Z", handled_at: null },
    ]),
  ),
  http.get("/api/cs/admin/docs", () => HttpResponse.json([])),
  http.get("/api/cs/admin/orders", () => HttpResponse.json([])),
  http.get("/api/cs/admin/conversations", () => HttpResponse.json([])),
  http.post("/api/cs/admin/conversations/:id/reply", () => HttpResponse.json(null)),
  http.get("/api/cs/whatsapp/status", () =>
    HttpResponse.json({ status: "disconnected", qr: null, number: null }),
  ),
  http.post("/api/cs/whatsapp/connect", () => HttpResponse.json(null)),
  http.post("/api/cs/whatsapp/disconnect", () => HttpResponse.json(null)),
];

export const server = setupServer(...handlers, ...csHandlers);
