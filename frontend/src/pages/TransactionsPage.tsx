import { useState } from "react";
import { Plus, Trash2 } from "lucide-react";
import { toast } from "sonner";
import { useTransactions, useDeleteTransaction } from "../api/hooks";
import { QueryState } from "../components/QueryState";
import { Dialog } from "../components/Dialog";
import { AddTransactionDialog } from "../components/AddTransactionDialog";
import { txTone, txLabel } from "../lib/txn";

export default function TransactionsPage() {
  const txns = useTransactions();
  const del = useDeleteTransaction();

  const [open, setOpen] = useState(false);
  const [confirmTx, setConfirmTx] = useState<null | (typeof txns_data)[number]>(null);

  const txns_data = txns.data ?? [];

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
                        <span className={"badge " + txTone(t.txn_type)}>
                          {txLabel(t.txn_type)}
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
      <AddTransactionDialog open={open} onClose={() => setOpen(false)} />

      {/* Delete Confirmation Dialog */}
      <Dialog
        open={!!confirmTx}
        onClose={() => setConfirmTx(null)}
        title="Hapus transaksi?"
        sub={confirmTx ? `${txLabel(confirmTx.txn_type)} · ${confirmTx.executed_at.slice(0, 10)}` : ""}
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
