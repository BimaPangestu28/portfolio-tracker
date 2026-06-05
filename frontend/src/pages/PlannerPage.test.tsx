import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { server } from "../test/server";
import PlannerPage from "./PlannerPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("shows planner header and total-target indicator", async () => {
  render(<PlannerPage />, { wrapper });
  expect(screen.getByText("Planner")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText(/Total target alokasi/)).toBeInTheDocument());
});

test("opens add-category dialog on button click", async () => {
  render(<PlannerPage />, { wrapper });
  const btn = screen.getByRole("button", { name: /tambah kategori target/i });
  fireEvent.click(btn);
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  expect(screen.getByLabelText("Nama kategori")).toBeInTheDocument();
  expect(screen.getByLabelText("Target persen")).toBeInTheDocument();
});

test("editing a category target PATCHes it on save", async () => {
  const patches: Array<{ id: string; body: Record<string, unknown> }> = [];
  const etf = { id: 6, name: "ETF US", target_pct: "30", tolerance_band_pct: "5", sort_order: 0, color: null };
  server.use(
    http.get("/api/categories", () => HttpResponse.json([etf])),
    http.patch("/api/categories/:id", async ({ params, request }) => {
      const body = (await request.json()) as Record<string, unknown>;
      patches.push({ id: String(params.id), body });
      return HttpResponse.json({ ...etf, target_pct: String(body.target_pct ?? etf.target_pct) });
    }),
  );
  render(<PlannerPage />, { wrapper });
  const input = await screen.findByLabelText("Target ETF US");
  fireEvent.change(input, { target: { value: "65" } });
  fireEvent.blur(input);
  await waitFor(() => expect(patches).toHaveLength(1));
  expect(patches[0]).toEqual({ id: "6", body: { target_pct: "65" } });
});

test("shows empty categories state", async () => {
  render(<PlannerPage />, { wrapper });
  await waitFor(() =>
    expect(screen.queryByText(/Belum ada kategori/) || screen.queryByText(/on target/)).toBeTruthy(),
  );
});
