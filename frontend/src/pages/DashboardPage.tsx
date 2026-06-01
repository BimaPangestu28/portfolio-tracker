import { Loader2, RefreshCw } from "lucide-react";
import { useSummary, useHistory, useRefreshPrices } from "../api/hooks";
import { NetWorthCard } from "../components/NetWorthCard";
import { PerformanceCards } from "../components/PerformanceCards";
import { AllocationDonut } from "../components/AllocationDonut";
import { DriftBars } from "../components/DriftBars";
import { HistoryChart } from "../components/HistoryChart";
import { QueryState } from "../components/QueryState";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

export default function DashboardPage() {
  const summary = useSummary();
  const history = useHistory();
  const refresh = useRefreshPrices();

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <h1 className="text-xl font-semibold">Dashboard</h1>
        <Button type="button" onClick={() => refresh.mutate()} disabled={refresh.isPending} size="sm">
          {refresh.isPending ? (
            <>
              <Loader2 className="animate-spin" /> Refreshing…
            </>
          ) : (
            <>
              <RefreshCw /> Refresh prices
            </>
          )}
        </Button>
      </div>

      <QueryState isLoading={summary.isLoading} error={summary.error}>
        {summary.data && (
          <>
            <NetWorthCard s={summary.data} />
            <PerformanceCards s={summary.data} />
            <div className="grid grid-cols-1 gap-4 lg:grid-cols-2">
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Allocation</CardTitle>
                </CardHeader>
                <CardContent>
                  <AllocationDonut allocation={summary.data.allocation} />
                </CardContent>
              </Card>
              <Card>
                <CardHeader>
                  <CardTitle className="text-sm">Target vs Actual</CardTitle>
                </CardHeader>
                <CardContent>
                  <DriftBars allocation={summary.data.allocation} />
                </CardContent>
              </Card>
            </div>
          </>
        )}
      </QueryState>

      <Card>
        <CardHeader>
          <CardTitle className="text-sm">Value History</CardTitle>
        </CardHeader>
        <CardContent>
          <QueryState isLoading={history.isLoading} error={history.error}>
            <HistoryChart snapshots={history.data ?? []} />
          </QueryState>
        </CardContent>
      </Card>
    </div>
  );
}
