import { Navigate, Routes, Route } from "react-router-dom";
import AppShell from "./components/AppShell";
import LoginPage from "./pages/LoginPage";
import { useAuth } from "./auth/AuthContext";

// Primary 6-item IA pages
import DashboardPage from "./pages/DashboardPage";
import PortfolioPage from "./pages/PortfolioPage";
import PlannerPage from "./pages/PlannerPage";
import BudgetPage from "./pages/BudgetPage";
import DcaPage from "./pages/DcaPage";
import DataPage from "./pages/DataPage";
import InvoicesPage from "./pages/InvoicesPage";
import ChatPage from "./pages/ChatPage";
import SettingsPage from "./pages/SettingsPage";
import AgendaPage from "./pages/AgendaPage";
import TugasPage from "./pages/TugasPage";
import NewsPage from "./pages/NewsPage";
import NewsDatePage from "./pages/NewsDatePage";
import CsDocsPage from "./pages/CsDocsPage";
import CsPricingPage from "./pages/CsPricingPage";
import CsOrdersPage from "./pages/CsOrdersPage";
import CsInboxPage from "./pages/CsInboxPage";
import CsWhatsAppPage from "./pages/CsWhatsAppPage";

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
        <Route path="planner" element={<PlannerPage />} />
        <Route path="budget" element={<BudgetPage />} />
        <Route path="dca" element={<DcaPage />} />
        <Route path="data" element={<DataPage />} />
        <Route path="invoices" element={<InvoicesPage />} />
        <Route path="agenda" element={<AgendaPage />} />
        <Route path="tugas" element={<TugasPage />} />
        <Route path="chat" element={<ChatPage />} />
        <Route path="news" element={<NewsPage />} />
        <Route path="news/:date" element={<NewsDatePage />} />
        <Route path="settings" element={<SettingsPage />} />

        {/* ── CS Admin ── */}
        <Route path="cs/admin/docs" element={<CsDocsPage />} />
        <Route path="cs/admin/pricing" element={<CsPricingPage />} />
        <Route path="cs/admin/orders" element={<CsOrdersPage />} />
        <Route path="cs/admin/inbox" element={<CsInboxPage />} />
        <Route path="cs/admin/whatsapp" element={<CsWhatsAppPage />} />

        {/* ── Legacy route redirects ── */}
        <Route path="holdings" element={<Navigate to="/portfolio" replace />} />
        <Route path="transactions" element={<Navigate to="/portfolio" replace />} />
        <Route path="performance" element={<Navigate to="/portfolio" replace />} />
        <Route path="connectors" element={<Navigate to="/data" replace />} />
        <Route path="import" element={<Navigate to="/data" replace />} />
        <Route path="whatsapp" element={<Navigate to="/settings" replace />} />
        <Route path="telegram" element={<Navigate to="/settings" replace />} />
      </Route>
    </Routes>
  );
}
