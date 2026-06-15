import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import ChatPage from "./ChatPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

test("renders chat input and Send button", async () => {
  render(<ChatPage />, { wrapper });
  expect(screen.getByLabelText("Chat message")).toBeInTheDocument();
  expect(screen.getByRole("button", { name: /kirim pesan/i })).toBeInTheDocument();
});

test("shows empty-history message when no messages exist", async () => {
  render(<ChatPage />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/no messages yet/i)).toBeInTheDocument(),
  );
});

test("renders suggestion chips", async () => {
  render(<ChatPage />, { wrapper });
  expect(screen.getByRole("button", { name: /agenda saya hari ini/i })).toBeInTheDocument();
});

test("shows WhatsApp status badge", async () => {
  render(<ChatPage />, { wrapper });
  await waitFor(() =>
    expect(screen.getByText(/WhatsApp tidak terhubung/)).toBeInTheDocument(),
  );
});
