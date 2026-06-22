import { useState } from "react";
import { Plus, CheckCircle, AlertTriangle } from "lucide-react";
import { usePlanTree, type PlanNodeAllocation } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { siblingTargetSum } from "../lib/plan-tree";
import { PlanTreeNode } from "../components/planner/PlanTreeNode";
import { AddPlanNodeDialog } from "../components/planner/AddPlanNodeDialog";

function ProgressBar({ value, color = "hsl(var(--primary))" }: { value: number; color?: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div className="progress">
      <span style={{ width: `${pct}%`, background: color }} />
    </div>
  );
}

export default function PlannerPage() {
  const tree = usePlanTree();
  const [addParent, setAddParent] = useState<PlanNodeAllocation | null>(null);
  const [addOpen, setAddOpen] = useState(false);

  const roots = tree.data ?? [];
  const totalTarget = siblingTargetSum(roots);
  const sumOk = Math.abs(totalTarget - 100) <= 0.01;

  const openAddRoot = () => { setAddParent(null); setAddOpen(true); };
  const openAddChild = (parent: PlanNodeAllocation) => { setAddParent(parent); setAddOpen(true); };

  return (
    <div>
      {/* Page header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 12 }}>
        <div>
          <h1 className="t-h1">Planner</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Struktur alokasi bertingkat &amp; batas toleransi</div>
        </div>
        <button type="button" className="btn btn-primary btn-sm" onClick={openAddRoot} aria-label="Tambah kelas aset">
          <Plus size={15} />
        </button>
      </div>

      {/* Top-level target-sum indicator */}
      <div className="card card-pad flex items-center" style={{ marginBottom: 18, gap: 16, flexWrap: "wrap" }}>
        <div className="flex items-center" style={{ gap: 12, flex: 1 }}>
          <span
            className="flex items-center justify-center"
            style={{
              width: 40, height: 40, borderRadius: 11, flexShrink: 0,
              background: sumOk ? "hsl(var(--gain-soft))" : "hsl(var(--warn-soft))",
              color: sumOk ? "hsl(var(--gain))" : "hsl(var(--warn))",
            }}
          >
            {sumOk ? <CheckCircle size={20} /> : <AlertTriangle size={20} />}
          </span>
          <div>
            <div className="t-h3">Total target kelas aset {totalTarget.toFixed(1)}%</div>
            <div className="t-sm t-muted">
              {sumOk
                ? "Seimbang — target berjumlah tepat 100%."
                : `Perlu disesuaikan ${100 - totalTarget > 0 ? "+" : ""}${(100 - totalTarget).toFixed(1)}% agar mencapai 100%.`}
            </div>
          </div>
        </div>
        <div style={{ width: 200 }}>
          <ProgressBar value={Math.min(totalTarget, 100)} color={sumOk ? "hsl(var(--gain))" : "hsl(var(--warn))"} />
        </div>
      </div>

      <QueryState isLoading={tree.isLoading} error={tree.error}>
        {roots.length === 0 ? (
          <div className="card">
            <div className="empty">
              <div className="empty-icon">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="2"/>
                </svg>
              </div>
              <div>
                <div className="t-h3">Belum ada kelas aset</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Tambahkan kelas aset untuk menyusun struktur alokasi portofolio.
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="card card-pad">
            {roots.map((n) => (
              <PlanTreeNode key={n.id} node={n} depth={0} onAddChild={openAddChild} />
            ))}
          </div>
        )}
      </QueryState>

      <AddPlanNodeDialog open={addOpen} parent={addParent} onClose={() => setAddOpen(false)} />
    </div>
  );
}
