import { useState } from "react";
import { ChevronRight, ChevronDown, Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useUpdatePlanNode, useDeletePlanNode, type PlanNodeAllocation } from "../../api/hooks";
import { NumberInput } from "@/components/ui/NumberInput";
import { formatIDR, parseNum } from "../../lib/format";
import { isSyntheticNode, siblingTargetSum } from "../../lib/plan-tree";
import { categoryColor } from "../charts/AllocationDonutChart";

interface Props {
  node: PlanNodeAllocation;
  depth: number;
  onAddChild: (parent: PlanNodeAllocation) => void;
}

export function PlanTreeNode({ node, depth, onAddChild }: Props) {
  const [expanded, setExpanded] = useState(true);
  const [target, setTarget] = useState(node.target_pct);
  const update = useUpdatePlanNode();
  const del = useDeletePlanNode();

  const synthetic = isSyntheticNode(node.id);
  const hasChildren = node.children.length > 0;
  const hasRealChildren = node.children.some((c) => !isSyntheticNode(c.id));
  const childTargetSum = siblingTargetSum(node.children);
  const actual = parseNum(node.actual_pct);
  const drift = parseNum(node.drift_pct);
  const rebalance = parseNum(node.rebalance_idr);
  const color = categoryColor(node.name);

  const saveTarget = () => {
    if (target === node.target_pct) return;
    update.mutate(
      { id: node.id, patch: { target_pct: target } },
      {
        onSuccess: () => toast.success(`Target ${node.name} disimpan`),
        onError: (err) => { toast.error((err as Error).message); setTarget(node.target_pct); },
      },
    );
  };

  const remove = () => {
    del.mutate(node.id, {
      onSuccess: () => toast.success(`"${node.name}" dihapus`),
      onError: (err) => toast.error((err as Error).message),
    });
  };

  return (
    <div>
      <div
        className="flex items-center gap-2"
        style={{ padding: "10px 0", paddingLeft: depth * 22, borderBottom: "1px solid hsl(var(--border))" }}
        data-testid={`plan-node-${node.id}`}
      >
        <button
          type="button"
          className="icon-btn"
          style={{ width: 22, height: 22, visibility: hasChildren ? "visible" : "hidden" }}
          onClick={() => setExpanded((v) => !v)}
          aria-label={expanded ? `Tutup ${node.name}` : `Buka ${node.name}`}
        >
          {expanded ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <span className="dot" style={{ background: color, width: 10, height: 10, flexShrink: 0 }} />
        <span style={{ fontWeight: synthetic ? 400 : 600, color: synthetic ? "hsl(var(--muted-foreground))" : "inherit" }}>
          {node.name}
        </span>

        {!synthetic && (
          node.out_of_band ? (
            <span className="badge badge-warn">drift {drift > 0 ? "+" : ""}{drift.toFixed(1)}%</span>
          ) : (
            <span className="badge badge-gain">on target</span>
          )
        )}

        {hasRealChildren && (
          <span className="t-xs t-muted" style={{ whiteSpace: "nowrap" }}>
            anak Σ {childTargetSum.toFixed(0)}%
          </span>
        )}

        <div className="flex items-center" style={{ marginLeft: "auto", gap: 10 }}>
          <span className="t-sm num t-muted">{actual.toFixed(1)}%</span>
          {synthetic ? (
            <span className="t-sm num t-muted" style={{ width: 64, textAlign: "right" }}>tanpa target</span>
          ) : (
            <span className="t-sm num" style={{ display: "inline-flex", alignItems: "baseline", gap: 2 }}>
              <NumberInput
                className=""
                style={{ width: 56, height: 26, textAlign: "right", fontSize: "inherit", padding: "0 4px" }}
                aria-label={`Target ${node.name}`}
                value={target}
                onChange={(v) => setTarget(v)}
                onBlur={saveTarget}
                onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
              />
              %
            </span>
          )}
          {!synthetic && (
            <>
              <button
                type="button"
                className="icon-btn"
                style={{ width: 26, height: 26 }}
                onClick={() => onAddChild(node)}
                aria-label={`Tambah anak ${node.name}`}
              >
                <Plus size={13} />
              </button>
              <button
                type="button"
                className="icon-btn"
                style={{ width: 26, height: 26 }}
                onClick={remove}
                aria-label={`Hapus ${node.name}`}
              >
                <Trash2 size={13} />
              </button>
            </>
          )}
        </div>
      </div>

      {!synthetic && node.out_of_band && rebalance !== 0 && (
        <div className="t-xs warn num" style={{ paddingLeft: depth * 22 + 32, padding: "0 0 6px", fontWeight: 500 }}>
          {rebalance > 0 ? "Beli " : "Pangkas "}{formatIDR(Math.abs(rebalance))}
        </div>
      )}

      {expanded && hasChildren && node.children.map((child) => (
        <PlanTreeNode key={child.id} node={child} depth={depth + 1} onAddChild={onAddChild} />
      ))}
    </div>
  );
}
