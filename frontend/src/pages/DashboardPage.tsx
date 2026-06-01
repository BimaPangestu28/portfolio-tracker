/**
 * DashboardPage — Phase 5C command-center redesign.
 *
 * Modules:
 *  1. Hero            — net worth (IDR primary / USD secondary) + day delta + sparkline
 *  2. KPI Cards       — Unrealized P&L, XIRR, Passive Yield, Savings Rate
 *  3. Kesehatan       — health status: runway, concentration, savings rate, diversification
 *  4. Alokasi         — donut + drift bars from summary.allocation
 *  5. Komposisi       — stacked area from insights.composition
 *  6. Rebalancing     — derived from out_of_band allocation items
 *  7. Tujuan Keuangan — goals progress bars
 *  8. Pergerakan Hari Ini — empty placeholder (per-holding movers not in backend yet)
 */

import { Loader2, RefreshCw, TrendingUp, TrendingDown, Minus, AlertTriangle, CheckCircle2, Info } from "lucide-react";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import { useSummary, useHistory, useInsights, useGoals, useRefreshPrices } from "../api/hooks";
import { formatIDR, formatUSD, formatPct, parseNum } from "../lib/format";
import { NetWorthSparkline } from "../components/charts/NetWorthSparkline";
import { AllocationDonutChart, AllocationDriftBars } from "../components/charts/AllocationDonutChart";
import { CompositionAreaChart } from "../components/charts/CompositionAreaChart";
import type { CategoryAllocation, Goal } from "../api/schemas";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

function signClass(v: string | number): string {
  const n = parseNum(v);
  if (n > 0) return "text-gain";
  if (n < 0) return "text-loss";
  return "text-muted-foreground";
}

function DeltaIcon({ v }: { v: string | number }) {
  const n = parseNum(v);
  if (n > 0) return <TrendingUp className="h-4 w-4 text-gain" />;
  if (n < 0) return <TrendingDown className="h-4 w-4 text-loss" />;
  return <Minus className="h-4 w-4 text-muted-foreground" />;
}

// ─────────────────────────────────────────────────────────────────────────────
// Module: Hero
// ─────────────────────────────────────────────────────────────────────────────

interface HeroProps {
  netWorthIdr: string;
  netWorthUsd: string;
  dayDeltaIdr: string;
  dayDeltaPct: string;
  snapshots: Array<{ as_of: string; total_idr: string; total_usd: string; breakdown_json: string }>;
}

