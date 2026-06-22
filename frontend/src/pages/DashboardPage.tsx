/**
 * DashboardPage — fidelity-pass to match claude-design-source/app/page_dashboard.jsx
 *
 * Section order (matches source):
 *  1. Hero card (left, net worth + sparkline + day delta + 12mo pct)
 *     + 2x2 KPI stat-cards (right) — one grid row
 *  2. Alokasi Aset (donut + legend) + Target vs Aktual (drift bars) — side by side
 *  3. Rekomendasi Rebalancing + Kesehatan Portofolio — side by side
 *  4. Komposisi Kekayaan (stacked area) — full width
 *  5. Pergerakan Hari Ini + Tujuan Keuangan — side by side
 *
 * Data wiring unchanged — real hooks, real formatting via format.ts.
 * Movers section keeps honest empty state (no backend data).
 */

import { useState } from "react";
import { Link } from "react-router-dom";
import {
  Loader2,
  RefreshCw,
  TrendingUp,
  Scale,
  Coins,
  Banknote,
  Shield,
  PieChart as PieIcon,
  Target,
  CheckCircle2,
  AlertTriangle,
  Activity,
  Trophy,
} from "lucide-react";
import { cn } from "@/lib/utils";

import {
  useSummary, useHistory, useInsights, useGoals, useRefreshPrices,
  useMovers, useBenchmark, useReviewItems, useTransactions, useInstruments,
} from "../api/hooks";
import { formatIDR, formatIDRShort, formatUSD, formatPct, formatQty, parseNum } from "../lib/format";
import { txTone, txLabel } from "../lib/txn";
import { DashboardAgendaCard } from "../components/DashboardAgendaCard";
import { DashboardTodoCard } from "../components/DashboardTodoCard";
import { DashboardReminderCard } from "../components/DashboardReminderCard";
import { DashboardInboxCard } from "../components/DashboardInboxCard";
import { HyperliquidCard } from "../components/HyperliquidCard";
import { HeroSparkline } from "../components/charts/HeroSparkline";
import { DonutChart } from "../components/charts/DonutChart";
import { DriftBarsChart } from "../components/charts/DriftBarsChart";
import { StackedAreaChart } from "../components/charts/StackedAreaChart";
import type {
  CategoryAllocation, Goal, Mover, BenchmarkRow, Position, Instrument, Transaction,
} from "../api/schemas";
import { UNCATEGORIZED_CATEGORY_ID } from "../api/schemas";

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/** Maps a category id/name to its CSS var color */
function categoryColor(name: string): string {
  const key = (name ?? "").toLowerCase();
  if (key.includes("saham") || key.includes("equity") || key.includes("stock"))
    return "hsl(var(--cat-saham))";
  if (key.includes("crypto") || key.includes("kripto")) return "hsl(var(--cat-crypto))";
  if (key.includes("reksa") || key.includes("mutual") || key.includes("fund"))
    return "hsl(var(--cat-reksadana))";
  if (key.includes("kas") || key.includes("cash") || key.includes("liquid"))
    return "hsl(var(--cat-kas))";
  if (key.includes("obligasi") || key.includes("bond")) return "hsl(var(--cat-obligasi))";
  if (key.includes("emas") || key.includes("gold")) return "hsl(var(--cat-emas))";
  if (
    key.includes("properti") ||
    key.includes("real estate") ||
    key.includes("property")
  )
    return "hsl(var(--cat-properti))";
  return "hsl(var(--cat-lainnya))";
}

// ─────────────────────────────────────────────────────────────────────────────
// StatCard — matches source's StatCard component (stat-card class)
// ─────────────────────────────────────────────────────────────────────────────

interface StatCardProps {
  label: string;
  icon: React.ReactNode;
  value: string;
  sub?: React.ReactNode;
  tone?: string;
  loading?: boolean;
}

