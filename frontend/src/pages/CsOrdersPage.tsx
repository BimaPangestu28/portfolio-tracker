import { useState } from "react";
import { X } from "lucide-react";
import { toast } from "sonner";
import { Dialog } from "@/components/Dialog";
import { useCsOrders, useUpsertOrder, useDeleteOrder } from "@/api/hooks";

const EMPTY = { external_ref: "", customer_name: "", customer_contact: "", status: "", details_json: "" };

export default function CsOrdersPage() {
  const orders = useCsOrders();
  const upsert = useUpsertOrder();
  const del = useDeleteOrder();

  const [open, setOpen] = useState(false);
  const [form, setForm] = useState(EMPTY);

  const set = (k: keyof typeof EMPTY) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.external_ref.trim() || !form.status.trim()) { toast.error("Ref & status wajib"); return; }
    upsert.mutate(
      {
        external_ref: form.external_ref.trim(),
        customer_name: form.customer_name || null,
        customer_contact: form.customer_contact || null,
        status: form.status.trim(),
        details_json: form.details_json || null,
      },
      {
        onSuccess: () => { toast.success("Tersimpan"); setOpen(false); setForm(EMPTY); },
        onError: (e) => toast.error((e as Error).message),
      },
    );
  };

  const list = orders.data ?? [];

  return (
    <div>
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">Order / Booking / Proyek</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Lacak status pesanan, booking, atau proyek/kontrak. Bot menjawab via referensi + kontak.</div>
        </div>
        <button
          type="button"
          className="btn btn-primary btn-sm"
          onClick={() => { setForm(EMPTY); setOpen(true); }}
        >
          Tambah / Update
        </button>
      </div>

      <div className="card">
        <div className="card-head">
          <div className="card-title">Daftar Order</div>
        </div>
        <div style={{ padding: "8px 0" }}>
          {list.length === 0 ? (
            <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>Belum ada order.</div>
          ) : (
            list.map((o) => (
              <div key={o.id} className="flex items-center" style={{ padding: "11px 20px", gap: 12, justifyContent: "space-between" }}>
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="t-sm" style={{ fontWeight: 500 }}>{o.external_ref} — {o.status}</div>
                  <div className="t-sm t-muted">{o.customer_name ?? "-"} · {o.customer_contact ?? "-"}</div>
                </div>
                <button
                  type="button"
                  className="icon-btn"
                  style={{ width: 26, height: 26, flexShrink: 0 }}
                  aria-label={`Hapus order ${o.external_ref}`}
                  onClick={() => del.mutate(o.id, {
                    onSuccess: () => toast.success("Dihapus"),
                    onError: (e) => toast.error((e as Error).message),
                  })}
                >
                  <X size={13} />
                </button>
              </div>
            ))
          )}
        </div>
      </div>

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title="Order"
        sub="Gunakan ref yang sama untuk memperbarui order yang ada"
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setOpen(false)}>Batal</button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={submit as unknown as React.MouseEventHandler}
              disabled={upsert.isPending}
            >
              Simpan
            </button>
          </>
        }
      >
        <form onSubmit={submit}>
          <label className="field">
            <span className="field-label">Ref order</span>
            <input className="input" value={form.external_ref} onChange={set("external_ref")} required />
          </label>
          <label className="field">
            <span className="field-label">Status</span>
            <input className="input" value={form.status} onChange={set("status")} placeholder="mis. diproses / dikirim / selesai" required />
          </label>
          <div className="grid" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Nama pelanggan</span>
              <input className="input" value={form.customer_name} onChange={set("customer_name")} />
            </label>
            <label className="field">
              <span className="field-label">Kontak (email/HP)</span>
              <input className="input" value={form.customer_contact} onChange={set("customer_contact")} />
            </label>
          </div>
          <label className="field">
            <span className="field-label">Detail (opsional)</span>
            <input className="input" value={form.details_json} onChange={set("details_json")} />
          </label>
        </form>
      </Dialog>
    </div>
  );
}
