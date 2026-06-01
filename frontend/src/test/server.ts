import { setupServer } from "msw/node";
import { http, HttpResponse } from "msw";

export const handlers = [
  http.get("/api/portfolio/summary", () =>
    HttpResponse.json({
      net_worth_idr: "4875000", net_worth_usd: "300",
      total_unrealized_pnl_idr: "100", total_realized_pnl_idr: "0", xirr: 1.68,
      positions: [], allocation: [],
    }),
  ),
  http.get("/api/portfolio/history", () => HttpResponse.json([])),
  http.post("/api/prices/refresh", () => HttpResponse.json(null)),
  http.get("/api/instruments", () => HttpResponse.json([])),
  http.get("/api/accounts", () => HttpResponse.json([])),
  http.get("/api/transactions", () => HttpResponse.json([])),
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
];

export const server = setupServer(...handlers);
