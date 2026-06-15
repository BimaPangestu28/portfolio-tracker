import { useState } from "react";
import { Check, RotateCcw, Plus, Pencil } from "lucide-react";
import { toast } from "sonner";
import { useTodos, useCompleteTodo, useReopenTodo, useCreateTodo, useUpdateTodo } from "../../api/hooks";
import type { Todo } from "../../api/schemas";

const STATUSES: { key: string; label: string }[] = [
  { key: "open", label: "Terbuka" },
  { key: "done", label: "Selesai" },
  { key: "all", label: "Semua" },
];

export function TodoTab() {
  const [status, setStatus] = useState("open");
  const todos = useTodos(status);
  const complete = useCompleteTodo();
  const reopen = useReopenTodo();
  const create = useCreateTodo();
  const update = useUpdateTodo();
  const [title, setTitle] = useState("");
  const [editing, setEditing] = useState<Todo | null>(null);

  const rows = todos.data ?? [];

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed) return;
    create.mutate({ title: trimmed }, {
      onSuccess: () => { setTitle(""); toast.success("Todo ditambahkan"); },
      onError: (err) => toast.error((err as Error).message),
    });
  }

  function handleSaveEdit(e: React.FormEvent) {
    e.preventDefault();
    if (!editing) return;
    update.mutate(
      { id: editing.id, body: { title: editing.title, notes: editing.notes ?? null, due_at: editing.due_at ?? null, priority: editing.priority ?? null, estimate_minutes: editing.estimate_minutes ?? null } },
      {
        onSuccess: () => { setEditing(null); toast.success("Todo disimpan"); },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  }

  return (
    <div className="flex col gap-3">
      <div className="ptabs" role="group" aria-label="Filter status">
        {STATUSES.map((st) => (
          <button key={st.key} className={`ptab${status === st.key ? " active" : ""}`} onClick={() => setStatus(st.key)}>
            {st.label}
          </button>
        ))}
      </div>

      <form onSubmit={handleAdd} className="flex items-center gap-2">
        <input className="input flex-1" placeholder="Tambah todo…" aria-label="Tambah todo" value={title} onChange={(e) => setTitle(e.target.value)} />
        <button type="submit" aria-label="Simpan todo baru" className="pt-icon-btn shrink-0" disabled={create.isPending}><Plus size={15} /></button>
      </form>

      {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada todo.</p>}
      {rows.map((t) => (
        <div key={t.id} className="flex items-center gap-2 text-sm">
          {t.status === "done" ? (
            <button type="button" aria-label={`Buka lagi ${t.title}`} className="pt-icon-btn shrink-0" disabled={reopen.isPending}
              onClick={() => reopen.mutate(t.id, { onSuccess: () => toast.success("Todo dibuka lagi"), onError: (e) => toast.error((e as Error).message) })}>
              <RotateCcw size={15} />
            </button>
          ) : (
            <button type="button" aria-label={`Selesaikan ${t.title}`} className="pt-icon-btn shrink-0" disabled={complete.isPending}
              onClick={() => complete.mutate(t.id, { onSuccess: () => toast.success("Todo selesai"), onError: (e) => toast.error((e as Error).message) })}>
              <Check size={15} />
            </button>
          )}
          <span className={`flex-1 truncate${t.status === "done" ? " line-through text-muted-foreground" : ""}`}>{t.title}</span>
          {t.priority && <span className="text-xs text-muted-foreground">{t.priority}</span>}
          <button type="button" aria-label={`Edit ${t.title}`} className="pt-icon-btn shrink-0" onClick={() => setEditing(t)}><Pencil size={14} /></button>
        </div>
      ))}

      {editing && (
        <form onSubmit={handleSaveEdit} className="card card-pad flex col gap-2" style={{ padding: 14 }}>
          <div className="card-title">Edit todo</div>
          <input className="input" aria-label="Judul" value={editing.title} onChange={(e) => setEditing({ ...editing, title: e.target.value })} />
          <input className="input" aria-label="Catatan" placeholder="Catatan" value={editing.notes ?? ""} onChange={(e) => setEditing({ ...editing, notes: e.target.value })} />
          <input className="input" aria-label="Prioritas" placeholder="Prioritas (low/med/high)" value={editing.priority ?? ""} onChange={(e) => setEditing({ ...editing, priority: e.target.value })} />
          <div className="flex items-center gap-2">
            <button type="submit" className="btn btn-primary" disabled={update.isPending}>Simpan</button>
            <button type="button" className="btn" onClick={() => setEditing(null)}>Batal</button>
          </div>
        </form>
      )}
    </div>
  );
}
