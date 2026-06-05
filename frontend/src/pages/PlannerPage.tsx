import { useState, useEffect } from "react";
import ReactDOM from "react-dom";
import { Plus, Check, X, CheckCircle, AlertTriangle, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useCategories, useCreateCategory, useUpdateCategory, useDeleteCategory, useSummary, type Category } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, parseNum } from "../lib/format";

/** Inline editor for a category's target percent — saves on blur/Enter. */
function TargetEditor({ category }: { category: Category }) {
  const update = useUpdateCategory();
  const [target, setTarget] = useState(category.target_pct);

  const save = () => {
    if (target === category.target_pct) return;
    update.mutate(
      { id: category.id, patch: { target_pct: target } },
      {
        onSuccess: () => toast.success(`Target ${category.name} disimpan`),
        onError: (err) => { toast.error((err as Error).message); setTarget(category.target_pct); },
      },
    );
  };

  return (
    <span className="t-h2 num t-muted" style={{ display: "inline-flex", alignItems: "baseline", gap: 2 }}>
      <input
        className="input num"
        style={{ width: 64, height: 28, textAlign: "right", fontSize: "inherit", padding: "0 4px" }}
        aria-label={`Target ${category.name}`}
        value={target}
        onChange={(e) => setTarget(e.target.value)}
        onBlur={save}
        onKeyDown={(e) => { if (e.key === "Enter") (e.target as HTMLInputElement).blur(); }}
      />
      %
    </span>
  );
}

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  sub?: string;
  children: React.ReactNode;
  footer: React.ReactNode;
}

function Dialog({ open, onClose, title, sub, children, footer }: DialogProps) {
  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [open, onClose]);

  if (!open) return null;

  return ReactDOM.createPortal(
    <div
      className="dialog-scrim"
      onMouseDown={(e) => { if (e.target === e.currentTarget) onClose(); }}
      role="presentation"
    >
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="planner-dialog-title">
        <div className="dialog-head">
          <div>
            <div className="t-h2" id="planner-dialog-title">{title}</div>
            {sub && <div className="card-sub" style={{ marginTop: 3 }}>{sub}</div>}
          </div>
          <button type="button" className="icon-btn" onClick={onClose} aria-label="Tutup dialog" style={{ width: 32, height: 32 }}>
            <X size={18} />
          </button>
        </div>
        <div className="dialog-body">{children}</div>
        <div className="dialog-foot">{footer}</div>
      </div>
    </div>,
    document.body,
  );
}

function ProgressBar({ value, color = "hsl(var(--primary))" }: { value: number; color?: string }) {
  const pct = Math.max(0, Math.min(100, value));
  return (
    <div className="progress">
      <span style={{ width: `${pct}%`, background: color }} />
    </div>
  );
}

const CAT_COLORS = [
  "hsl(var(--cat-saham))",
  "hsl(var(--cat-crypto))",
  "hsl(var(--cat-reksadana))",
  "hsl(var(--cat-kas))",
  "hsl(var(--cat-obligasi))",
  "hsl(var(--cat-emas))",
  "hsl(var(--cat-properti))",
  "hsl(var(--cat-lainnya))",
];

