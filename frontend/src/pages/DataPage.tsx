/**
 * DataPage — 6-item IA consolidation
 * Hosts Connectors (Sinkron) + Import (Review) as page-level tabs.
 */

import { useState } from "react";
import ConnectorsPage from "./ConnectorsPage";
import ImportPage from "./ImportPage";

type Tab = "sync" | "review";

export default function DataPage() {
  const [tab, setTab] = useState<Tab>("sync");

  return (
    <div className="space-y-0">
      <div className="ptabs">
        <button
          type="button"
          className={`ptab${tab === "sync" ? " active" : ""}`}
          onClick={() => setTab("sync")}
        >
          Sinkron
        </button>
        <button
          type="button"
          className={`ptab${tab === "review" ? " active" : ""}`}
          onClick={() => setTab("review")}
        >
          Review Impor
        </button>
      </div>

      {tab === "sync" && <ConnectorsPage />}
      {tab === "review" && <ImportPage />}
    </div>
  );
}
