import { Navigate, Routes, Route } from "react-router-dom";
import AppShell from "./components/AppShell";
import LoginPage from "./pages/LoginPage";
import { useAuth } from "./auth/AuthContext";

// Primary 6-item IA pages
import DashboardPage from "./pages/DashboardPage";
import PortfolioPage from "./pages/PortfolioPage";
import PerformancePage from "./pages/PerformancePage";
import PlannerPage from "./pages/PlannerPage";
import BudgetPage from "./pages/BudgetPage";
import DataPage from "./pages/DataPage";
import ChatPage from "./pages/ChatPage";
import WhatsAppPage from "./pages/WhatsAppPage";
import TelegramPage from "./pages/TelegramPage";
import SettingsPage from "./pages/SettingsPage";

export default function App() {
  const { isUnlocked } = useAuth();

  // ── Auth gate ─────────────────────────────────────────────────────────────
  // No stored JWT => show the master-password login. The token is issued by
  // POST /auth/login and validated server-side; see AuthContext.tsx.
  if (!isUnlocked) {
    return <LoginPage />;
  }

  return (
    <Routes>
      <Route element={<AppShell />}>
        {/* ── Primary 6-item IA ── */}
        <Route index element={<DashboardPage />} />
        <Route path="portfolio" element={<PortfolioPage />} />
        <Route path="performance" element={<PerformancePage />} />
        <Route path="planner" element={<PlannerPage />} />
        <Route path="budget" element={<BudgetPage />} />
        <Route path="data" element={<DataPage />} />
        <Route path="chat" element={<ChatPage />} />
        <Route path="whatsapp" element={<WhatsAppPage />} />
        <Route path="telegram" element={<TelegramPage />} />

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
