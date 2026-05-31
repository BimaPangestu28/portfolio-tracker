import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import PlannerPage from "./PlannerPage";

test("shows planner form and total-target hint", async () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><PlannerPage /></QueryClientProvider>);
  expect(screen.getByText("Add category")).toBeInTheDocument();
  await waitFor(() => expect(screen.getByText(/Total target:/)).toBeInTheDocument());
});
