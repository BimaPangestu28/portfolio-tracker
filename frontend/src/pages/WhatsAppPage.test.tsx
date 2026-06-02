import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import WhatsAppPage from "./WhatsAppPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <WhatsAppPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => vi.restoreAllMocks());

test("shows Connect button when disconnected", async () => {
  vi.spyOn(global, "fetch").mockResolvedValue(
    new Response(JSON.stringify({ status: "disconnected", qr: null, number: null }), {
      headers: { "content-type": "application/json" },
    }),
  );
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /hubungkan whatsapp/i })).toBeInTheDocument(),
  );
});

test("shows the connected number and Disconnect button when connected", async () => {
  vi.spyOn(global, "fetch").mockResolvedValue(
    new Response(JSON.stringify({ status: "connected", qr: null, number: "62812" }), {
      headers: { "content-type": "application/json" },
    }),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/62812/)).toBeInTheDocument());
  expect(screen.getByRole("button", { name: /putuskan/i })).toBeInTheDocument();
});
