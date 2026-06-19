import { useState } from "react";
import { X } from "lucide-react";
import { toast } from "sonner";
import { Dialog } from "@/components/Dialog";
import { useCsProducts, useCreateProduct, useUpdateProduct, useSetProductActive, useDeleteProduct } from "@/api/hooks";
import type { CsProduct } from "@/api/schemas";
import { NumberInput } from "@/components/ui/NumberInput";

const EMPTY = { name: "", description: "", price: "", currency: "IDR", availability: "" };

export default function CsPricingPage() {
  const products = useCsProducts();
  const create = useCreateProduct();
  const update = useUpdateProduct();
  const setActive = useSetProductActive();
  const del = useDeleteProduct();

  const [open, setOpen] = useState(false);
  const [editId, setEditId] = useState<number | null>(null);
  const [form, setForm] = useState(EMPTY);

  const set = (k: keyof typeof EMPTY) => (e: React.ChangeEvent<HTMLInputElement>) =>
    setForm({ ...form, [k]: e.target.value });

  const openCreate = () => { setEditId(null); setForm(EMPTY); setOpen(true); };
  const openEdit = (p: CsProduct) => {
    setEditId(p.id);
    setForm({
      name: p.name,
      description: p.description ?? "",
      price: p.price?.toString() ?? "",
      currency: p.currency ?? "IDR",
      availability: p.availability ?? "",
    });
    setOpen(true);
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.name.trim()) { toast.error("Nama wajib diisi"); return; }
    const body = {
      name: form.name.trim(),
      description: form.description || null,
      price: form.price ? Number(form.price) : null,
      currency: form.currency || null,
      availability: form.availability || null,
    };
    const onDone = {
      onSuccess: () => { toast.success("Tersimpan"); setOpen(false); },
      onError: (e: unknown) => toast.error((e as Error).message),
    };
    if (editId == null) create.mutate(body, onDone);
    else update.mutate({ id: editId, body }, onDone);
  };

  const list = products.data ?? [];

  return (
    <div>
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">Harga / Paket</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Kelola produk &amp; paket yang dijual</div>
        </div>
        <button type="button" className="btn btn-primary btn-sm" onClick={openCreate}>
          Tambah
        </button>
      </div>

      <div className="card">
        <div className="card-head">
          <div className="card-title">Daftar Produk</div>
        </div>
        <div style={{ padding: "8px 0" }}>
          {list.length === 0 ? (
            <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>Belum ada produk.</div>
          ) : (
            list.map((p) => (
              <div key={p.id} className="flex items-center" style={{ padding: "11px 20px", gap: 12, justifyContent: "space-between" }}>
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="t-sm" style={{ fontWeight: 500 }}>
                    {p.name} {!p.active ? <span className="t-muted">(nonaktif)</span> : null}
                  </div>
                  <div className="t-sm t-muted">
                    {p.currency} {p.price ?? "-"} · {p.availability ?? "-"}
                  </div>
                </div>
                <div className="flex items-center" style={{ gap: 8, flexShrink: 0 }}>
                  <button type="button" className="btn btn-outline btn-sm" onClick={() => openEdit(p)}>Edit</button>
                  <button
                    type="button"
                    className="btn btn-outline btn-sm"
                    onClick={() => setActive.mutate(
                      { id: p.id, active: !p.active },
                      { onError: (e) => toast.error((e as Error).message) },
                    )}
                  >
                    {p.active ? "Nonaktifkan" : "Aktifkan"}
                  </button>
                  <button
                    type="button"
                    className="icon-btn"
                    style={{ width: 26, height: 26 }}
                    aria-label={`Hapus produk ${p.name}`}
                    onClick={() => del.mutate(p.id, {
                      onSuccess: () => toast.success("Dihapus"),
                      onError: (e) => toast.error((e as Error).message),
                    })}
                  >
                    <X size={13} />
                  </button>
                </div>
              </div>
            ))
          )}
        </div>
      </div>

      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={editId == null ? "Tambah Produk" : "Edit Produk"}
        footer={
          <>
            <button type="button" className="btn btn-outline" onClick={() => setOpen(false)}>Batal</button>
            <button
              type="button"
              className="btn btn-primary"
              onClick={submit as unknown as React.MouseEventHandler}
              disabled={create.isPending || update.isPending}
            >
              Simpan
            </button>
          </>
        }
      >
        <form onSubmit={submit}>
          <label className="field">
            <span className="field-label">Nama</span>
            <input className="input" value={form.name} onChange={set("name")} required />
          </label>
          <label className="field">
            <span className="field-label">Deskripsi</span>
            <input className="input" value={form.description} onChange={set("description")} />
          </label>
          <div className="grid form-stack" style={{ gridTemplateColumns: "1fr 1fr", gap: 12 }}>
            <label className="field">
              <span className="field-label">Harga</span>
              <NumberInput className="input" value={form.price} onChange={(v) => setForm({ ...form, price: v })} />
            </label>
            <label className="field">
              <span className="field-label">Mata uang</span>
              <input className="input" value={form.currency} onChange={set("currency")} />
            </label>
          </div>
          <label className="field">
            <span className="field-label">Ketersediaan</span>
            <input className="input" value={form.availability} onChange={set("availability")} />
          </label>
        </form>
      </Dialog>
    </div>
  );
}
