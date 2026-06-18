import { useState } from "react";
import { X } from "lucide-react";
import { toast } from "sonner";
import { Dialog } from "@/components/Dialog";
import { useKbDocs, useCreateDoc, useUpdateDoc, useDeleteDoc, useReindexKb } from "@/api/hooks";
import type { KbDoc } from "@/api/schemas";

const EMPTY = { title: "", source: "", body: "" };

export default function CsDocsPage() {
  const docs = useKbDocs();
  const create = useCreateDoc();
  const update = useUpdateDoc();
  const del = useDeleteDoc();
  const reindex = useReindexKb();

  const [open, setOpen] = useState(false);
  const [editId, setEditId] = useState<number | null>(null);
  const [form, setForm] = useState(EMPTY);

  const openCreate = () => { setEditId(null); setForm(EMPTY); setOpen(true); };
  const openEdit = (d: KbDoc) => {
    setEditId(d.id);
    setForm({ title: d.title, source: d.source ?? "", body: d.body });
    setOpen(true);
  };

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!form.title.trim() || !form.body.trim()) { toast.error("Judul & isi wajib"); return; }
    const body = { title: form.title.trim(), source: form.source || null, body: form.body };
    const onDone = {
      onSuccess: () => { toast.success("Tersimpan"); setOpen(false); },
      onError: (e: unknown) => toast.error((e as Error).message),
    };
    if (editId == null) create.mutate(body, onDone);
    else update.mutate({ id: editId, body }, onDone);
  };

  const list = docs.data ?? [];

  return (
    <div>
      <div className="flex items-center justify-between" style={{ marginBottom: 18, flexWrap: "wrap", gap: 10 }}>
        <div>
          <h1 className="t-h1">Knowledge Base</h1>
          <div className="t-sm t-muted" style={{ marginTop: 2 }}>Dokumen referensi untuk bot CS</div>
        </div>
        <div className="flex items-center" style={{ gap: 8 }}>
          <button
            type="button"
            className="btn btn-outline btn-sm"
            disabled={reindex.isPending}
            onClick={() =>
              reindex.mutate(undefined, {
                onSuccess: (r) => toast.success(`Re-embed ${(r as { embedded: number }).embedded} potongan`),
                onError: (e) => toast.error((e as Error).message),
              })
            }
          >
            Reindex
          </button>
          <button type="button" className="btn btn-primary btn-sm" onClick={openCreate}>
            Tambah
          </button>
        </div>
      </div>

      <div className="card">
        <div className="card-head">
          <div className="card-title">Daftar Dokumen</div>
        </div>
        <div style={{ padding: "8px 0" }}>
          {list.length === 0 ? (
            <div className="t-sm t-muted" style={{ padding: "16px 20px" }}>Belum ada dokumen.</div>
          ) : (
            list.map((d) => (
              <div key={d.id} className="flex items-center" style={{ padding: "11px 20px", gap: 12, justifyContent: "space-between" }}>
                <div style={{ minWidth: 0, flex: 1 }}>
                  <div className="t-sm" style={{ fontWeight: 500 }}>{d.title}</div>
                  <div className="t-sm t-muted truncate">{d.body.slice(0, 80)}{d.body.length > 80 ? "…" : ""}</div>
                </div>
                <div className="flex items-center" style={{ gap: 8, flexShrink: 0 }}>
                  <button type="button" className="btn btn-outline btn-sm" onClick={() => openEdit(d)}>Edit</button>
                  <button
                    type="button"
                    className="icon-btn"
                    style={{ width: 26, height: 26 }}
                    aria-label={`Hapus dokumen ${d.title}`}
                    onClick={() => del.mutate(d.id, {
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
        title={editId == null ? "Tambah Dokumen" : "Edit Dokumen"}
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
            <span className="field-label">Judul</span>
            <input className="input" value={form.title} onChange={(e) => setForm({ ...form, title: e.target.value })} required />
          </label>
          <label className="field">
            <span className="field-label">Sumber (opsional)</span>
            <input className="input" value={form.source} onChange={(e) => setForm({ ...form, source: e.target.value })} />
          </label>
          <label className="field">
            <span className="field-label">Isi</span>
            <textarea
              className="input"
              rows={8}
              value={form.body}
              onChange={(e) => setForm({ ...form, body: e.target.value })}
              style={{ resize: "vertical" }}
            />
          </label>
        </form>
      </Dialog>
    </div>
  );
}
