import { Routes, Route } from "react-router-dom";
import AppLayout from "./components/AppLayout";
import DashboardPage from "./pages/DashboardPage";
import HoldingsPage from "./pages/HoldingsPage";
import TransactionsPage from "./pages/TransactionsPage";
import PlannerPage from "./pages/PlannerPage";
import SettingsPage from "./pages/SettingsPage";
import ImportPage from "./pages/ImportPage";
import BudgetPage from "./pages/BudgetPage";
import ConnectorsPage from "./pages/ConnectorsPage";

export default function App() {
  return (
    <Routes>
      <Route element={<AppLayout />}>
        <Route index element={<DashboardPage />} />
        <Route path="holdings" element={<HoldingsPage />} />
        <Route path="transactions" element={<TransactionsPage />} />
        <Route path="planner" element={<PlannerPage />} />
        <Route path="settings" element={<SettingsPage />} />
        <Route path="import" element={<ImportPage />} />
        <Route path="budget" element={<BudgetPage />} />
        <Route path="connectors" element={<ConnectorsPage />} />
      </Route>
    </Routes>
  );
}
