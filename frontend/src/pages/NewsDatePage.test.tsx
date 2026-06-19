import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import NewsDatePage from "./NewsDatePage";

function renderAt(date: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[`/news/${date}`]}>
        <Routes><Route path="news/:date" element={<NewsDatePage />} /></Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders a stored historical digest", async () => {
  server.use(http.get("*/api/news/digest/2026-06-18", () => HttpResponse.json({
    available: true, date: "2026-06-18",
    articles: [{ position: 0, title: "Old News", url: "https://ex.com/o", source: "HN", summary: "ringkas", key_points: [], image_url: null, read_minutes: null }],
    quiz: [],
  })));
  renderAt("2026-06-18");
  expect(await screen.findByText("Old News")).toBeInTheDocument();
});

test("shows empty state when that date has no digest", async () => {
  server.use(http.get("*/api/news/digest/2020-01-01", () => HttpResponse.json({ available: false, date: null, articles: [], quiz: [] })));
  renderAt("2020-01-01");
  expect(await screen.findByText(/tidak ada digest/i)).toBeInTheDocument();
});
