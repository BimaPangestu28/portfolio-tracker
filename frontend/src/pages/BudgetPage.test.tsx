import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import BudgetPage from "./BudgetPage";

function renderPage() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <BudgetPage />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders Income, Expense, Net stat cards and cashflow entry button", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText("Income")).toBeInTheDocument());
  expect(screen.getByText("Expense")).toBeInTheDocument();
  expect(screen.getByText("Net")).toBeInTheDocument();
  // Catat button is always present
  expect(screen.getByRole("button", { name: /catat arus kas/i })).toBeInTheDocument();
});

test("shows empty category hint when no categories", async () => {
  renderPage();
  await waitFor(() => expect(screen.getByText(/No category data for this month/i)).toBeInTheDocument());
});

test("opens add cashflow dialog with Amount and Direction fields", async () => {
  renderPage();
  fireEvent.click(screen.getByRole("button", { name: /catat arus kas/i }));
  await waitFor(() => expect(screen.getByRole("dialog")).toBeInTheDocument());
  expect(screen.getByLabelText("Amount")).toBeInTheDocument();
  expect(screen.getByLabelText("Direction")).toBeInTheDocument();
});
