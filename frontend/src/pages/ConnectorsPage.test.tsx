import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import ConnectorsPage from "./ConnectorsPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

test("renders Connectors header and empty state", async () => {
  render(<ConnectorsPage />, { wrapper });
  expect(screen.getByText("Connectors")).toBeInTheDocument();
  await waitFor(() =>
    expect(screen.getByText(/No connectors yet/i)).toBeInTheDocument(),
  );
});

test("opens add-connector dialog with API key password field", async () => {
  render(<ConnectorsPage />, { wrapper });
  fireEvent.click(screen.getByRole("button", { name: /tambah konektor/i }));
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  // API key input type=password
  expect(screen.getByLabelText("API key")).toHaveAttribute("type", "password");
  // Wallet address field present
  expect(screen.getByLabelText("Wallet address")).toBeInTheDocument();
  // Connector label field present
  expect(screen.getByLabelText("Connector label")).toBeInTheDocument();
});
