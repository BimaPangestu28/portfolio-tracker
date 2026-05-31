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
];

export const server = setupServer(...handlers);
