import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import ConnectorsPage from "./ConnectorsPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <ConnectorsPage />
    </QueryClientProvider>,
  );
}

test("renders the add-connector form and empty list", async () => {
  renderPage();
  expect(screen.getByLabelText("Wallet address")).toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByText("No connectors yet.")).toBeInTheDocument(),
  );
});
