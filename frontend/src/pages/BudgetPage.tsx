import { useState, useEffect } from "react";
import ReactDOM from "react-dom";
import { Plus, ArrowDown, ArrowUp, Scale, Check, X } from "lucide-react";
import { toast } from "sonner";
import {
  useCashflow,
  useCashflowCategories,
  useMonthSummary,
  useCreateCashflow,
  useDeleteCashflow,
  useCreateCashflowCategory,
} from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { formatIDR, parseNum } from "../lib/format";

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
      <div className="dialog" role="dialog" aria-modal="true" aria-labelledby="budget-dialog-title">
        <div className="dialog-head">
          <div>
            <div className="t-h2" id="budget-dialog-title">{title}</div>
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

export default function BudgetPage() {
  const defaultMonth = new Date().toISOString().slice(0, 7);
  const [month, setMonth] = useState(defaultMonth);
  const [entryOpen, setEntryOpen] = useState(false);
  const [catOpen, setCatOpen] = useState(false);

  const summary = useMonthSummary(month);
  const cashflow = useCashflow();
  const categories = useCashflowCategories();
  const createCashflow = useCreateCashflow();
  const deleteCashflow = useDeleteCashflow();
  const createCategory = useCreateCashflowCategory();

  const [entryForm, setEntryForm] = useState({
    occurred_on: new Date().toISOString().slice(0, 10),
    direction: "out",
    amount: "",
    currency: "IDR",
    category_id: "",
    note: "",
  });

  const [catForm, setCatForm] = useState({
    name: "",
    kind: "expense",
    monthly_budget: "",
  });

  const setEntry = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setEntryForm({ ...entryForm, [k]: e.target.value });

  const setCat = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setCatForm({ ...catForm, [k]: e.target.value });

  const submitEntry = (e: React.FormEvent) => {
    e.preventDefault();
    createCashflow.mutate(
      {
        occurred_on: entryForm.occurred_on,
        direction: entryForm.direction,
        amount: entryForm.amount,
        currency: entryForm.currency,
        category_id: entryForm.category_id ? Number(entryForm.category_id) : null,
        note: entryForm.note || null,
        account_id: null,
      },
      {
        onSuccess: () => {
          toast.success("Transaksi kas ditambahkan");
          setEntryForm({
            occurred_on: new Date().toISOString().slice(0, 10),
            direction: "out",
            amount: "",
            currency: "IDR",
            category_id: "",
            note: "",
          });
          setEntryOpen(false);
        },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  const submitCat = (e: React.FormEvent) => {
    e.preventDefault();
    createCategory.mutate(
      {
        name: catForm.name,
        kind: catForm.kind,
        monthly_budget: catForm.monthly_budget || null,
        color: null,
      },
      {
        onSuccess: () => {
          toast.success("Kategori ditambahkan");
          setCatForm({ name: "", kind: "expense", monthly_budget: "" });
          setCatOpen(false);
        },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  const summaryData = summary.data;
  const catsList = categories.data ?? [];
  const cashflowList = cashflow.data ?? [];

  return (
    <div>
      {/* Page header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">Budget</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Arus kas &amp; anggaran bulanan</div>
        </div>
        <div className="flex items-center" style={{ gap: 8 }}>
          <input
            type="month"
            className="input"
            style={{ width: 160 }}
            value={month}
            onChange={(e) => setMonth(e.target.value)}
            aria-label="Bulan"
          />
          <button
            type="button"
            className="btn btn-outline btn-sm"
            onClick={() => setCatOpen(true)}
            aria-label="Tambah kategori anggaran"
          >
            <Plus size={15} />
            Kategori
          </button>
          <button
            type="button"
            className="btn btn-primary btn-sm"
            onClick={() => setEntryOpen(true)}
            aria-label="Catat arus kas"
          >
            <Plus size={15} />
            Catat
          </button>
        </div>
      </div>

      {/* KPI stat cards */}
      <QueryState isLoading={summary.isLoading} error={summary.error}>
        <div className="lay-3" style={{ gap: 16, marginBottom: 18 }}>
          {/* Income */}
          <div className="card stat-card">
            <div className="stat-label">
              <ArrowDown size={15} />
              Income
            </div>
            <div className="stat-value num gain">{formatIDR(summaryData?.total_in ?? "0")}</div>
          </div>
          {/* Expense */}
          <div className="card stat-card">
            <div className="stat-label">
              <ArrowUp size={15} />
              Expense
            </div>
            <div className="stat-value num loss">{formatIDR(summaryData?.total_out ?? "0")}</div>
          </div>
          {/* Net */}
          <div className="card stat-card">
            <div className="stat-label">
              <Scale size={15} />
              Net
            </div>
            <div className={"stat-value num " + (parseNum(summaryData?.net ?? "0") >= 0 ? "gain" : "loss")}>
              {formatIDR(summaryData?.net ?? "0")}
            </div>
          </div>
        </div>

        {/* Two-column layout: category budgets + cashflow list */}
        <div className="lay-2-15col" style={{ gap: 16 }}>
          {/* Per-category progress */}
          <div className="card">
            <div className="card-head">
              <div>
                <div className="card-title">Anggaran per Kategori</div>
                <div className="card-sub">terpakai vs anggaran bulan ini</div>
              </div>
            </div>
            <div className="card-pad flex-col" style={{ paddingTop: 16, display: "flex", flexDirection: "column", gap: 16 }}>
              {(summaryData?.categories ?? []).length === 0 ? (
                <div className="t-sm t-muted">No category data for this month.</div>
              ) : (
                (summaryData?.categories ?? []).map((cat) => {
                  const actualNum = parseNum(cat.actual);
                  const budgetNum = cat.budget != null ? parseNum(cat.budget) : null;
                  const ratio = budgetNum != null && budgetNum > 0 ? Math.min((actualNum / budgetNum) * 100, 100) : 0;
                  const over = cat.over_budget;
                  const near = !over && ratio > 85;
                  const color = over
                    ? "hsl(var(--loss))"
                    : near
                    ? "hsl(var(--warn))"
                    : "hsl(var(--primary))";
                  return (
                    <div key={cat.category_id ?? "uncategorized"} style={{ display: "flex", flexDirection: "column", gap: 8 }}>
                      <div className="flex items-center justify-between">
                        <span className="t-sm" style={{ fontWeight: 500 }}>{cat.name}</span>
                        <span className="t-sm num">
                          <span style={{ color, fontWeight: 500 }}>{formatIDR(cat.actual)}</span>
                          {cat.budget != null && (
                            <span className="t-muted"> / {formatIDR(cat.budget)}</span>
                          )}
                        </span>
                      </div>
                      {budgetNum != null && <ProgressBar value={ratio} color={color} />}
                      {over && (
                        <span className="t-xs loss num">
                          Lebih {formatIDR(String(actualNum - (budgetNum ?? 0)))} dari anggaran
                        </span>
                      )}
                    </div>
                  );
                })
              )}
            </div>
          </div>

          {/* Recent cashflow list */}
          <div className="card">
            <div className="card-head">
              <div>
                <div className="card-title">Arus Kas Terbaru</div>
              </div>
            </div>
            <div style={{ padding: "8px 0" }}>
              {cashflowList.length === 0 ? (
                <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>
                  No cashflow entries yet.
                </div>
              ) : (
                cashflowList.map((f) => (
                  <div
                    key={f.id}
                    className="flex items-center"
                    style={{ padding: "11px 20px", gap: 12 }}
                  >
                    <span
                      style={{
                        width: 34,
                        height: 34,
                        borderRadius: 9,
                        display: "grid",
                        placeItems: "center",
                        flexShrink: 0,
                        background: f.direction === "in" ? "hsl(var(--gain-soft))" : "hsl(var(--muted))",
                        color: f.direction === "in" ? "hsl(var(--gain))" : "hsl(var(--muted-foreground))",
                      }}
                    >
                      {f.direction === "in" ? <ArrowDown size={16} strokeWidth={2.4} /> : <ArrowUp size={16} strokeWidth={2.4} />}
                    </span>
                    <div className="flex-1" style={{ minWidth: 0 }}>
                      <div className="t-sm truncate" style={{ fontWeight: 500 }}>{f.note ?? f.currency}</div>
                      <div className="t-xs t-muted truncate">{f.occurred_on}</div>
                    </div>
                    <div className="flex items-center" style={{ gap: 8 }}>
                      <span
                        className={"num t-sm " + (f.direction === "in" ? "gain" : "")}
                        style={{ fontWeight: 500 }}
                      >
                        {f.direction === "in" ? "+" : "−"}{formatIDR(f.amount)}
                      </span>
                      <button
                        type="button"
                        className="icon-btn"
                        style={{ width: 26, height: 26 }}
                        onClick={() =>
                          deleteCashflow.mutate(f.id, {
                            onSuccess: () => toast.success("Entri dihapus"),
                            onError: (err) => toast.error((err as Error).message),
                          })
                        }
                        aria-label={`Hapus entri ${f.id}`}
                      >
                        <X size={13} />
                      </button>
                    </div>
                  </div>
                ))
              )}
            </div>
          </div>
        </div>
      </QueryState>

      {/* Add Cashflow Entry Dialog */}
      <Dialog
        open={entryOpen}
        onClose={() => setEntryOpen(false)}
        title="Catat Arus Kas"
        sub="Tambahkan pemasukan atau pengeluaran"
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setEntryOpen(false)}>
              Batal
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={submitEntry as unknown as React.MouseEventHandler}
              disabled={createCashflow.isPending}
              aria-label="Simpan arus kas"
            >
              <Check size={16} />
              Simpan
            </button>
          </>
        }
      >
        <form id="entry-form" onSubmit={submitEntry}>
          {/* Direction segmented control */}
          <div className="seg" style={{ alignSelf: "flex-start" }}>
            <button
              type="button"
              className={entryForm.direction === "out" ? "active" : ""}
              onClick={() => setEntryForm({ ...entryForm, direction: "out" })}
            >
              Pengeluaran
            </button>
            <button
              type="button"
              className={entryForm.direction === "in" ? "active" : ""}
              onClick={() => setEntryForm({ ...entryForm, direction: "in" })}
            >
              Pemasukan
            </button>
          </div>
          <label className="field">
            <span className="field-label">Keterangan</span>
            <input
              className="input"
              placeholder="mis. Belanja bulanan"
              value={entryForm.note}
              onChange={setEntry("note")}
              aria-label="Keterangan"
            />
          </label>
          <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Nominal</span>
              <input
                type="number"
                className="input"
                placeholder="0"
                value={entryForm.amount}
                onChange={setEntry("amount")}
                aria-label="Amount"
                required
              />
            </label>
            <label className="field">
              <span className="field-label">Tanggal</span>
              <input
                type="date"
                className="input"
                value={entryForm.occurred_on}
                onChange={setEntry("occurred_on")}
                aria-label="Tanggal"
              />
            </label>
          </div>
          {entryForm.direction === "out" && (
            <label className="field">
              <span className="field-label">Kategori</span>
              <select
                className="select"
                value={entryForm.category_id}
                onChange={setEntry("category_id")}
                aria-label="Direction"
              >
                <option value="">— no category —</option>
                {catsList.map((c) => (
                  <option key={c.id} value={c.id}>{c.name}</option>
                ))}
              </select>
            </label>
          )}
        </form>
      </Dialog>

      {/* Add Budget Category Dialog */}
      <Dialog
        open={catOpen}
        onClose={() => setCatOpen(false)}
        title="Tambah Kategori Anggaran"
        sub="Buat kategori dengan budget bulanan"
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setCatOpen(false)}>
              Batal
            </button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={submitCat as unknown as React.MouseEventHandler}
              disabled={createCategory.isPending}
              aria-label="Simpan kategori anggaran"
            >
              <Check size={16} />
              Simpan
            </button>
          </>
        }
      >
        <form id="cat-form" onSubmit={submitCat}>
          <label className="field">
            <span className="field-label">Nama Kategori</span>
            <input
              className="input"
              placeholder="mis. Makan, Transport…"
              value={catForm.name}
              onChange={setCat("name")}
              aria-label="Category name"
              required
            />
          </label>
          <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Jenis</span>
              <select
                className="select"
                value={catForm.kind}
                onChange={setCat("kind")}
                aria-label="Jenis kategori"
              >
                <option value="income">income</option>
                <option value="expense">expense</option>
              </select>
            </label>
            <label className="field">
              <span className="field-label">Budget Bulanan</span>
              <input
                type="number"
                className="input"
                placeholder="opsional"
                value={catForm.monthly_budget}
                onChange={setCat("monthly_budget")}
                aria-label="Monthly budget"
              />
            </label>
          </div>
        </form>
      </Dialog>
    </div>
  );
}
