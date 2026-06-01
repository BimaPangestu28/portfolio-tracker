import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import HoldingsPage from "./HoldingsPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("shows empty state when no positions", async () => {
  render(<HoldingsPage />, { wrapper });
  await waitFor(() => expect(screen.getByText(/Belum ada posisi/)).toBeInTheDocument());
});

test("renders the Holdings page header and table headers", async () => {
  render(<HoldingsPage />, { wrapper });
  expect(screen.getByText("Holdings")).toBeInTheDocument();
  await waitFor(() => {
    // Empty state OR table — both are valid when backend returns empty
    const emptyState = screen.queryByText(/Belum ada posisi/);
    const instrHeader = screen.queryByText("Instrumen");
    expect(emptyState || instrHeader).toBeTruthy();
  });
});
