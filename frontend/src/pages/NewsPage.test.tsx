import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import NewsPage from "./NewsPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter><NewsPage /></MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders articles and key points", async () => {
  server.use(http.get("*/api/news/dates", () => HttpResponse.json([])));
  server.use(http.get("*/api/news/today", () => HttpResponse.json({
    available: true, date: "2026-06-16",
    articles: [{ position: 0, title: "Rust 2.0", url: "https://ex.com/r", source: "HN", summary: "rilis besar", key_points: ["lebih cepat"], image_url: "https://ex.com/i.png", read_minutes: 4 }],
    quiz: [],
  })));
  renderPage();
  expect(await screen.findByText("Rust 2.0")).toBeInTheDocument();
  expect(await screen.findByText("lebih cepat")).toBeInTheDocument();
  expect(await screen.findByText(/4 mnt/)).toBeInTheDocument();
});

test("shows an empty state when no digest yet", async () => {
  server.use(http.get("*/api/news/dates", () => HttpResponse.json([])));
  server.use(http.get("*/api/news/today", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  renderPage();
  expect(await screen.findByText(/belum siap/i)).toBeInTheDocument();
});

test("lists archive dates linking to the detail route", async () => {
  server.use(http.get("*/api/news/today", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  server.use(http.get("*/api/news/dates", () => HttpResponse.json([
    { date: "2026-06-18", article_count: 3, created_at: "2026-06-18T00:00:00Z" },
  ])));
  renderPage();
  const link = await screen.findByRole("link", { name: /3 artikel/i });
  expect(link).toHaveAttribute("href", "/news/2026-06-18");
});
