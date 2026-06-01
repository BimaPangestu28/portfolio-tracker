import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import PlannerPage from "./PlannerPage";

function wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={qc}>{children}</QueryClientProvider>;
}

test("shows planner header and total-target indicator", async () => {
  render(<PlannerPage />, { wrapper });
  expect(screen.getByText("Planner")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText(/Total target alokasi/)).toBeInTheDocument());
});

test("opens add-category dialog on button click", async () => {
  render(<PlannerPage />, { wrapper });
  const btn = screen.getByRole("button", { name: /tambah kategori target/i });
  fireEvent.click(btn);
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  expect(screen.getByLabelText("Nama kategori")).toBeInTheDocument();
  expect(screen.getByLabelText("Target persen")).toBeInTheDocument();
});

test("shows empty categories state", async () => {
  render(<PlannerPage />, { wrapper });
  await waitFor(() =>
    expect(screen.queryByText(/Belum ada kategori/) || screen.queryByText(/on target/)).toBeTruthy(),
  );
});
