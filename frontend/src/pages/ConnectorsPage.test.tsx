import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import ConnectorsPage from "./ConnectorsPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

test("renders the add-connector form with address field and empty state", async () => {
  render(<ConnectorsPage />, { wrapper });

  // Address input field is present
  expect(screen.getByLabelText("Wallet address")).toBeInTheDocument();

  // API key input should be type=password
  expect(screen.getByLabelText("API key")).toHaveAttribute("type", "password");

  // Wait for empty state message to appear after connectors load
  await waitFor(() =>
    expect(screen.getByText(/no connectors yet/i)).toBeInTheDocument(),
  );
});
