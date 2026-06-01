import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import TransactionsPage from "./TransactionsPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("renders Tambah Transaksi button and empty state", async () => {
  render(<TransactionsPage />, { wrapper });
  // Header button should always be present
  expect(screen.getByRole("button", { name: /tambah transaksi/i })).toBeInTheDocument();
  // Wait for empty state
  await waitFor(() => expect(screen.getByText(/Belum ada transaksi/)).toBeInTheDocument());
});

test("opens dialog when Tambah Transaksi is clicked", async () => {
  render(<TransactionsPage />, { wrapper });
  const btn = screen.getByRole("button", { name: /tambah transaksi/i });
  fireEvent.click(btn);
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  // Dialog title appears inside the dialog element
  expect(screen.getByRole("dialog")).toHaveTextContent(/Tambah Transaksi/);
});

test("dialog has aria-label inputs", async () => {
  render(<TransactionsPage />, { wrapper });
  fireEvent.click(screen.getByRole("button", { name: /tambah transaksi/i }));
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  expect(screen.getByLabelText("Jumlah")).toBeInTheDocument();
  expect(screen.getByLabelText("Harga")).toBeInTheDocument();
});
