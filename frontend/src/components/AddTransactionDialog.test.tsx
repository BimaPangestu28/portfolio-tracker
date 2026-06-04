import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { http, HttpResponse } from "msw";
import { vi } from "vitest";
import { AddTransactionDialog } from "./AddTransactionDialog";
import { server } from "../test/server";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("Simpan submits the form via form association and closes on success", async () => {
  const onClose = vi.fn();
  render(<AddTransactionDialog open onClose={onClose} />, { wrapper });

  fireEvent.change(screen.getByLabelText("Jumlah"), { target: { value: "10" } });
  fireEvent.change(screen.getByLabelText("Harga"), { target: { value: "150" } });
  fireEvent.click(screen.getByRole("button", { name: /simpan/i }));

  await waitFor(() => expect(onClose).toHaveBeenCalled());
});

test("shows error and stays open when create fails", async () => {
  server.use(
    http.post("/api/transactions", () =>
      HttpResponse.json({ error: "akun tidak valid" }, { status: 400 }),
    ),
  );
  const onClose = vi.fn();
  render(<AddTransactionDialog open onClose={onClose} />, { wrapper });

  fireEvent.click(screen.getByRole("button", { name: /simpan/i }));

  await waitFor(() => expect(screen.getByText("akun tidak valid")).toBeInTheDocument());
  expect(onClose).not.toHaveBeenCalled();
});
