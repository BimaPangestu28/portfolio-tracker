import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { http, HttpResponse } from "msw";
import { expect, test } from "vitest";
import { server } from "../test/server";
import TelegramPage from "./TelegramPage";

function renderPage() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={queryClient}>
      <MemoryRouter>
        <TelegramPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("shows setup instructions when the bot token is not configured", async () => {
  server.use(
    http.get("/api/telegram/status", () =>
      HttpResponse.json({ configured: false, linked: false, username: null }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/TELEGRAM_BOT_TOKEN/)).toBeInTheDocument());
});

test("shows the generate-code button when configured but unlinked", async () => {
  // Default handler in server.ts returns { configured: true, linked: false }
  renderPage();
  await waitFor(() =>
    expect(screen.getByRole("button", { name: /buat kode tautan/i })).toBeInTheDocument(),
  );
});

test("generating a code displays it with instructions", async () => {
  server.use(
    http.post("/api/telegram/link-code", () =>
      HttpResponse.json({ code: "123456", expires_in: 600 }),
    ),
  );
  renderPage();
  const button = await screen.findByRole("button", { name: /buat kode tautan/i });
  await userEvent.click(button);
  await waitFor(() => expect(screen.getByText("123456")).toBeInTheDocument());
  expect(screen.getByText(/kirim kode ini/i)).toBeInTheDocument();
});

test("shows the linked username and unlink button when linked", async () => {
  server.use(
    http.get("/api/telegram/status", () =>
      HttpResponse.json({ configured: true, linked: true, username: "bima" }),
    ),
  );
  renderPage();
  await waitFor(() => expect(screen.getByText(/@bima/)).toBeInTheDocument());
  expect(screen.getByRole("button", { name: /putus tautan/i })).toBeInTheDocument();
});