function StatCard({ label, icon, value, sub, tone, loading }: StatCardProps) {
  if (loading) {
    return (
      <div className="card stat-card">
        <div className="skeleton" style={{ width: 90, height: 12 }} />
        <div className="skeleton" style={{ width: 140, height: 24 }} />
        <div className="skeleton" style={{ width: 70, height: 12 }} />
      </div>
    );
  }
  return (
    <div className="card stat-card">
      <div className="stat-label">
        {icon}
        {label}
      </div>
      <div className={cn("stat-value num", tone ?? "")}>{value}</div>
      {sub}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Hero Section — net worth card + 2×2 stat cards
// ─────────────────────────────────────────────────────────────────────────────

/** Year-window trend: percent change across snapshots within the last 365
 *  days, with an honest label — "12 bln" only when history actually spans a
 *  year, otherwise "sejak <oldest date in window>". */
export function yearTrend(
  snapshots: Array<{ as_of: string; total_idr: string }>,
  now: Date,
): { pct: number; label: string } | null {
  const cutoff = now.getTime() - 365 * 24 * 3600 * 1000;
  const window = snapshots.filter((s) => new Date(s.as_of).getTime() >= cutoff);
  if (window.length < 2) return null;
  const first = Number(window[0].total_idr);
  const last = Number(window[window.length - 1].total_idr);
  if (!Number.isFinite(first) || first === 0) return null;
  const pct = ((last - first) / first) * 100;
  const coversFullYear =
    snapshots.length > window.length || new Date(window[0].as_of).getTime() <= cutoff + 7 * 24 * 3600 * 1000;
  const label = coversFullYear
    ? "12 bln"
    : `sejak ${new Date(window[0].as_of).toLocaleDateString("id-ID", { day: "numeric", month: "short" })}`;
  return { pct, label };
}

interface HeroProps {
  netWorthIdr: string;
  netWorthUsd: string;
  dayDeltaIdr: string;
  dayDeltaPct: string;
  unrealizedPnl: string;
  unrealizedPricePnl: string;
  unrealizedFxPnl: string;
  fxIncomplete: boolean;
  xirr: number | null;
  yieldPct: string;
  dividendTtmIdr: string;
  savingsRate: string;
  snapshots: Array<{ as_of: string; total_idr: string; total_usd: string; breakdown_json: string }>;
  staleCount: number;
}

function HeroSection({
  netWorthIdr,
  netWorthUsd,
  dayDeltaIdr,
  dayDeltaPct,
  unrealizedPnl,
  unrealizedPricePnl,
  unrealizedFxPnl,
  fxIncomplete,
  xirr,
  yieldPct,
  dividendTtmIdr,
  savingsRate,
  snapshots,
  staleCount,
}: HeroProps) {
  const deltaPos = parseNum(dayDeltaIdr) >= 0;
  const pnlN = parseNum(unrealizedPnl);
  const pnlPos = pnlN >= 0;
  // P&L as a percentage of cost basis (net worth = market value, so
  // cost basis = net worth - unrealized P&L).
  const pnlCostBasis = parseNum(netWorthIdr) - pnlN;
  const pnlPct = pnlCostBasis !== 0 ? (pnlN / pnlCostBasis) * 100 : 0;
  const savingsN = parseNum(savingsRate);
  const yieldN = parseNum(yieldPct);
  const xirrVal = xirr == null ? "—" : `${(xirr * 100).toFixed(1)}%`;

  const trend = yearTrend(snapshots, new Date());

  return (
    <div className="grid gap-5 lay-2-15">
      {/* Hero net-worth card */}
      <div
        className="card card-pad"
        style={{
          display: "flex",
          flexDirection: "column",
          justifyContent: "space-between",
          gap: 16,
          minHeight: 200,
        }}
      >
        <div className="flex items-center justify-between">
          <span className="t-label" style={{ whiteSpace: "nowrap" }}>
            Total Kekayaan Bersih
          </span>
          {staleCount > 0 ? (
            <span className="badge badge-warn" title="Beberapa harga gagal di-refresh">
              <span className="badge-dot" style={{ background: "currentColor" }} />
              {staleCount} harga basi
            </span>
          ) : (
            <span className="badge badge-gain">
              <span className="badge-dot" style={{ background: "currentColor" }} />
              Live
            </span>
          )}
        </div>

        <div className="flex col gap-2">
          <div className="flex items-end gap-3 flex-wrap">
            <span className="t-display num">{formatIDR(netWorthIdr)}</span>
            <span
              className="num t-muted"
              style={{ fontSize: 17, fontWeight: 500, paddingBottom: 5 }}
            >
              ≈ {formatUSD(netWorthUsd)}
            </span>
          </div>
          <div className="flex items-center gap-3 flex-wrap">
            <span className={cn("stat-delta num", deltaPos ? "gain" : "loss")}>
              {deltaPos ? "▲" : "▼"}
              {formatIDR(dayDeltaIdr)} ({formatPct(dayDeltaPct)})
            </span>
            <span className="t-xs t-muted">
              hari ini
              {trend &&
                ` · ${trend.label} ${trend.pct >= 0 ? "+" : ""}${trend.pct.toFixed(1)}%`}
            </span>
          </div>
        </div>

        {/* Inline sparkline */}
        <HeroSparkline snapshots={snapshots} />
      </div>

      {/* 2×2 KPI stat-cards */}
      <div className="grid gap-5 lay-2-sm">
        <StatCard
          label="Unrealized P&L"
          icon={<TrendingUp size={15} />}
          value={formatIDR(unrealizedPnl)}
          tone={pnlPos ? "gain" : "loss"}
          sub={
            <div className="flex col" style={{ gap: 2 }}>
              <span className={cn("stat-delta num", pnlPos ? "gain" : "loss")}>
                {pnlPos ? "▲" : "▼"} {formatPct(pnlPct)}
                {fxIncomplete && (
                  <span className="badge badge-warn" style={{ marginLeft: 6 }} title="Sebagian transaksi tidak punya kurs historis — dekomposisi FX tidak lengkap">
                    FX?
                  </span>
                )}
              </span>
              <span className="t-xs t-muted num">
                Harga {formatIDR(unrealizedPricePnl)} · FX {formatIDR(unrealizedFxPnl)}
              </span>
            </div>
          }
        />
        <StatCard
          label="XIRR"
          icon={<Scale size={15} />}
          value={xirrVal}
          tone={xirr != null && xirr >= 0 ? "gain" : ""}
          sub={<span className="t-xs t-muted">tahunan, sejak awal</span>}
        />
        <StatCard
          label="Imbal Hasil Pasif"
          icon={<Coins size={15} />}
          value={`${yieldN.toFixed(2).replace(".", ",")}%`}
          sub={
            <span className="t-xs t-muted">
              {formatIDR(dividendTtmIdr)} dividen 12 bln
            </span>
          }
        />
        <StatCard
          label="Tingkat Menabung"
          icon={<Banknote size={15} />}
          value={`${savingsN.toFixed(0)}%`}
          tone={savingsN >= 20 ? "gain" : savingsN >= 10 ? "warn" : "loss"}
          sub={<span className="t-xs t-muted">dari pemasukan bulan ini</span>}
        />
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Loading skeleton for hero row
// ─────────────────────────────────────────────────────────────────────────────

function HeroRowSkeleton() {
  return (
    <div className="grid gap-5 lay-2-15">
      <div className="card card-pad" style={{ minHeight: 200 }}>
        <div className="skeleton" style={{ width: 200, height: 12, marginBottom: 16 }} />
        <div className="skeleton" style={{ width: 320, height: 44, marginBottom: 12 }} />
        <div className="skeleton" style={{ width: "100%", height: 46 }} />
      </div>
      <div className="grid gap-5 lay-2-sm">
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className="card stat-card">
            <div className="skeleton" style={{ width: 90, height: 12 }} />
            <div className="skeleton" style={{ width: 140, height: 24 }} />
            <div className="skeleton" style={{ width: 70, height: 12 }} />
          </div>
        ))}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// MetricRow — matches source's MetricRow for health indicators
// ─────────────────────────────────────────────────────────────────────────────

type HealthStatus = "good" | "warn" | "bad";

const STATUS_STYLE: Record<HealthStatus, { color: string; soft: string; label: string }> = {
  good: {
    color: "hsl(var(--gain))",
    soft: "hsl(var(--gain-soft))",
    label: "Sehat",
  },
  warn: {
    color: "hsl(var(--warn))",
    soft: "hsl(var(--warn-soft))",
    label: "Perhatikan",
  },
  bad: {
    color: "hsl(var(--loss))",
    soft: "hsl(var(--loss-soft))",
    label: "Risiko",
  },
};

interface MetricRowProps {
  icon: React.ReactNode;
  label: string;
  value: string;
  hint?: string;
  status: HealthStatus;
}

function MetricRow({ icon, label, value, hint, status }: MetricRowProps) {
  const s = STATUS_STYLE[status];
  return (
    <div
      className="flex items-center gap-3"
      style={{ padding: "12px 0", borderBottom: "1px solid hsl(var(--border))" }}
    >
      <span
        style={{
          width: 36,
          height: 36,
          borderRadius: 10,
          flexShrink: 0,
          display: "grid",
          placeItems: "center",
          color: s.color,
          background: s.soft,
        }}
      >
        {icon}
      </span>
      <div className="flex-1" style={{ minWidth: 0 }}>
        <div className="flex items-center justify-between gap-2">
          <span className="t-sm" style={{ fontWeight: 500 }}>
            {label}
          </span>
          <span className="num t-sm" style={{ fontWeight: 600, color: s.color }}>
            {value}
          </span>
        </div>
        {hint && <div className="t-xs t-muted">{hint}</div>}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Health + Allocation Row
// ─────────────────────────────────────────────────────────────────────────────

interface AloksiProps {
  allocation: CategoryAllocation[];
  loading: boolean;
}

function AlokasiCard({ allocation, loading }: AloksiProps) {
  const netWorthTotal = allocation.reduce(
    (s, c) => s + Math.max(0, Number(c.actual_value_idr)),
    0,
  );
  const data = allocation
    .map((c) => ({ id: c.category_id, label: c.name, value: Number(c.actual_value_idr), color: categoryColor(c.name) }))
    .filter((d) => d.value > 0);

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Alokasi Aset</div>
          <div className="card-sub">berdasarkan nilai pasar</div>
        </div>
      </div>
      <div className="card-pad flex items-center gap-5" style={{ paddingTop: 14 }}>
        {loading ? (
          <div
            className="skeleton"
            style={{ width: 180, height: 180, borderRadius: "50%", flexShrink: 0 }}
          />
        ) : (
          <DonutChart
            data={data}
            centerLabel="Total"
            centerValue={formatIDR(String(netWorthTotal))}
          />
        )}
        <div className="flex col gap-2 flex-1">
          {allocation
            .filter((a) => Number(a.actual_value_idr) > 0)
            .map((a) => (
              <div key={a.category_id} className="flex items-center gap-2">
                <span
                  className="dot"
                  style={{ background: categoryColor(a.name) }}
                />
                {loading ? (
                  <div className="skeleton" style={{ width: 120, height: 12 }} />
                ) : (
                  <>
                    <span className="t-sm flex-1 truncate">{a.name}</span>
                    <span
                      className="t-xs num t-muted"
                      title={formatIDR(a.actual_value_idr)}
                    >
                      {formatIDRShort(a.actual_value_idr)}
                    </span>
                    <span className="t-sm num" style={{ fontWeight: 500 }}>
                      {Number(a.actual_pct).toFixed(1).replace(".", ",")}%
                    </span>
                  </>
                )}
              </div>
            ))}
        </div>
      </div>
    </div>
  );
}

interface DriftProps {
  allocation: CategoryAllocation[];
  loading: boolean;
}

function DriftCard({ allocation, loading }: DriftProps) {
  const outOfBandCount = allocation.filter((c) => c.out_of_band).length;
  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Target vs Aktual</div>
          <div className="card-sub">drift dari alokasi target</div>
        </div>
        <span
          className={cn(
            "badge",
            outOfBandCount > 0 ? "badge-warn" : "badge-gain",
          )}
        >
          {outOfBandCount} di luar batas
        </span>
      </div>
      <div className="card-pad flex col gap-3" style={{ paddingTop: 14 }}>
        {loading ? (
          [0, 1, 2].map((i) => (
            <div key={i} className="skeleton" style={{ width: "100%", height: 40 }} />
          ))
        ) : (
          <DriftBarsChart allocation={allocation} />
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Rebalancing Section — matches source's rebalancing card
// ─────────────────────────────────────────────────────────────────────────────

function RebalancingCard({ allocation }: { allocation: CategoryAllocation[] }) {
  const outOfBand = allocation.filter((c) => c.out_of_band);

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Rekomendasi Rebalancing</div>
          <div className="card-sub">langkah agar kembali ke target</div>
        </div>
      </div>
      <div className="flex col" style={{ padding: "6px 20px 18px" }}>
        {outOfBand.length === 0 ? (
          <div className="flex items-center gap-3" style={{ padding: "18px 0" }}>
            <span
              style={{
                width: 36,
                height: 36,
                borderRadius: 10,
                background: "hsl(var(--gain-soft))",
                color: "hsl(var(--gain))",
                display: "grid",
                placeItems: "center",
              }}
            >
              <CheckCircle2 size={18} />
            </span>
            <div>
              <div className="t-sm" style={{ fontWeight: 600 }}>
                Portofolio seimbang
              </div>
              <div className="t-xs t-muted">Semua kategori dalam batas toleransi.</div>
            </div>
          </div>
        ) : (
          outOfBand.map((c) => {
            const reb = parseNum(c.rebalance_idr);
            const isBuy = reb > 0;
            return (
              <div
                key={c.category_id}
                className="flex items-center gap-3"
                style={{
                  padding: "13px 0",
                  borderBottom: "1px solid hsl(var(--border))",
                }}
              >
                <span
                  style={{
                    width: 36,
                    height: 36,
                    borderRadius: 10,
                    flexShrink: 0,
                    display: "grid",
                    placeItems: "center",
                    background: isBuy
                      ? "hsl(var(--gain-soft))"
                      : "hsl(var(--warn-soft))",
                    color: isBuy
                      ? "hsl(var(--gain))"
                      : "hsl(var(--warn))",
                  }}
                >
                  {isBuy ? "▲" : "▼"}
                </span>
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <div className="t-sm" style={{ fontWeight: 600 }}>
                    {isBuy ? "Beli" : "Pangkas"} {c.name}
                  </div>
                  <div className="t-xs t-muted">
                    {Number(c.actual_pct).toFixed(1).replace(".", ",")}% → target{" "}
                    {Number(c.target_pct).toFixed(0)}% · selisih{" "}
                    {(Number(c.actual_pct) - Number(c.target_pct))
                      .toFixed(1)
                      .replace(".", ",")}
                    %
                  </div>
                </div>
                <span
                  className="num t-sm"
                  style={{
                    fontWeight: 600,
                    color: isBuy ? "hsl(var(--gain))" : "hsl(var(--warn))",
                  }}
                >
                  {isBuy ? "+" : "−"}
                  {formatIDR(String(Math.abs(reb)))}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Kesehatan Portofolio — matches source's health card using MetricRow
// ─────────────────────────────────────────────────────────────────────────────

interface HealthCardProps {
  runwayMonths: string;
  liquidIdr: string;
  concentration: { symbol: string; pct: string } | null;
  savingsRate: string;
  categoryCount: number;
  outOfBandCount: number;
}

function HealthCard({
  runwayMonths,
  liquidIdr,
  concentration,
  savingsRate,
  categoryCount,
  outOfBandCount,
}: HealthCardProps) {
  const runway = parseNum(runwayMonths);
  const concPct = concentration ? parseNum(concentration.pct) : 0;
  const savingsN = parseNum(savingsRate);

  const runwayStatus: HealthStatus =
    runway >= 6 ? "good" : runway >= 3 ? "warn" : "bad";
  const concStatus: HealthStatus =
    concPct < 20 ? "good" : concPct < 30 ? "warn" : "bad";
  const saveStatus: HealthStatus =
    savingsN >= 30 ? "good" : savingsN >= 15 ? "warn" : "bad";
  const driftStatus: HealthStatus =
    outOfBandCount === 0 ? "good" : outOfBandCount <= 2 ? "warn" : "bad";

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Kesehatan Portofolio</div>
          <div className="card-sub">indikator risiko &amp; ketahanan</div>
        </div>
      </div>
      <div className="card-pad" style={{ paddingTop: 6 }}>
        <MetricRow
          icon={<Shield size={17} />}
          label="Dana darurat"
          status={runwayStatus}
          value={`${runway.toFixed(1).replace(".", ",")} bln`}
          hint={`Aset likuid ${formatIDR(liquidIdr)} · menutup ${runway.toFixed(0)}× pengeluaran`}
        />
        <MetricRow
          icon={<PieIcon size={17} />}
          label="Konsentrasi terbesar"
          status={concStatus}
          value={concentration ? formatPct(concentration.pct) : "—"}
          hint={concentration ? `${concentration.symbol} — posisi tunggal terbesar` : undefined}
        />
        <MetricRow
          icon={<Scale size={17} />}
          label="Tingkat menabung"
          status={saveStatus}
          value={`${savingsN.toFixed(0)}%`}
          hint="dari pemasukan bulan ini"
        />
        {/* Diversification row — no border-bottom (last item) */}
        <div
          className="flex items-center gap-3"
          style={{ padding: "12px 0" }}
        >
          <span
            style={{
              width: 36,
              height: 36,
              borderRadius: 10,
              flexShrink: 0,
              display: "grid",
              placeItems: "center",
              color: STATUS_STYLE[driftStatus].color,
              background: STATUS_STYLE[driftStatus].soft,
            }}
          >
            <Target size={17} />
          </span>
          <div className="flex-1">
            <div className="flex items-center justify-between gap-2">
              <span className="t-sm" style={{ fontWeight: 500 }}>
                Diversifikasi
              </span>
              <span
                className="num t-sm"
                style={{ fontWeight: 600, color: STATUS_STYLE[driftStatus].color }}
              >
                {categoryCount} kelas
              </span>
            </div>
            <div className="t-xs t-muted">
              {outOfBandCount} kategori perlu penyesuaian
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Komposisi Kekayaan — stacked area with range seg
// ─────────────────────────────────────────────────────────────────────────────

interface CompositionProps {
  composition: Array<{
    as_of: string;
    parts: Array<{ category: string; value_idr: string }>;
  }>;
  loading: boolean;
}

function KomposisiCard({ composition, loading }: CompositionProps) {
  const [range, setRange] = useState<"3m" | "6m" | "1y">("1y");

  const slicedData =
    range === "3m"
      ? composition.slice(-3)
      : range === "6m"
        ? composition.slice(-6)
        : composition;

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Komposisi Kekayaan</div>
          <div className="card-sub">nilai pasar per kelas aset (IDR)</div>
        </div>
        <div className="seg">
          {(["3m", "6m", "1y"] as const).map((v) => (
            <button
              key={v}
              type="button"
              className={range === v ? "active" : ""}
              onClick={() => setRange(v)}
            >
              {v === "3m" ? "3B" : v === "6m" ? "6B" : "1Th"}
            </button>
          ))}
        </div>
      </div>
      <div className="card-pad" style={{ paddingTop: 10 }}>
        {loading ? (
          <div className="skeleton" style={{ width: "100%", height: 260 }} />
        ) : (
          <StackedAreaChart composition={slicedData} />
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Tujuan Keuangan — goals with icon squares + progress
// ─────────────────────────────────────────────────────────────────────────────

function GoalRow({ goal }: { goal: Goal }) {
  const current = parseNum(goal.current_idr);
  const target = parseNum(goal.target_idr);
  const pct = target > 0 ? Math.min(100, (current / target) * 100) : 0;
  const color = categoryColor(goal.label);

  return (
    <div className="flex col gap-2">
      <div className="flex items-center gap-3">
        <span
          style={{
            width: 34,
            height: 34,
            borderRadius: 9,
            flexShrink: 0,
            display: "grid",
            placeItems: "center",
            background: color + "22",
            color: color,
          }}
        >
          <Target size={17} />
        </span>
        <div className="flex-1" style={{ minWidth: 0 }}>
          <div className="t-sm" style={{ fontWeight: 600 }}>
            {goal.label}
          </div>
          <div className="t-xs t-muted">{goal.note ?? ""}</div>
        </div>
        <span className="num t-sm" style={{ fontWeight: 600 }}>
          {pct.toFixed(0)}%
        </span>
      </div>
      {/* Progress bar */}
      <div className="progress">
        <span
          style={{
            width: `${pct}%`,
            background: color,
          }}
        />
      </div>
      <div className="flex items-center justify-between t-xs t-muted num">
        <span>{formatIDR(goal.current_idr)}</span>
        <span>target {formatIDR(goal.target_idr)}</span>
      </div>
    </div>
  );
}

function GoalsCard({ goals }: { goals: Goal[] }) {
  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Tujuan Keuangan</div>
          <div className="card-sub">progres menuju target</div>
        </div>
      </div>
      <div className="card-pad flex col gap-5" style={{ paddingTop: 16 }}>
        {goals.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">
              <Target size={26} />
            </div>
            <div>
              <div className="t-h3">Belum ada tujuan keuangan</div>
              <div className="t-sm t-muted" style={{ marginTop: 4, maxWidth: 340 }}>
                Tetapkan target keuangan untuk memantau progres.
              </div>
            </div>
          </div>
        ) : (
          goals.map((g) => <GoalRow key={g.id} goal={g} />)
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Pergerakan Hari Ini — top movers by absolute IDR impact
// ─────────────────────────────────────────────────────────────────────────────

function MoversCard({ movers, loading }: { movers: Mover[]; loading: boolean }) {
  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Pergerakan Hari Ini</div>
          <div className="card-sub">kontributor terbesar</div>
        </div>
      </div>
      <div className="card-pad flex col" style={{ paddingTop: 6 }}>
        {loading ? (
          [0, 1, 2].map((i) => (
            <div key={i} className="skeleton" style={{ width: "100%", height: 36, marginTop: 8 }} />
          ))
        ) : movers.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">
              <TrendingUp size={26} />
            </div>
            <div>
              <div className="t-h3">Belum cukup data harga</div>
              <div className="t-sm t-muted" style={{ marginTop: 4, maxWidth: 300 }}>
                Pergerakan harian muncul setelah ada dua hari data harga per instrumen.
              </div>
            </div>
          </div>
        ) : (
          movers.map((m) => {
            const up = parseNum(m.delta_idr) >= 0;
            return (
              <div
                key={m.instrument_id}
                className="flex items-center gap-3"
                style={{ padding: "10px 0", borderBottom: "1px solid hsl(var(--border))" }}
              >
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <div className="t-sm" style={{ fontWeight: 600 }}>{m.symbol}</div>
                  <div className="t-xs t-muted truncate">{m.name}</div>
                </div>
                <span
                  className={cn("num t-xs", up ? "gain" : "loss")}
                  style={{ fontWeight: 500 }}
                >
                  {up ? "▲" : "▼"} {Math.abs(m.delta_pct).toFixed(2).replace(".", ",")}%
                </span>
                <span
                  className={cn("num t-sm", up ? "gain" : "loss")}
                  style={{ fontWeight: 600, minWidth: 110, textAlign: "right" }}
                >
                  {up ? "+" : "−"}
                  {formatIDR(String(Math.abs(parseNum(m.delta_idr))))}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Pending review banner — closes the Telegram/web ingest loop
// ─────────────────────────────────────────────────────────────────────────────

function PendingReviewBanner({ count }: { count: number }) {
  if (count === 0) return null;
  return (
    <Link
      to="/data"
      className="card card-pad flex items-center gap-3"
      style={{ textDecoration: "none", color: "inherit", borderLeft: "3px solid hsl(var(--warn))" }}
    >
      <span style={{ color: "hsl(var(--warn))", display: "grid", placeItems: "center" }}>
        <AlertTriangle size={18} />
      </span>
      <span className="t-sm" style={{ fontWeight: 500 }}>
        {count} transaksi menunggu konfirmasi
      </span>
      <span className="t-xs t-muted" style={{ marginLeft: "auto" }}>
        Buka Data →
      </span>
    </Link>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Top Holdings — largest positions by market value
// ─────────────────────────────────────────────────────────────────────────────

function TopHoldingsCard({
  positions,
  instruments,
  netWorthIdr,
}: {
  positions: Position[];
  instruments: Instrument[];
  netWorthIdr: string;
}) {
  const labelOf = (id: number) => instruments.find((i) => i.id === id);
  const netWorth = parseNum(netWorthIdr);
  const top = [...positions]
    .filter((p) => parseNum(p.quantity) !== 0)
    .sort((a, b) => parseNum(b.market_value_idr) - parseNum(a.market_value_idr))
    .slice(0, 5);

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Posisi Terbesar</div>
          <div className="card-sub">5 holding teratas berdasarkan nilai</div>
        </div>
      </div>
      <div className="card-pad flex col" style={{ paddingTop: 6 }}>
        {top.length === 0 ? (
          <div className="t-sm t-muted" style={{ padding: "14px 0" }}>
            Belum ada posisi.
          </div>
        ) : (
          top.map((p) => {
            const ins = labelOf(p.instrument_id);
            const value = parseNum(p.market_value_idr);
            const weight = netWorth > 0 ? (value / netWorth) * 100 : 0;
            return (
              <div
                key={p.instrument_id}
                className="flex items-center gap-3"
                style={{ padding: "10px 0", borderBottom: "1px solid hsl(var(--border))" }}
              >
                <div className="flex-1" style={{ minWidth: 0 }}>
                  <div className="t-sm" style={{ fontWeight: 600 }}>
                    {ins?.symbol ?? `#${p.instrument_id}`}
                  </div>
                  <div className="t-xs t-muted truncate">
                    {formatQty(p.quantity)} unit
                  </div>
                </div>
                <span className="num t-xs t-muted">{weight.toFixed(1).replace(".", ",")}%</span>
                <span className="num t-sm" style={{ fontWeight: 600, minWidth: 110, textAlign: "right" }}>
                  {formatIDR(p.market_value_idr)}
                </span>
              </div>
            );
          })
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Aktivitas Terakhir — most recent transactions
// ─────────────────────────────────────────────────────────────────────────────

function ActivityCard({
  transactions,
  instruments,
}: {
  transactions: Transaction[];
  instruments: Instrument[];
}) {
  const symbolOf = (id: number) =>
    instruments.find((i) => i.id === id)?.symbol ?? `#${id}`;
  const recent = [...transactions]
    .sort((a, b) => b.executed_at.localeCompare(a.executed_at))
    .slice(0, 5);

  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">Aktivitas Terakhir</div>
          <div className="card-sub">5 transaksi terbaru</div>
        </div>
      </div>
      <div className="card-pad flex col" style={{ paddingTop: 6 }}>
        {recent.length === 0 ? (
          <div className="empty">
            <div className="empty-icon">
              <Activity size={26} />
            </div>
            <div className="t-sm t-muted">Belum ada transaksi.</div>
          </div>
        ) : (
          recent.map((t) => (
            <div
              key={t.id}
              className="flex items-center gap-3"
              style={{ padding: "10px 0", borderBottom: "1px solid hsl(var(--border))" }}
            >
              <span className={cn("badge", txTone(t.txn_type))}>{txLabel(t.txn_type)}</span>
              <div className="flex-1" style={{ minWidth: 0 }}>
                <div className="t-sm" style={{ fontWeight: 600 }}>{symbolOf(t.instrument_id)}</div>
                <div className="t-xs t-muted">{t.executed_at.slice(0, 10)}</div>
              </div>
              <span className="num t-sm" style={{ fontWeight: 500 }}>
                {formatQty(t.quantity)}
              </span>
            </div>
          ))
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Benchmark — portfolio TWR vs index returns (1 year)
// ─────────────────────────────────────────────────────────────────────────────

function BenchmarkCard({ rows }: { rows: BenchmarkRow[] }) {
  // Nothing useful to show while loading or when every fetch failed.
  if (rows.length === 0) return null;
  return (
    <div className="card">
      <div className="card-head">
        <div>
          <div className="card-title">vs Benchmark</div>
          <div className="card-sub">return 1 tahun terakhir</div>
        </div>
        <Trophy size={16} style={{ color: "hsl(var(--muted-foreground))" }} />
      </div>
      <div
        className="card-pad grid gap-4"
        style={{ paddingTop: 10, gridTemplateColumns: `repeat(${rows.length}, 1fr)` }}
      >
        {rows.map((r) => {
            const pct = r.return_ratio * 100;
            const up = pct >= 0;
            return (
              <div key={r.label} className="flex col gap-1">
                <span className="t-xs t-muted">{r.label}</span>
                <span className={cn("num t-h3", up ? "gain" : "loss")}>
                  {up ? "+" : ""}
                  {pct.toFixed(1).replace(".", ",")}%
                </span>
              </div>
            );
        })}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Shared card skeleton
// ─────────────────────────────────────────────────────────────────────────────

function CardSkeleton({ rows = 3, height = 8 }: { rows?: number; height?: number }) {
  return (
    <div className="card">
      <div className="card-head">
        <div className="skeleton" style={{ width: 160, height: 16 }} />
      </div>
      <div className="card-pad flex col gap-3">
        {Array.from({ length: rows }).map((_, i) => (
          <div key={i} className="skeleton" style={{ width: "100%", height }} />
        ))}
      </div>
    </div>
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
  const movers = useMovers();
  const benchmark = useBenchmark();
  const pendingReviews = useReviewItems("pending");
  const transactions = useTransactions();
  const instruments = useInstruments();

  const isLoadingCore = summary.isLoading || insights.isLoading;

  const outOfBandCount = summary.data?.allocation.filter((c) => c.out_of_band).length ?? 0;
  const activeCategories =
    summary.data?.allocation.filter(
      (c) => Number(c.actual_value_idr) > 0 && c.category_id !== UNCATEGORIZED_CATEGORY_ID,
    ).length ?? 0;
  const staleCount =
    summary.data?.positions.filter((p) => p.price_stale && parseNum(p.quantity) !== 0).length ?? 0;

  return (
    <div className="flex col gap-5">
      {/* ── Topbar actions ──────────────────────────────────────────────────── */}
      <div className="flex items-center justify-between">
        <div>
          <div className="t-h2">Dashboard</div>
          <div className="t-xs t-muted">Command center keuangan Anda</div>
        </div>
        <button
          type="button"
          className="btn btn-outline btn-sm"
          onClick={() => refresh.mutate()}
          disabled={refresh.isPending}
        >
          {refresh.isPending ? (
            <>
              <Loader2 size={13} className="animate-spin" />
              Memperbarui…
            </>
          ) : (
            <>
              <RefreshCw size={13} />
              Refresh harga
            </>
          )}
        </button>
      </div>

      {/* ── 0. Pending review banner ──────────────────────────────────────── */}
      <PendingReviewBanner count={pendingReviews.data?.length ?? 0} />

      {/* ── Hari ini (assistant section) ──────────────────────────────────── */}
      <div>
        <div className="t-h3" style={{ marginBottom: 12 }}>Hari ini</div>
        <div className="grid gap-5 lay-2">
          <DashboardTodoCard />
          <DashboardAgendaCard />
          <DashboardReminderCard />
          <DashboardInboxCard />
        </div>
      </div>

      {/* ── Keuangan ──────────────────────────────────────────────────────── */}
      <div className="t-h3" style={{ marginTop: 4 }}>Keuangan</div>

      {/* ── 1. Hero + KPI row ──────────────────────────────────────────────── */}
      {isLoadingCore ? (
        <HeroRowSkeleton />
      ) : summary.error || insights.error ? (
        <div className="card card-pad">
          <div className="t-sm loss">
            Gagal memuat data:{" "}
            {summary.error instanceof Error
              ? summary.error.message
              : insights.error instanceof Error
                ? insights.error.message
                : "unknown"}
          </div>
        </div>
      ) : summary.data && insights.data ? (
        <HeroSection
          netWorthIdr={summary.data.net_worth_idr}
          netWorthUsd={summary.data.net_worth_usd}
          dayDeltaIdr={insights.data.day_delta_idr}
          dayDeltaPct={insights.data.day_delta_pct}
          unrealizedPnl={summary.data.total_unrealized_pnl_idr}
          unrealizedPricePnl={summary.data.total_unrealized_price_pnl_idr}
          unrealizedFxPnl={summary.data.total_unrealized_fx_pnl_idr}
          fxIncomplete={summary.data.fx_incomplete}
          xirr={summary.data.xirr}
          yieldPct={insights.data.yield_pct}
          dividendTtmIdr={insights.data.dividend_ttm_idr}
          savingsRate={insights.data.savings_rate}
          snapshots={history.data ?? []}
          staleCount={staleCount}
        />
      ) : null}

      {/* ── 2. Alokasi + Tujuan Keuangan side by side (50/50) ─────────────── */}
      <div className="grid gap-5 lay-2">
        {summary.isLoading ? (
          <CardSkeleton rows={3} height={12} />
        ) : summary.data ? (
          <AlokasiCard allocation={summary.data.allocation} loading={false} />
        ) : null}

        {goals.isLoading ? (
          <CardSkeleton rows={2} height={60} />
        ) : (
          <GoalsCard goals={goals.data ?? []} />
        )}
      </div>

      {/* ── 2b. Target vs Aktual (full width) ─────────────────────────────── */}
      <div className="grid gap-5">
        {summary.isLoading ? (
          <CardSkeleton rows={4} height={30} />
        ) : summary.data ? (
          <DriftCard allocation={summary.data.allocation} loading={false} />
        ) : null}
      </div>

      {/* ── 3. Rebalancing + Kesehatan side by side ───────────────────────── */}
      <div className="grid gap-5 lay-2">
        {summary.isLoading ? (
          <CardSkeleton rows={3} height={44} />
        ) : summary.data ? (
          <RebalancingCard allocation={summary.data.allocation} />
        ) : null}

        {isLoadingCore ? (
          <CardSkeleton rows={4} height={36} />
        ) : insights.data ? (
          <HealthCard
            runwayMonths={insights.data.runway_months}
            liquidIdr={insights.data.liquid_idr ?? "0"}
            concentration={insights.data.concentration}
            savingsRate={insights.data.savings_rate}
            categoryCount={activeCategories}
            outOfBandCount={outOfBandCount}
          />
        ) : null}
      </div>

      {/* ── 4. Komposisi Kekayaan full width ──────────────────────────────── */}
      {insights.isLoading ? (
        <CardSkeleton rows={1} height={260} />
      ) : insights.data ? (
        <KomposisiCard
          composition={insights.data.composition}
          loading={false}
        />
      ) : null}

      {/* ── 5. Pergerakan + Tujuan side by side ───────────────────────────── */}
      <div className="grid gap-5 lay-2">
        <MoversCard movers={movers.data ?? []} loading={movers.isLoading} />
        <HyperliquidCard />
      </div>

      {/* ── 5b. Hyperliquid equity card ───────────────────────────────────── */}
      <HyperliquidCard />

      {/* ── 6. Posisi Terbesar + Aktivitas side by side ───────────────────── */}
      <div className="grid gap-5 lay-2">
        <TopHoldingsCard
          positions={summary.data?.positions ?? []}
          instruments={instruments.data ?? []}
          netWorthIdr={summary.data?.net_worth_idr ?? "0"}
        />
        <ActivityCard
          transactions={transactions.data ?? []}
          instruments={instruments.data ?? []}
        />
      </div>

      {/* ── 7. Benchmark ──────────────────────────────────────────────────── */}
      <BenchmarkCard rows={benchmark.data ?? []} />
    </div>
  );
}
