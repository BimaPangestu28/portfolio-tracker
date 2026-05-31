import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import DashboardPage from "./DashboardPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <DashboardPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("dashboard shows net worth from the API", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText("Net Worth (USD)")).toBeInTheDocument());
  expect(screen.getByText("$300.00")).toBeInTheDocument();
});
