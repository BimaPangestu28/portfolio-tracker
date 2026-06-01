import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen } from "@testing-library/react";
import App from "./App";
import { ThemeProvider } from "@/components/theme-provider";

function renderApp(initialPath = "/") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <ThemeProvider defaultTheme="dark" storageKey="test-theme">
      <QueryClientProvider client={qc}>
        <MemoryRouter initialEntries={[initialPath]}>
          <App />
        </MemoryRouter>
      </QueryClientProvider>
    </ThemeProvider>,
  );
}

test("renders nav with Dashboard link", () => {
  renderApp();
  // Sidebar nav item + topbar title both say "Dashboard"
  expect(screen.getAllByText("Dashboard").length).toBeGreaterThan(0);
});

test("renders 6-item nav items", () => {
  renderApp();
  expect(screen.getAllByText("Portofolio").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Rencana").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Budget").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Data").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Chat").length).toBeGreaterThan(0);
});
