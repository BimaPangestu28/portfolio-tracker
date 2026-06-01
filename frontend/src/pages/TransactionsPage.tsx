import { useState, useEffect } from "react";
import ReactDOM from "react-dom";
import { Plus, Trash2, X, Check, RefreshCw } from "lucide-react";
import { toast } from "sonner";
import {
  useAccounts,
  useInstruments,
  useTransactions,
  useCreateTransaction,
  useDeleteTransaction,
} from "../api/hooks";
import { QueryState } from "../components/QueryState";

const TXN_TYPES = ["buy", "sell", "dividend", "interest", "fee", "deposit", "withdrawal", "opening_balance"];

const TX_TONE: Record<string, string> = {
  buy: "badge-gain",
  sell: "badge-loss",
  dividend: "badge-primary",
  interest: "badge-primary",
  fee: "badge-warn",
  deposit: "badge-gain",
  withdrawal: "badge-loss",
  opening_balance: "badge-neutral",
};

const TX_LABEL: Record<string, string> = {
  buy: "Beli",
  sell: "Jual",
  dividend: "Dividen",
  interest: "Bunga",
  fee: "Biaya",
  deposit: "Deposit",
  withdrawal: "Tarik",
  opening_balance: "Saldo Awal",
};

interface DialogProps {
  open: boolean;
  onClose: () => void;
  title: string;
  sub?: string;
  children: React.ReactNode;
  footer: React.ReactNode;
  width?: number;
}

