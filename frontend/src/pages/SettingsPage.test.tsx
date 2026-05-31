import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import SettingsPage from "./SettingsPage";

test("renders settings sections", () => {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(<QueryClientProvider client={qc}><SettingsPage /></QueryClientProvider>);
  expect(screen.getByText("Accounts")).toBeInTheDocument();
  expect(screen.getByText("Instruments")).toBeInTheDocument();
  expect(screen.getByText("USD → IDR FX rate")).toBeInTheDocument();
});
