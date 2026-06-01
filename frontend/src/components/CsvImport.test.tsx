import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen, waitFor, fireEvent } from "@testing-library/react";
import { CsvImport } from "./CsvImport";

function renderComponent() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <CsvImport />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

test("renders CSV textarea", () => {
  renderComponent();
  expect(screen.getByLabelText("CSV text")).toBeInTheDocument();
});

test("after entering a header line, mapping selects and Import CSV button render", async () => {
  renderComponent();

  const textarea = screen.getByLabelText("CSV text");
  fireEvent.change(textarea, { target: { value: "Date,Side,Ticker,Qty,Price\n2026-01-02,buy,BTC,0.5,60000\n" } });

  await waitFor(() => expect(screen.getByText("Map CSV columns to fields")).toBeInTheDocument());

  // mapping selects for each field should be present
  expect(screen.getByLabelText("Map entry_type")).toBeInTheDocument();
  expect(screen.getByLabelText("Map symbol")).toBeInTheDocument();
  expect(screen.getByLabelText("Map quantity")).toBeInTheDocument();
  expect(screen.getByLabelText("Map price_native")).toBeInTheDocument();
  expect(screen.getByLabelText("Map executed_at")).toBeInTheDocument();

  // Import button
  expect(screen.getByText("Import CSV")).toBeInTheDocument();
});