function Dialog({ open, onClose, title, sub, children, footer, width }: DialogProps) {
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
      <div
        className="dialog"
        style={width ? { maxWidth: width } : undefined}
        role="dialog"
        aria-modal="true"
        aria-labelledby="dialog-title"
      >
        <div className="dialog-head">
          <div>
            <div className="t-h2" id="dialog-title">{title}</div>
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

export default function TransactionsPage() {
  const txns = useTransactions();
  const accounts = useAccounts();
  const instruments = useInstruments();
  const create = useCreateTransaction();
  const del = useDeleteTransaction();

  const [open, setOpen] = useState(false);
  const [confirmTx, setConfirmTx] = useState<null | (typeof txns_data)[number]>(null);

  const defaultForm = {
    account_id: "",
    instrument_id: "",
    txn_type: "buy",
    executed_at: new Date().toISOString().slice(0, 16),
    quantity: "",
    price_native: "",
    fee_native: "0",
    currency: "USD",
    fx_to_idr: "16000",
    fx_to_usd: "1",
  };
  const [form, setForm] = useState(defaultForm);

  const set = (k: string) => (e: React.ChangeEvent<HTMLInputElement | HTMLSelectElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const txns_data = txns.data ?? [];

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        account_id: Number(form.account_id),
        instrument_id: Number(form.instrument_id),
        txn_type: form.txn_type,
        executed_at: new Date(form.executed_at).toISOString(),
        quantity: form.quantity,
        price_native: form.price_native,
        fee_native: form.fee_native,
        currency: form.currency,
        fx_to_idr: form.fx_to_idr,
        fx_to_usd: form.fx_to_usd,
      },
      {
        onSuccess: () => {
          toast.success("Transaksi ditambahkan");
          setForm(defaultForm);
          setOpen(false);
        },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  };

  const handleDelete = () => {
    if (!confirmTx) return;
    del.mutate(confirmTx.id, {
      onSuccess: () => {
        toast.success("Transaksi dihapus");
        setConfirmTx(null);
      },
      onError: (err) => toast.error((err as Error).message),
    });
  };

  const accounts_data = accounts.data ?? [];
  const instruments_data = instruments.data ?? [];

  return (
    <div>
      {/* Header */}
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">Transaksi</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>{txns_data.length} catatan</div>
        </div>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          onClick={() => setOpen(true)}
          aria-label="Tambah transaksi"
        >
          <Plus size={15} />
          Tambah Transaksi
        </button>
      </div>

      {/* Table */}
      <QueryState isLoading={txns.isLoading} error={txns.error}>
        <div className="card">
          {txns_data.length === 0 ? (
            <div className="empty">
              <div className="empty-icon">
                <svg width="26" height="26" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M8 3 4 7l4 4"/><path d="M4 7h16"/><path d="m16 21 4-4-4-4"/><path d="M20 17H4"/>
                </svg>
              </div>
              <div>
                <div className="t-h3">Belum ada transaksi</div>
                <div className="t-sm t-muted" style={{ marginTop: 4 }}>
                  Tambahkan transaksi pertama untuk mulai melacak portofolio.
                </div>
              </div>
              <button
                type="button"
                className="btn btn-primary"
                onClick={() => setOpen(true)}
                aria-label="Tambah transaksi pertama"
              >
                <Plus size={16} />
                Tambah Transaksi
              </button>
            </div>
          ) : (
            <div className="table-wrap">
              <table className="tbl">
                <thead>
                  <tr>
                    <th>Tanggal</th>
                    <th>Tipe</th>
                    <th>Instrumen</th>
                    <th>Akun</th>
                    <th className="r">Jumlah</th>
                    <th className="r">Harga</th>
                    <th />
                  </tr>
                </thead>
                <tbody>
                  {txns_data.map((t) => (
                    <tr key={t.id}>
                      <td className="num t-muted">{t.executed_at.slice(0, 10)}</td>
                      <td>
                        <span className={"badge " + (TX_TONE[t.txn_type] ?? "badge-neutral")}>
                          {TX_LABEL[t.txn_type] ?? t.txn_type}
                        </span>
                      </td>
                      <td style={{ fontWeight: 580 }}>#{t.instrument_id}</td>
                      <td className="t-muted t-sm">#{t.account_id}</td>
                      <td className="r num">{t.quantity}</td>
                      <td className="r num">{t.price_native} {t.currency}</td>
                      <td className="r">
                        <button
                          type="button"
                          className="icon-btn"
                          style={{ width: 30, height: 30 }}
                          onClick={() => setConfirmTx(t)}
                          aria-label={`Hapus transaksi ${t.id}`}
                        >
                          <Trash2 size={15} />
                        </button>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </div>
      </QueryState>

      {/* Add Transaction Dialog */}
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Tambah Transaksi"
        sub="Catat aktivitas portofolio secara manual"
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
              aria-label={create.isPending ? "Menyimpan transaksi" : "Simpan transaksi"}
            >
              {create.isPending ? <RefreshCw size={16} className="animate-spin" /> : <Check size={16} />}
              {create.isPending ? "Menyimpan…" : "Simpan"}
            </button>
          </>
        }
      >
        <form id="tx-form" onSubmit={submit}>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Tipe</span>
              <select
                className="select"
                value={form.txn_type}
                onChange={set("txn_type")}
                aria-label="Tipe transaksi"
              >
                {TXN_TYPES.map((t) => (
                  <option key={t} value={t}>{TX_LABEL[t] ?? t}</option>
                ))}
              </select>
            </label>
            <label className="field">
              <span className="field-label">Akun</span>
              <select
                className="select"
                value={form.account_id}
                onChange={set("account_id")}
                aria-label="Akun"
              >
                <option value="">— pilih akun —</option>
                {accounts_data.map((a) => (
                  <option key={a.id} value={a.id}>{a.name}</option>
                ))}
              </select>
            </label>
          </div>
          <label className="field">
            <span className="field-label">Instrumen</span>
            <select
              className="select"
              value={form.instrument_id}
              onChange={set("instrument_id")}
              aria-label="Instrumen"
            >
              <option value="">— pilih instrumen —</option>
              {instruments_data.map((i) => (
                <option key={i.id} value={i.id}>{i.symbol}</option>
              ))}
            </select>
          </label>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Jumlah</span>
              <input
                type="number"
                className="input"
                placeholder="0"
                value={form.quantity}
                onChange={set("quantity")}
                aria-label="Jumlah"
              />
            </label>
            <label className="field">
              <span className="field-label">Harga</span>
              <input
                type="number"
                className="input"
                placeholder="0"
                value={form.price_native}
                onChange={set("price_native")}
                aria-label="Harga"
              />
            </label>
            <label className="field">
              <span className="field-label">Biaya</span>
              <input
                type="number"
                className="input"
                placeholder="0"
                value={form.fee_native}
                onChange={set("fee_native")}
                aria-label="Biaya"
              />
            </label>
          </div>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Mata Uang</span>
              <input
                className="input"
                placeholder="USD"
                value={form.currency}
                onChange={set("currency")}
                aria-label="Mata uang"
              />
            </label>
            <label className="field">
              <span className="field-label">Tanggal & Waktu</span>
              <input
                type="datetime-local"
                className="input"
                value={form.executed_at}
                onChange={set("executed_at")}
                aria-label="Tanggal dan waktu"
              />
            </label>
          </div>
          {create.error && (
            <div className="t-sm loss">{(create.error as Error).message}</div>
          )}
        </form>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog
        open={!!confirmTx}
        onClose={() => setConfirmTx(null)}
        title="Hapus transaksi?"
        sub={confirmTx ? `${TX_LABEL[confirmTx.txn_type] ?? confirmTx.txn_type} · ${confirmTx.executed_at.slice(0, 10)}` : ""}
        width={400}
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setConfirmTx(null)}>
              Batal
            </button>
            <button
              type="button"
              className="btn btn-danger"
              onClick={handleDelete}
              disabled={del.isPending}
              aria-label="Konfirmasi hapus transaksi"
            >
              <Trash2 size={16} />
              Hapus
            </button>
          </>
        }
      >
        <p className="t-sm t-muted" style={{ margin: 0 }}>
          Tindakan ini tidak dapat dibatalkan. Catatan akan dihapus permanen dari portofolio.
        </p>
      </Dialog>
    </div>
  );
}
