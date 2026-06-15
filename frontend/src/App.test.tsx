import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render, screen } from "@testing-library/react";
import App from "./App";
import { ThemeProvider } from "@/components/theme-provider";
import { AuthProvider } from "@/auth/AuthContext";

/**
 * Tests run in an "unlocked" state — we seed a JWT in localStorage before
 * render so the auth gate passes and the AppShell + nav render normally.
 *
 * localStorage key "pt-auth-token" present === AuthContext.isUnlocked === true.
 */
function renderApp(initialPath = "/") {
  // Seed a token so AuthContext.isUnlocked === true from the start
  localStorage.setItem("pt-auth-token", "test-token");

  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <ThemeProvider defaultTheme="dark" storageKey="test-theme">
      <AuthProvider>
        <QueryClientProvider client={qc}>
          <MemoryRouter initialEntries={[initialPath]}>
            <App />
          </MemoryRouter>
        </QueryClientProvider>
      </AuthProvider>
    </ThemeProvider>,
  );
}

afterEach(() => {
  // Clean up auth localStorage between tests
  localStorage.clear();
});

test("renders nav with Dashboard link", () => {
  renderApp();
  // Sidebar nav item + topbar title both say "Dashboard"
  expect(screen.getAllByText("Dashboard").length).toBeGreaterThan(0);
});

test("renders grouped nav items", () => {
  renderApp();
  expect(screen.getAllByText("Portofolio").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Rencana").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Budget").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Data").length).toBeGreaterThan(0);
  expect(screen.getAllByText("Noah").length).toBeGreaterThan(0);
});

test("consolidated pages are not standalone nav items", () => {
  renderApp();
  // Performa lives as a tab inside Portofolio; WhatsApp/Telegram inside Pengaturan
  expect(screen.queryByRole("link", { name: /performa/i })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: /whatsapp/i })).not.toBeInTheDocument();
  expect(screen.queryByRole("link", { name: /telegram/i })).not.toBeInTheDocument();
});

test("settings link lives in the sidebar footer", () => {
  renderApp();
  expect(screen.getAllByRole("link", { name: /pengaturan/i }).length).toBeGreaterThan(0);
});

test("legacy /performance redirects to /portfolio", () => {
  renderApp("/performance");
  // Portfolio page renders its tab bar
  expect(screen.getAllByText("Transaksi").length).toBeGreaterThan(0);
});

test("legacy /whatsapp and /telegram redirect to /settings", () => {
  renderApp("/whatsapp");
  // Settings page renders its tab bar with the Umum tab
  expect(screen.getAllByText("Umum").length).toBeGreaterThan(0);
});
