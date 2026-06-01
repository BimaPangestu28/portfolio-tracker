import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import ImportPage from "./ImportPage";

test("shows upload control and empty pending state", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={qc}>
      <MemoryRouter><ImportPage /></MemoryRouter>
    </QueryClientProvider>,
  );
  expect(screen.getByText(/Choose screenshots/i)).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText(/No pending items/i)).toBeInTheDocument());
});
