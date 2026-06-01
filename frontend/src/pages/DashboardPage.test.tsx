/**
 * DashboardPage tests — Phase 5C command-center redesign.
 *
 * MSW handlers in src/test/server.ts already provide:
 *   GET /api/portfolio/summary  — net_worth_idr "4875000", net_worth_usd "300"
 *   GET /api/portfolio/insights — day_delta_idr "8500000", savings_rate "0.32", etc.
 *   GET /api/goals              — []  (empty)
 *   GET /api/portfolio/history  — []  (empty)
 */

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
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

test("dashboard shows Passive Yield KPI card", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Passive Yield/i)).toBeInTheDocument(),
  );
});

test("dashboard shows Savings Rate KPI card", async () => {
  renderPage();
  await waitFor(() => {
    const els = screen.getAllByText(/Savings Rate/i);
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
  // MSW insights: runway_months "16"
  await waitFor(() =>
    expect(screen.getByText(/16.*bulan/i)).toBeInTheDocument(),
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

test("dashboard shows Alokasi Portofolio section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Alokasi Portofolio")).toBeInTheDocument(),
  );
});

test("dashboard shows Target vs Aktual Alokasi section", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Target vs Aktual Alokasi")).toBeInTheDocument(),
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

test("dashboard shows 'semua dalam batas' when allocation is empty (MSW returns [])", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText(/Semua dalam batas target/i)).toBeInTheDocument(),
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

// ── Movers (empty placeholder) ────────────────────────────────────────────────

test("dashboard shows Pergerakan Hari Ini empty placeholder (not fabricated data)", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByText("Pergerakan Hari Ini")).toBeInTheDocument(),
  );
  expect(screen.getByText(/Pergerakan per-aset menyusul/)).toBeInTheDocument();
});

// ── Refresh button ────────────────────────────────────────────────────────────

test("dashboard renders Refresh harga button", async () => {
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /Refresh harga/i })).toBeInTheDocument(),
  );
});
