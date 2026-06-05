/**
 * DashboardPage tests — Phase 5C command-center redesign.
 *
 * MSW handlers in src/test/server.ts already provide:
 *   GET /api/portfolio/summary  — net_worth_idr "4875000", net_worth_usd "300"
 *   GET /api/portfolio/insights — day_delta_idr "8500000", savings_rate "32" (percent), etc.
 *   GET /api/goals              — []  (empty)
 *   GET /api/portfolio/history  — []  (empty)
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import DashboardPage from "./DashboardPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

// ── Hero ──────────────────────────────────────────────────────────────────────

test("unrealized P&L sub shows percent of cost basis, not the raw IDR amount", async () => {
  // net worth 90.000, pnl -10.000 -> cost basis 100.000 -> -10.0%
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "90000", net_worth_usd: "6",
        total_unrealized_pnl_idr: "-10000", total_realized_pnl_idr: "0", xirr: null,
        total_unrealized_price_pnl_idr: "-10000", total_unrealized_fx_pnl_idr: "0",
        total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
        fx_incomplete: false,
        positions: [], allocation: [],
      }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/▼\s*-10\.0%/)).toBeInTheDocument());
  // The raw amount must never be rendered as a percentage
  expect(screen.queryByText(/-10000\.0%/)).not.toBeInTheDocument();
});

test("dashboard shows net worth IDR from summary", async () => {
  renderPage();
  // The MSW handler returns net_worth_idr "4875000"
  await waitFor(() =>
    expect(screen.getByText(/Rp\s*4\.875\.000/)).toBeInTheDocument(),
  );
});

test("dashboard shows net worth USD from summary", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/\$300\.00/)).toBeInTheDocument(),
  );
});

test("dashboard shows day delta from insights", async () => {
  renderPage();
  // MSW insights: day_delta_idr "8500000"
  await waitFor(() =>
    expect(screen.getByText(/Rp\s*8\.500\.000/)).toBeInTheDocument(),
  );
});

// ── KPI Cards ─────────────────────────────────────────────────────────────────

test("dashboard shows Unrealized P&L KPI card", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Unrealized P&L/i)).toBeInTheDocument(),
  );
});

test("dashboard shows XIRR KPI card", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/XIRR/i)).toBeInTheDocument(),
  );
});

test("dashboard shows Imbal Hasil Pasif KPI card", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Imbal Hasil Pasif/i)).toBeInTheDocument(),
  );
});

test("dashboard shows Tingkat Menabung KPI card", async () => {
  renderPage();
  await waitFor(() => {
    const els = screen.getAllByText(/Tingkat Menabung/i);
    expect(els.length).toBeGreaterThanOrEqual(1);
  });
});

// ── Health section ────────────────────────────────────────────────────────────

test("dashboard shows Kesehatan Portofolio section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Kesehatan Portofolio")).toBeInTheDocument(),
  );
});

test("dashboard shows runway months from insights", async () => {
  renderPage();
  // MSW insights: runway_months "16" — displayed as "16,0 bln"
  await waitFor(() =>
    expect(screen.getByText(/16.*bln/i)).toBeInTheDocument(),
  );
});

test("dashboard shows concentration symbol from insights", async () => {
  renderPage();
  // MSW insights: concentration { symbol: "BBCA", pct: "18.5" }
  await waitFor(() =>
    expect(screen.getByText(/BBCA/)).toBeInTheDocument(),
  );
});

// ── Allocation section ────────────────────────────────────────────────────────

test("dashboard shows Alokasi Aset section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Alokasi Aset")).toBeInTheDocument(),
  );
});

test("dashboard shows Target vs Aktual section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Target vs Aktual")).toBeInTheDocument(),
  );
});

// ── Composition chart ─────────────────────────────────────────────────────────

test("dashboard shows Komposisi Kekayaan section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Komposisi Kekayaan")).toBeInTheDocument(),
  );
});

// ── Rebalancing ───────────────────────────────────────────────────────────────

test("dashboard shows Rekomendasi Rebalancing section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Rekomendasi Rebalancing")).toBeInTheDocument(),
  );
});

test("dashboard shows 'portofolio seimbang' when allocation is empty (MSW returns [])", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Portofolio seimbang/i)).toBeInTheDocument(),
  );
});

// ── Goals ──────────────────────────────────────────────────────────────────────

test("dashboard shows Tujuan Keuangan section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Tujuan Keuangan")).toBeInTheDocument(),
  );
});

test("dashboard shows empty goals state when no goals", async () => {
  renderPage();
  // MSW returns [] for /api/goals
  await waitFor(() =>
    expect(screen.getByText(/Belum ada tujuan keuangan/i)).toBeInTheDocument(),
  );
});

// ── Movers ────────────────────────────────────────────────────────────────────

test("dashboard shows movers empty state when no price history", async () => {
  // Default MSW handler returns [] for /api/portfolio/movers
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Belum cukup data harga/)).toBeInTheDocument(),
  );
});

test("dashboard lists movers with formatted IDR deltas", async () => {
  server.use(
    http.get("/api/portfolio/movers", () =>
      HttpResponse.json([
        { instrument_id: 5, symbol: "TLKM", name: "Telkom Indonesia Tbk", delta_idr: "21000", delta_pct: 1.05, value_idr: "2030000" },
        { instrument_id: 18, symbol: "BTC", name: "Bitcoin", delta_idr: "-150000", delta_pct: -2.4, value_idr: "5000000" },
      ]),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText("TLKM")).toBeInTheDocument());
  expect(screen.getByText(/\+\s*Rp\s*21\.000/)).toBeInTheDocument();
  expect(screen.getByText(/−\s*Rp\s*150\.000/)).toBeInTheDocument();
});

// ── Pending review banner ─────────────────────────────────────────────────────

test("dashboard shows a banner when review items are pending", async () => {
  server.use(
    http.get("/api/ingest/review", () =>
      HttpResponse.json([
        {
          id: 75, batch_id: "tg-1", source_kind: "image", source_filename: "f.jpg",
          source_path: "p", doc_type: "holdings_snapshot", status: "pending",
          needs_attention: 1, payload_json: "{}", raw_llm_json: "{}",
          created_at: "2026-06-05T00:00:00Z",
        },
      ]),
    ),
  );
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/1 transaksi menunggu konfirmasi/)).toBeInTheDocument(),
  );
});

test("dashboard hides the review banner when nothing is pending", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText("Pergerakan Hari Ini")).toBeInTheDocument());
  expect(screen.queryByText(/menunggu konfirmasi/)).not.toBeInTheDocument();
});

// ── Stale price badge ─────────────────────────────────────────────────────────

test("hero badge warns when held positions have stale prices", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "1000000", net_worth_usd: "61",
        total_unrealized_pnl_idr: "0", total_realized_pnl_idr: "0", xirr: null,
        total_unrealized_price_pnl_idr: "0", total_unrealized_fx_pnl_idr: "0",
        total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
        fx_incomplete: false,
        positions: [{
          instrument_id: 10, quantity: "3.23", avg_cost: "2600000",
          cost_basis_total: "8398000", latest_price: "2600000", price_stale: true,
          market_value_native: "8398000", market_value_idr: "8398000",
          market_value_usd: "515", unrealized_pnl: "0", realized_pnl: "0", income: "0",
          cost_basis_idr_total: "8398000", unrealized_pnl_idr: "0",
          unrealized_price_pnl_idr: "0", unrealized_fx_pnl_idr: "0",
          realized_pnl_idr: "0", realized_price_pnl_idr: "0", realized_fx_pnl_idr: "0",
          fx_incomplete: false,
        }],
        allocation: [],
      }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/1 harga basi/)).toBeInTheDocument());
  expect(screen.queryByText("Live")).not.toBeInTheDocument();
});

// ── Benchmark ─────────────────────────────────────────────────────────────────

test("dashboard shows benchmark returns when available", async () => {
  server.use(
    http.get("/api/portfolio/benchmark", () =>
      HttpResponse.json([
        { label: "Portofolio", return_ratio: 0.083 },
        { label: "IHSG", return_ratio: 0.051 },
      ]),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText("IHSG")).toBeInTheDocument());
  expect(screen.getByText("+8,3%")).toBeInTheDocument();
  expect(screen.getByText("+5,1%")).toBeInTheDocument();
});

test("dashboard hides the benchmark card when no data", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText("Pergerakan Hari Ini")).toBeInTheDocument());
  expect(screen.queryByText("vs Benchmark")).not.toBeInTheDocument();
});

// ── Posisi Terbesar + Aktivitas ───────────────────────────────────────────────

test("dashboard shows top holdings and recent activity sections", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText("Posisi Terbesar")).toBeInTheDocument());
  expect(screen.getByText("Aktivitas Terakhir")).toBeInTheDocument();
});

// ── Refresh button ────────────────────────────────────────────────────────────

test("dashboard renders Refresh harga button", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /Refresh harga/i })).toBeInTheDocument(),
  );
});

// ── FX breakdown on Unrealized P&L card ──────────────────────────────────────

test("unrealized P&L card shows the price vs FX breakdown", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "2550000", net_worth_usd: "150",
        total_unrealized_pnl_idr: "950000", total_realized_pnl_idr: "0", xirr: null,
        total_unrealized_price_pnl_idr: "850000", total_unrealized_fx_pnl_idr: "100000",
        total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
        fx_incomplete: false,
        positions: [], allocation: [],
      }),
    ),
  );
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Harga\s*Rp\s*850\.000/)).toBeInTheDocument(),
  );
  expect(screen.getByText(/FX\s*Rp\s*100\.000/)).toBeInTheDocument();
  expect(screen.queryByText("FX?")).not.toBeInTheDocument();
});

test("unrealized P&L card warns when FX data is incomplete", async () => {
  server.use(
    http.get("/api/portfolio/summary", () =>
      HttpResponse.json({
        net_worth_idr: "2550000", net_worth_usd: "150",
        total_unrealized_pnl_idr: "950000", total_realized_pnl_idr: "0", xirr: null,
        total_unrealized_price_pnl_idr: "850000", total_unrealized_fx_pnl_idr: "100000",
        total_realized_price_pnl_idr: "0", total_realized_fx_pnl_idr: "0",
        fx_incomplete: true,
        positions: [], allocation: [],
      }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText("FX?")).toBeInTheDocument());
});
