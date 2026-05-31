import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import TransactionsPage from "./TransactionsPage";

test("renders the add-transaction form and empty list", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><TransactionsPage /></QueryClientProvider>);
  expect(screen.getByText("Add transaction")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText("No transactions yet.")).toBeInTheDocument());
});
