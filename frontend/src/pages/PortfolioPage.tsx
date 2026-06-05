/**
 * PortfolioPage — 6-item IA consolidation
 * Hosts Holdings + Transactions + Performance as page-level tabs.
 */

import { useState } from "react";
import HoldingsPage from "./HoldingsPage";
import TransactionsPage from "./TransactionsPage";
import PerformancePage from "./PerformancePage";

type Tab = "holdings" | "transactions" | "performance";

export default function PortfolioPage() {
  const [tab, setTab] = useState<Tab>("holdings");

  return (
    <div className="space-y-0">
      <div className="ptabs">
        <button
          type="button"
          className={`ptab${tab === "holdings" ? " active" : ""}`}
          onClick={() => setTab("holdings")}
        >
          Holdings
        </button>
        <button
          type="button"
          className={`ptab${tab === "transactions" ? " active" : ""}`}
          onClick={() => setTab("transactions")}
        >
          Transaksi
        </button>
        <button
          type="button"
          className={`ptab${tab === "performance" ? " active" : ""}`}
          onClick={() => setTab("performance")}
        >
          Performa
        </button>
      </div>

      {tab === "holdings" && <HoldingsPage />}
      {tab === "transactions" && <TransactionsPage />}
      {tab === "performance" && <PerformancePage />}
    </div>
  );
}