function HeroSection({ netWorthIdr, netWorthUsd, dayDeltaIdr, dayDeltaPct, snapshots }: HeroProps) {
  const deltaPositive = parseNum(dayDeltaIdr) >= 0;
  return (
    <Card className="overflow-hidden">
      <CardContent className="pt-6">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-start sm:justify-between">
          <div className="flex-1 min-w-0">
            <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground mb-1">
              Total Kekayaan Bersih
            </p>
            <div className="flex items-baseline gap-3 flex-wrap">
              <span className="num text-4xl font-bold tracking-tight text-foreground">
                {formatIDR(netWorthIdr)}
              </span>
              <span className="num text-lg text-muted-foreground">{formatUSD(netWorthUsd)}</span>
            </div>
            <div className="mt-2 flex items-center gap-1.5">
              <DeltaIcon v={dayDeltaIdr} />
              <span className={cn("num text-sm font-medium", signClass(dayDeltaIdr))}>
                {parseNum(dayDeltaIdr) >= 0 ? "+" : ""}{formatIDR(dayDeltaIdr)}
              </span>
              <span className={cn("num text-xs", signClass(dayDeltaPct))}>
                ({parseNum(dayDeltaPct) >= 0 ? "+" : ""}{formatPct(dayDeltaPct)})
              </span>
              <span className="text-xs text-muted-foreground">hari ini</span>
            </div>
          </div>

          {/* Sparkline */}
          <div className="w-full sm:w-56 flex-shrink-0">
            <NetWorthSparkline snapshots={snapshots} points={12} />
          </div>
        </div>

        {/* Status pill */}
        <div className="mt-4 flex items-center gap-1.5">
          <div
            className={cn(
              "h-2 w-2 rounded-full",
              deltaPositive ? "bg-gain" : "bg-loss",
            )}
          />
          <span className="text-xs text-muted-foreground">
            {deltaPositive ? "Portofolio naik" : "Portofolio turun"} sejak kemarin
          </span>
        </div>
      </CardContent>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Module: KPI Cards
// ─────────────────────────────────────────────────────────────────────────────

interface KpiCardProps {
  label: string;
  value: string;
  sub?: string;
  tone?: "pos" | "neg" | "neutral";
  icon?: React.ReactNode;
}

function KpiCard({ label, value, sub, tone, icon }: KpiCardProps) {
  const valueColor =
    tone === "pos" ? "text-gain" : tone === "neg" ? "text-loss" : "text-foreground";
  return (
    <Card>
      <CardContent className="pt-5 pb-4">
        <div className="flex items-start justify-between gap-2">
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium uppercase tracking-wide text-muted-foreground truncate">
              {label}
            </p>
            <p className={cn("num mt-1.5 text-2xl font-bold tracking-tight", valueColor)}>
              {value}
            </p>
            {sub && <p className="mt-0.5 text-xs text-muted-foreground num">{sub}</p>}
          </div>
          {icon && (
            <div className="flex-shrink-0 text-muted-foreground/50 mt-0.5">{icon}</div>
          )}
        </div>
      </CardContent>
    </Card>
  );
}

interface KpiSectionProps {
  unrealizedPnl: string;
  xirr: number | null;
  yieldPct: string;
  dividendTtmIdr: string;
  savingsRate: string;
}

function KpiSection({ unrealizedPnl, xirr, yieldPct, dividendTtmIdr, savingsRate }: KpiSectionProps) {
  const xirrVal = xirr == null ? "—" : `${(xirr * 100).toFixed(1)}%`;
  const xirrTone: "pos" | "neg" | "neutral" =
    xirr == null ? "neutral" : xirr >= 0 ? "pos" : "neg";
  const pnlTone = (v: string): "pos" | "neg" | "neutral" => {
    const n = parseNum(v);
    if (n > 0) return "pos";
    if (n < 0) return "neg";
    return "neutral";
  };
  const savingsN = parseNum(savingsRate) * 100;

  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      <KpiCard
        label="Unrealized P&L"
        value={formatIDR(unrealizedPnl)}
        tone={pnlTone(unrealizedPnl)}
      />
      <KpiCard
        label="XIRR (annualized)"
        value={xirrVal}
        tone={xirrTone}
        sub={xirr != null ? "return tahunan efektif" : undefined}
      />
      <KpiCard
        label="Passive Yield"
        value={formatPct(yieldPct)}
        sub={`Dividen TTM ${formatIDR(dividendTtmIdr)}`}
        tone={parseNum(yieldPct) > 0 ? "pos" : "neutral"}
      />
      <KpiCard
        label="Savings Rate"
        value={`${(savingsN).toFixed(0)}%`}
        sub={savingsN >= 20 ? "Sehat ✓" : "Perlu ditingkatkan"}
        tone={savingsN >= 20 ? "pos" : savingsN >= 10 ? "neutral" : "neg"}
      />
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Module: Kesehatan Portofolio (health)
// ─────────────────────────────────────────────────────────────────────────────

interface HealthItemProps {
  label: string;
  status: "sehat" | "perhatikan" | "kritis";
  value: string;
  note?: string;
}

function HealthItem({ label, status, value, note }: HealthItemProps) {
  const pillClass =
    status === "sehat"
      ? "bg-gain-soft text-gain"
      : status === "perhatikan"
        ? "bg-warn-soft text-warn"
        : "bg-loss-soft text-loss";
  const pillText = status === "sehat" ? "Sehat" : status === "perhatikan" ? "Perhatikan" : "Kritis";
  const Icon =
    status === "sehat" ? CheckCircle2 : status === "perhatikan" ? AlertTriangle : AlertTriangle;

  return (
    <div className="flex items-start gap-3 py-3 border-b last:border-b-0 border-border">
      <Icon
        className={cn(
          "mt-0.5 h-4 w-4 flex-shrink-0",
          status === "sehat" ? "text-gain" : status === "perhatikan" ? "text-warn" : "text-loss",
        )}
      />
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-sm font-medium text-foreground">{label}</span>
          <span className={cn("rounded-full px-2 py-0.5 text-[10px] font-semibold", pillClass)}>
            {pillText}
          </span>
        </div>
        <p className="num mt-0.5 text-sm font-semibold text-foreground">{value}</p>
        {note && <p className="mt-0.5 text-xs text-muted-foreground">{note}</p>}
      </div>
    </div>
  );
}

interface HealthSectionProps {
  runwayMonths: string;
  concentration: { symbol: string; pct: string } | null;
  savingsRate: string;
  categoryCount: number;
}

function HealthSection({ runwayMonths, concentration, savingsRate, categoryCount }: HealthSectionProps) {
  const runway = parseNum(runwayMonths);
  const savingsN = parseNum(savingsRate) * 100;

  const runwayStatus: "sehat" | "perhatikan" | "kritis" =
    runway >= 6 ? "sehat" : runway >= 3 ? "perhatikan" : "kritis";
  const runwayNote =
    runway >= 6
      ? "Dana darurat mencukupi (≥ 6 bulan)"
      : runway >= 3
        ? "Dana darurat di bawah ideal (< 6 bulan)"
        : "Dana darurat kritis (< 3 bulan)";

  const concPct = concentration ? parseNum(concentration.pct) : 0;
  const concStatus: "sehat" | "perhatikan" | "kritis" =
    concPct < 25 ? "sehat" : concPct < 40 ? "perhatikan" : "kritis";

  const savingsStatus: "sehat" | "perhatikan" | "kritis" =
    savingsN >= 20 ? "sehat" : savingsN >= 10 ? "perhatikan" : "kritis";

  const divStatus: "sehat" | "perhatikan" | "kritis" =
    categoryCount >= 4 ? "sehat" : categoryCount >= 2 ? "perhatikan" : "kritis";

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-semibold">Kesehatan Portofolio</CardTitle>
      </CardHeader>
      <CardContent>
        <HealthItem
          label="Dana Darurat"
          status={runwayStatus}
          value={`${runway.toFixed(1)} bulan`}
          note={runwayNote}
        />
        <HealthItem
          label="Konsentrasi Tertinggi"
          status={concStatus}
          value={concentration ? `${concentration.symbol} — ${formatPct(concentration.pct)}` : "—"}
          note={
            concentration
              ? concPct < 25
                ? "Konsentrasi terdiversifikasi dengan baik"
                : "Pertimbangkan untuk diversifikasi lebih"
              : "Tidak ada posisi"
          }
        />
        <HealthItem
          label="Savings Rate"
          status={savingsStatus}
          value={`${savingsN.toFixed(0)}%`}
          note={savingsN >= 20 ? "Di atas target 20%" : "Target minimal 20% dari pendapatan"}
        />
        <HealthItem
          label="Diversifikasi Kategori"
          status={divStatus}
          value={`${categoryCount} kategori aktif`}
          note={categoryCount >= 4 ? "Portofolio terdiversifikasi" : "Tambah kategori untuk diversifikasi lebih"}
        />
      </CardContent>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Module: Rekomendasi Rebalancing
// ─────────────────────────────────────────────────────────────────────────────

function RebalancingSection({ allocation }: { allocation: CategoryAllocation[] }) {
  const outOfBand = allocation.filter((c) => c.out_of_band);

  if (outOfBand.length === 0) {
    return (
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-semibold">Rekomendasi Rebalancing</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center gap-2 py-6 text-center">
            <CheckCircle2 className="h-8 w-8 text-gain" />
            <p className="text-sm font-medium text-foreground">Semua dalam batas target!</p>
            <p className="text-xs text-muted-foreground">
              Tidak ada aksi rebalancing yang diperlukan saat ini.
            </p>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-semibold">Rekomendasi Rebalancing</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {outOfBand.map((c) => {
            const reb = parseNum(c.rebalance_idr);
            const isBuy = reb > 0;
            return (
              <div
                key={c.category_id}
                className={cn(
                  "flex items-center justify-between rounded-lg px-3 py-2.5 text-sm",
                  isBuy ? "bg-gain-soft" : "bg-loss-soft",
                )}
              >
                <div className="flex items-center gap-2">
                  <span
                    className={cn(
                      "rounded-full px-2 py-0.5 text-xs font-semibold",
                      isBuy
                        ? "bg-gain/20 text-gain"
                        : "bg-loss/20 text-loss",
                    )}
                  >
                    {isBuy ? "BELI" : "PANGKAS"}
                  </span>
                  <span className="font-medium text-foreground">{c.name}</span>
                </div>
                <span className={cn("num font-semibold", isBuy ? "text-gain" : "text-loss")}>
                  {formatIDR(String(Math.abs(reb)))}
                </span>
              </div>
            );
          })}
        </div>
        <p className="mt-3 text-xs text-muted-foreground">
          <Info className="mr-1 inline h-3 w-3" />
          Estimasi berdasarkan target alokasi. Konsultasikan sebelum bertransaksi.
        </p>
      </CardContent>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Module: Tujuan Keuangan (Goals)
// ─────────────────────────────────────────────────────────────────────────────

function GoalProgress({ goal }: { goal: Goal }) {
  const current = parseNum(goal.current_idr);
  const target = parseNum(goal.target_idr);
  const pct = target > 0 ? Math.min(100, (current / target) * 100) : 0;
  const done = pct >= 100;

  return (
    <div className="py-3 border-b last:border-b-0 border-border">
      <div className="flex items-center justify-between mb-1.5 gap-2 flex-wrap">
        <span className="text-sm font-medium text-foreground">{goal.label}</span>
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span className="num">{formatIDR(goal.current_idr)}</span>
          <span>/</span>
          <span className="num">{formatIDR(goal.target_idr)}</span>
        </div>
      </div>
      <div className="h-1.5 w-full rounded-full bg-muted">
        <div
          className={cn("h-1.5 rounded-full transition-all", done ? "bg-gain" : "bg-primary")}
          style={{ width: `${pct}%` }}
        />
      </div>
      <div className="mt-1 flex items-center justify-between text-[10px] text-muted-foreground">
        <span>{goal.current_kind === "portfolio" ? "dari portofolio" : goal.current_kind === "savings" ? "dari tabungan" : "manual"}</span>
        <span className={cn("num font-semibold", done ? "text-gain" : "text-muted-foreground")}>
          {pct.toFixed(0)}%{done ? " ✓" : ""}
        </span>
      </div>
    </div>
  );
}

function GoalsSection({ goals }: { goals: Goal[] }) {
  if (goals.length === 0) {
    return (
      <Card>
        <CardHeader className="pb-2">
          <CardTitle className="text-sm font-semibold">Tujuan Keuangan</CardTitle>
        </CardHeader>
        <CardContent>
          <div className="flex flex-col items-center gap-2 py-6 text-center">
            <div className="text-3xl">🎯</div>
            <p className="text-sm font-medium text-foreground">Belum ada tujuan keuangan</p>
            <p className="text-xs text-muted-foreground">
              Tetapkan target keuangan Anda untuk memantau progres.
            </p>
            <Button variant="outline" size="sm" asChild>
              <a href="/planner">Tambah tujuan</a>
            </Button>
          </div>
        </CardContent>
      </Card>
    );
  }

  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-semibold">Tujuan Keuangan</CardTitle>
      </CardHeader>
      <CardContent>
        {goals.map((g) => (
          <GoalProgress key={g.id} goal={g} />
        ))}
      </CardContent>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Module: Pergerakan Hari Ini (movers) — empty placeholder
// ─────────────────────────────────────────────────────────────────────────────

function MoversSection() {
  return (
    <Card>
      <CardHeader className="pb-2">
        <CardTitle className="text-sm font-semibold">Pergerakan Hari Ini</CardTitle>
      </CardHeader>
      <CardContent>
        <div className="flex flex-col items-center gap-2 py-6 text-center">
          <div className="text-3xl">📡</div>
          <p className="text-sm font-medium text-foreground">Pergerakan per-aset menyusul</p>
          <p className="text-xs text-muted-foreground">
            Fitur ini membutuhkan data intraday per-holding yang belum tersedia di backend.
          </p>
        </div>
      </CardContent>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Skeletons
// ─────────────────────────────────────────────────────────────────────────────

function HeroSkeleton() {
  return (
    <Card>
      <CardContent className="pt-6">
        <Skeleton className="h-5 w-40 mb-2" />
        <Skeleton className="h-10 w-72 mb-2" />
        <Skeleton className="h-4 w-32" />
      </CardContent>
    </Card>
  );
}

function KpiSkeleton() {
  return (
    <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
      {[0, 1, 2, 3].map((i) => (
        <Card key={i}>
          <CardContent className="pt-5 pb-4">
            <Skeleton className="h-3 w-24 mb-2" />
            <Skeleton className="h-8 w-32" />
          </CardContent>
        </Card>
      ))}
    </div>
  );
}

function SectionSkeleton({ rows = 3 }: { rows?: number }) {
  return (
    <Card>
      <CardHeader className="pb-2">
        <Skeleton className="h-4 w-40" />
      </CardHeader>
      <CardContent>
        <div className="space-y-3">
          {Array.from({ length: rows }).map((_, i) => (
            <Skeleton key={i} className="h-8 w-full" />
          ))}
        </div>
      </CardContent>
    </Card>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Main Page
// ─────────────────────────────────────────────────────────────────────────────

export default function DashboardPage() {
  const summary = useSummary();
  const history = useHistory();
  const insights = useInsights();
  const goals = useGoals();
  const refresh = useRefreshPrices();

  const isLoadingCore = summary.isLoading || insights.isLoading;

  return (
    <div className="space-y-5">
      {/* ── Page header ───────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-lg font-semibold tracking-tight">Dashboard</h1>
          <p className="text-xs text-muted-foreground">Command center keuangan Anda</p>
        </div>
        <Button
          type="button"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
          variant="outline"
          size="sm"
          className="gap-1.5"
        >
          {refresh.isPending ? (
            <><Loader2 className="h-3.5 w-3.5 animate-spin" /> Memperbarui…</>
          ) : (
            <><RefreshCw className="h-3.5 w-3.5" /> Refresh harga</>
          )}
        </Button>
      </div>

      {/* ── 1. Hero ────────────────────────────────────────────────────────── */}
      {isLoadingCore ? (
        <HeroSkeleton />
      ) : summary.error || insights.error ? (
        <Card>
          <CardContent className="pt-6">
            <p className="text-sm text-destructive">
              Gagal memuat data: {summary.error instanceof Error ? summary.error.message : insights.error instanceof Error ? insights.error.message : "unknown"}
            </p>
          </CardContent>
        </Card>
      ) : summary.data && insights.data ? (
        <HeroSection
          netWorthIdr={summary.data.net_worth_idr}
          netWorthUsd={summary.data.net_worth_usd}
          dayDeltaIdr={insights.data.day_delta_idr}
          dayDeltaPct={insights.data.day_delta_pct}
          snapshots={history.data ?? []}
        />
      ) : null}

      {/* ── 2. KPI Cards ────────────────────────────────────────────────────── */}
      {isLoadingCore ? (
        <KpiSkeleton />
      ) : summary.data && insights.data ? (
        <KpiSection
          unrealizedPnl={summary.data.total_unrealized_pnl_idr}
          xirr={summary.data.xirr}
          yieldPct={insights.data.yield_pct}
          dividendTtmIdr={insights.data.dividend_ttm_idr}
          savingsRate={insights.data.savings_rate}
        />
      ) : null}

      {/* ── 3. Kesehatan + Alokasi (side by side on lg) ─────────────────── */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        {/* Health */}
        {isLoadingCore ? (
          <SectionSkeleton rows={4} />
        ) : insights.data ? (
          <HealthSection
            runwayMonths={insights.data.runway_months}
            concentration={insights.data.concentration}
            savingsRate={insights.data.savings_rate}
            categoryCount={summary.data?.allocation.filter((c) => Number(c.actual_value_idr) > 0).length ?? 0}
          />
        ) : null}

        {/* Alokasi */}
        {summary.isLoading ? (
          <SectionSkeleton rows={3} />
        ) : summary.data ? (
          <Card>
            <CardHeader className="pb-2">
              <CardTitle className="text-sm font-semibold">Alokasi Portofolio</CardTitle>
            </CardHeader>
            <CardContent>
              <AllocationDonutChart allocation={summary.data.allocation} />
            </CardContent>
          </Card>
        ) : null}
      </div>

      {/* ── 4. Drift Bars ──────────────────────────────────────────────────── */}
      {summary.isLoading ? (
        <SectionSkeleton rows={3} />
      ) : summary.data ? (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-semibold">Target vs Aktual Alokasi</CardTitle>
          </CardHeader>
          <CardContent>
            <AllocationDriftBars allocation={summary.data.allocation} />
          </CardContent>
        </Card>
      ) : null}

      {/* ── 5. Komposisi Kekayaan ──────────────────────────────────────────── */}
      {insights.isLoading ? (
        <SectionSkeleton rows={4} />
      ) : insights.data ? (
        <Card>
          <CardHeader className="pb-2">
            <CardTitle className="text-sm font-semibold">Komposisi Kekayaan</CardTitle>
          </CardHeader>
          <CardContent>
            <CompositionAreaChart composition={insights.data.composition} />
          </CardContent>
        </Card>
      ) : null}

      {/* ── 6. Rebalancing + Goals (side by side on lg) ──────────────────── */}
      <div className="grid grid-cols-1 gap-5 lg:grid-cols-2">
        {/* Rebalancing */}
        {summary.isLoading ? (
          <SectionSkeleton rows={2} />
        ) : summary.data ? (
          <RebalancingSection allocation={summary.data.allocation} />
        ) : null}

        {/* Goals */}
        {goals.isLoading ? (
          <SectionSkeleton rows={2} />
        ) : (
          <GoalsSection goals={goals.data ?? []} />
        )}
      </div>

      {/* ── 7. Pergerakan Hari Ini ─────────────────────────────────────────── */}
      <MoversSection />
    </div>
  );
}