export default function PlannerPage() {
  const cats = useCategories();
  const summary = useSummary();
  const create = useCreateCategory();
  const del = useDeleteCategory();
  const [open, setOpen] = useState(false);
  const [form, setForm] = useState({ name: "", target_pct: "", tolerance_band_pct: "" });

  const categories = cats.data ?? [];
  const totalTarget = categories.reduce((acc, c) => acc + parseNum(c.target_pct), 0);
  const sumOk = Math.abs(totalTarget - 100) <= 0.01;

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        name: form.name,
        target_pct: form.target_pct,
        tolerance_band_pct: form.tolerance_band_pct || null,
        color: null,
      },
      {
        onSuccess: () => {
          toast.success("Kategori alokasi ditambahkan");
          setForm({ name: "", target_pct: "", tolerance_band_pct: "" });
          setOpen(false);
        },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  return (
    <div>
      {/* Page header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 12 }}>
        <div>
          <h1 className="t-h1">Planner</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Target alokasi &amp; batas toleransi</div>
        </div>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          onClick={() => setOpen(true)}
          aria-label="Tambah kategori target"
        >
          <Plus size={15} />
          Tambah Kategori
        </button>
      </div>

      {/* 100% sum indicator */}
      <div
        className="card card-pad flex items-center"
        style={{ marginBottom: 18, gap: 16, flexWrap: "wrap" }}
      >
        <div className="flex items-center" style={{ gap: 12, flex: 1 }}>
          <span
            className="flex items-center justify-center"
            style={{
              width: 40,
              height: 40,
              borderRadius: 11,
              flexShrink: 0,
              background: sumOk ? "hsl(var(--gain-soft))" : "hsl(var(--warn-soft))",
              color: sumOk ? "hsl(var(--gain))" : "hsl(var(--warn))",
            }}
          >
            {sumOk ? <CheckCircle size={20} /> : <AlertTriangle size={20} />}
          </span>
          <div>
            <div className="t-h3">Total target alokasi {totalTarget.toFixed(1)}%</div>
            <div className="t-sm t-muted">
              {sumOk
                ? "Seimbang — target berjumlah tepat 100%."
                : `Perlu disesuaikan ${100 - totalTarget > 0 ? "+" : ""}${(100 - totalTarget).toFixed(1)}% agar mencapai 100%.`}
            </div>
          </div>
        </div>
        <div style={{ width: 200 }}>
          <ProgressBar
            value={Math.min(totalTarget, 100)}
            color={sumOk ? "hsl(var(--gain))" : "hsl(var(--warn))"}
          />
        </div>
      </div>

      <QueryState isLoading={cats.isLoading} error={cats.error}>
        {categories.length === 0 ? (
          <div className="card">
            <div className="empty">
              <div className="empty-icon">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="6"/><circle cx="12" cy="12" r="2"/>
                </svg>
              </div>
              <div>
                <div className="t-h3">Belum ada kategori</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Tambahkan kategori untuk mengatur target alokasi portofolio.
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="grid" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(280px, 1fr))", gap: 16 }}>
            {categories.map((c, idx) => {
              const allocation = summary.data?.allocation.find((x) => x.category_id === c.id);
              const actual = allocation ? parseNum(allocation.actual_pct) : 0;
              const target = parseNum(c.target_pct);
              const tol = parseNum(c.tolerance_band_pct ?? "0") || 0;
              const outOfBand = allocation?.out_of_band ?? false;
              const delta = actual - target;
              const rebalanceIdr = allocation ? parseNum(allocation.rebalance_idr) : 0;
              const color = CAT_COLORS[idx % CAT_COLORS.length];

              return (
                <div key={c.id} className="card card-pad">
                  <div className="flex items-center justify-between" style={{ marginBottom: 14 }}>
                    <div className="flex items-center" style={{ gap: 8 }}>
                      <span className="dot" style={{ background: color, width: 11, height: 11 }} />
                      <span style={{ fontWeight: 600 }}>{c.name}</span>
                    </div>
                    <div className="flex items-center" style={{ gap: 6 }}>
                      {outOfBand ? (
                        <span className="badge badge-warn">
                          drift {delta > 0 ? "+" : ""}{delta.toFixed(1)}%
                        </span>
                      ) : (
                        <span className="badge badge-gain">on target</span>
                      )}
                      <button
                        type="button"
                        className="icon-btn"
                        style={{ width: 26, height: 26 }}
                        onClick={() =>
                          del.mutate(c.id, {
                            onSuccess: () => toast.success(`Kategori "${c.name}" dihapus`),
                            onError: (err) => toast.error((err as Error).message),
                          })
                        }
                        aria-label={`Hapus kategori ${c.name}`}
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  </div>
                  <div className="flex items-end justify-between" style={{ marginBottom: 10 }}>
                    <div>
                      <div className="t-xs t-muted">Aktual</div>
                      <div className="t-h2 num">{actual.toFixed(1)}%</div>
                    </div>
                    <div style={{ textAlign: "right" }}>
                      <div className="t-xs t-muted">Target ±{tol}%</div>
                      <TargetEditor category={c} />
                    </div>
                  </div>
                  <ProgressBar
                    value={(actual / Math.max(target * 1.6, actual || 1)) * 100}
                    color={color}
                  />
                  {outOfBand && rebalanceIdr !== 0 && (
                    <div className="t-xs warn num" style={{ marginTop: 8, fontWeight: 500 }}>
                      {rebalanceIdr > 0 ? "Beli " : "Pangkas "}
                      {formatIDR(Math.abs(rebalanceIdr))}
                    </div>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </QueryState>

      {/* Add Category Dialog */}
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Tambah Kategori Target"
        sub="Tetapkan alokasi target & batas toleransi"
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setOpen(false)}>
              Batal
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={submit as unknown as React.MouseEventHandler}
              disabled={create.isPending}
              aria-label="Simpan kategori"
            >
              <Check size={16} />
              Simpan
            </button>
          </>
        }
      >
        <form id="cat-form" onSubmit={submit}>
          <label className="field">
            <span className="field-label">Nama Kategori</span>
            <input
              className="input"
              placeholder="mis. Saham US, Properti…"
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              aria-label="Nama kategori"
              required
            />
          </label>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Target %</span>
              <input
                type="number"
                className="input"
                placeholder="0"
                value={form.target_pct}
                onChange={(e) => setForm({ ...form, target_pct: e.target.value })}
                aria-label="Target persen"
                required
              />
            </label>
            <label className="field">
              <span className="field-label">Toleransi ± %</span>
              <input
                type="number"
                className="input"
                placeholder="5"
                value={form.tolerance_band_pct}
                onChange={(e) => setForm({ ...form, tolerance_band_pct: e.target.value })}
                aria-label="Toleransi persen"
              />
            </label>
          </div>
          {create.error && (
            <div className="t-sm loss">{(create.error as Error).message}</div>
          )}
        </form>
      </Dialog>
    </div>
  );
}
