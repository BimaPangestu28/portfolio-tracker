import { Navigate, Routes, Route } from "react-router-dom";
import AppShell from "./components/AppShell";
import LoginPage from "./pages/LoginPage";
import { useAuth } from "./auth/AuthContext";

// Primary 6-item IA pages
import DashboardPage from "./pages/DashboardPage";
import PortfolioPage from "./pages/PortfolioPage";
import PlannerPage from "./pages/PlannerPage";
import BudgetPage from "./pages/BudgetPage";
import DataPage from "./pages/DataPage";
import ChatPage from "./pages/ChatPage";
import SettingsPage from "./pages/SettingsPage";

export default function App() {
  const { isUnlocked } = useAuth();

  // ── Auth gate ─────────────────────────────────────────────────────────────
  // If not unlocked, show LoginPage (setup or login depending on hasPassword).
  // This is a frontend-only mock; see AuthContext.tsx for security notes.
  if (!isUnlocked) {
    return <LoginPage />;
  }

  return (
    <Routes>
      <Route element={<AppShell />}>
        {/* ── Primary 6-item IA ── */}
        <Route index element={<DashboardPage />} />
        <Route path="portfolio" element={<PortfolioPage />} />
        <Route path="planner" element={<PlannerPage />} />
        <Route path="budget" element={<BudgetPage />} />
        <Route path="data" element={<DataPage />} />
        <Route path="chat" element={<ChatPage />} />

        {/* ── Legacy route redirects ── */}
        <Route path="holdings" element={<Navigate to="/portfolio" replace />} />
        <Route path="transactions" element={<Navigate to="/portfolio" replace />} />
        <Route path="connectors" element={<Navigate to="/data" replace />} />
        <Route path="import" element={<Navigate to="/data" replace />} />
        <Route path="settings" element={<SettingsPage />} />
      </Route>
    </Routes>
  );
}
