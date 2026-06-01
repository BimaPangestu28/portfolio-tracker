import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor } from "@testing-library/react";
import ImportPage from "./ImportPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MemoryRouter>{children}</MemoryRouter>
    </QueryClientProvider>
  );
}

test("shows upload control and info banner", async () => {
  render(<ImportPage />, { wrapper });
  // Info banner
  expect(screen.getByText(/tidak ada yang otomatis/i)).toBeInTheDocument();
  // Upload label
  expect(screen.getByText(/pilih berkas/i)).toBeInTheDocument();
  // File input type=file
  expect(screen.getByLabelText(/pilih berkas/i)).toHaveAttribute("type", "file");
});

test("shows empty pending state with queue header", async () => {
  render(<ImportPage />, { wrapper });
  await waitFor(() => expect(screen.getByText(/No pending items/i)).toBeInTheDocument());
  expect(screen.getByText(/Antrian Review/)).toBeInTheDocument();
});
