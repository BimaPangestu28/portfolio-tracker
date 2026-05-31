import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import HoldingsPage from "./HoldingsPage";

test("shows empty state when no positions", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><HoldingsPage /></QueryClientProvider>);
  await waitFor(() => expect(screen.getByText(/No positions yet/)).toBeInTheDocument());
});
