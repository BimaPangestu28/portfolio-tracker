import { useSummary, useHistory, useRefreshPrices } from "../api/hooks";
import { NetWorthCard } from "../components/NetWorthCard";
import { PerformanceCards } from "../components/PerformanceCards";
import { AllocationDonut } from "../components/AllocationDonut";
import { DriftBars } from "../components/DriftBars";
import { HistoryChart } from "../components/HistoryChart";
import { QueryState } from "../components/QueryState";

export default function DashboardPage() {
  const summary = useSummary();
  const history = useHistory();
  const refresh = useRefreshPrices();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Dashboard</h1>
        <button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          className="rounded bg-blue-600 px-3 py-1.5 text-sm text-white disabled:opacity-50"
        >
          {refresh.isPending ? "Refreshing…" : "Refresh prices"}
        </button>
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        {summary.data && (
          <>
            <NetWorthCard s={summary.data} />
            <PerformanceCards s={summary.data} />
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <section>
                <h2 className="mb-2 text-sm font-semibold text-gray-700">Allocation</h2>
                <AllocationDonut allocation={summary.data.allocation} />
              </section>
              <section>
                <h2 className="mb-2 text-sm font-semibold text-gray-700">Target vs Actual</h2>
                <DriftBars allocation={summary.data.allocation} />
              </section>
            </div>
          </>
        )}
      </QueryState>

      <section>
        <h2 className="mb-2 text-sm font-semibold text-gray-700">Value History</h2>
        <QueryState isLoading={history.isLoading} error={history.error}>
          <HistoryChart snapshots={history.data ?? []} />
        </QueryState>
      </section>
    </div>
  );
}
