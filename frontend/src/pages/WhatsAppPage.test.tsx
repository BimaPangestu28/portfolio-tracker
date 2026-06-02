import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../test/server";
import WhatsAppPage from "./WhatsAppPage";

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <WhatsAppPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("shows Connect button when disconnected", async () => {
  // Default handler in server.ts already returns { status: "disconnected", qr: null, number: null }
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /hubungkan whatsapp/i })).toBeInTheDocument(),
  );
});

test("shows the connected number and Disconnect button when connected", async () => {
  server.use(
    http.get("/api/whatsapp/status", () =>
      HttpResponse.json({ status: "connected", qr: null, number: "62812" }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/62812/)).toBeInTheDocument());
  expect(screen.getByRole("button", { name: /putuskan/i })).toBeInTheDocument();
});
