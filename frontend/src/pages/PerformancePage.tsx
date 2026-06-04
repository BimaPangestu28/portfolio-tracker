/**
 * PerformancePage — "Performa" page.
 *
 * Renders a time-weighted return (TWR) curve plus risk-metric cards
 * (total/annualized return, max drawdown, volatility) with IDR/USD base
 * and period toggles. Data comes from GET /portfolio/performance via
 * usePerformance(); snake_case JSON is decoded by PerformanceSchema.
 */

import { useState } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  Tooltip,
  ResponsiveContainer,
  CartesianGrid,
} from "recharts";
import { usePerformance } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { StatCard } from "../components/StatCard";

/** Period options for the curve window. */
const PERIODS: { value: string; label: string }[] = [
  { value: "1m", label: "1B" },
  { value: "3m", label: "3B" },
  { value: "6m", label: "6B" },
  { value: "ytd", label: "YTD" },
  { value: "1y", label: "1Th" },
  { value: "all", label: "Semua" },
];

const BASES: { value: "idr" | "usd"; label: string }[] = [
  { value: "idr", label: "IDR" },
  { value: "usd", label: "USD" },
];

/** Formats a ratio (e.g. 0.1) as a signed percentage string. */
function formatPercent(ratio: number): string {
  return `${(ratio * 100).toFixed(2)}%`;
}

/** Maps a return ratio to a StatCard tone. */
function returnTone(ratio: number): "pos" | "neg" | "neutral" {
  if (ratio > 0) return "pos";
  if (ratio < 0) return "neg";
  return "neutral";
}

/** Reuses the app-wide `pt-seg` segmented control (see AppShell.SegControl). */
function Segmented<T extends string>({
  value,
  onChange,
  options,
}: {
  value: T;
  onChange: (value: T) => void;
  options: { value: T; label: string }[];
}) {
  return (
    <div className="pt-seg">
      {options.map((option) => (
        <button
          key={option.value}
          type="button"
          className={value === option.value ? "active" : ""}
          onClick={() => onChange(option.value)}
        >
          {option.label}
        </button>
      ))}
    </div>
  );
}

export default function PerformancePage() {
  const [base, setBase] = useState<"idr" | "usd">("idr");
  const [period, setPeriod] = useState("1y");
  const performanceQuery = usePerformance(base, period);
  const performance = performanceQuery.data;

  const chartData = (performance?.points ?? []).map((point) => ({
    date: point.date,
    returnPct: point.cum_return * 100,
  }));

  return (
    <div className="flex flex-col gap-4">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <h1 className="text-xl font-semibold tracking-tight">Performa</h1>
          <p className="text-sm text-muted-foreground">Return tertimbang waktu (TWR)</p>
        </div>
        <div className="flex flex-wrap gap-2">
          <Segmented value={base} onChange={setBase} options={BASES} />
          <Segmented value={period} onChange={setPeriod} options={PERIODS} />
        </div>
      </div>

      <QueryState isLoading={performanceQuery.isLoading} error={performanceQuery.error}>
        {performance?.insufficient_data ? (
          <div className="rounded-lg border bg-card p-10 text-center">
            <p className="text-sm text-muted-foreground">
              Belum cukup data — snapshot harian terkumpul seiring waktu.
            </p>
          </div>
        ) : (
          <>
            <div className="grid grid-cols-1 gap-3 sm:grid-cols-2 lg:grid-cols-4">
              <StatCard
                label="Total return"
                value={formatPercent(performance?.metrics.total_return ?? 0)}
                tone={returnTone(performance?.metrics.total_return ?? 0)}
              />
              <StatCard
                label="Annualized"
                value={formatPercent(performance?.metrics.annualized ?? 0)}
                tone={returnTone(performance?.metrics.annualized ?? 0)}
              />
              <StatCard
                label="Max drawdown"
                value={formatPercent(performance?.metrics.max_drawdown ?? 0)}
                tone={(performance?.metrics.max_drawdown ?? 0) < 0 ? "neg" : "neutral"}
              />
              <StatCard
                label="Volatility"
                value={formatPercent(performance?.metrics.volatility ?? 0)}
                tone="neutral"
              />
            </div>

            <div className="h-80 w-full rounded-lg border bg-card p-4">
              <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={chartData} margin={{ top: 10, right: 20, bottom: 0, left: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--border))" vertical={false} />
                  <XAxis
                    dataKey="date"
                    fontSize={11}
                    minTickGap={32}
                    stroke="hsl(var(--muted-foreground))"
                    tickLine={false}
                    axisLine={false}
                  />
                  <YAxis
                    tickFormatter={(value: number) => `${value.toFixed(0)}%`}
                    width={48}
                    fontSize={11}
                    stroke="hsl(var(--muted-foreground))"
                    tickLine={false}
                    axisLine={false}
                  />
                  <Tooltip
                    formatter={(value: number) => `${value.toFixed(2)}%`}
                    contentStyle={{
                      background: "hsl(var(--popover))",
                      border: "1px solid hsl(var(--border))",
                      borderRadius: "var(--radius)",
                      color: "hsl(var(--popover-foreground))",
                      fontSize: 12,
                    }}
                  />
                  <Area
                    type="monotone"
                    dataKey="returnPct"
                    stroke="hsl(var(--chart-1))"
                    strokeWidth={1.5}
                    fill="hsl(var(--chart-1))"
                    fillOpacity={0.15}
                    dot={false}
                  />
                </AreaChart>
              </ResponsiveContainer>
            </div>
          </>
        )}
      </QueryState>
    </div>
  );
}
