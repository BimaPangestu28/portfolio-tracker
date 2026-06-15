import { useState } from "react";
import { Link } from "react-router-dom";
import { Check, Plus } from "lucide-react";
import { toast } from "sonner";
import { useTodos, useCompleteTodo, useCreateTodo } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardTodoCard() {
  const todos = useTodos();
  const completeTodo = useCompleteTodo();
  const createTodo = useCreateTodo();
  const [title, setTitle] = useState("");

  const rows = (todos.data ?? []).slice(0, MAX_ROWS);

  function handleComplete(id: number) {
    completeTodo.mutate(id, {
      onSuccess: () => toast.success("Todo selesai"),
      onError: (err) => toast.error((err as Error).message),
    });
  }

  function handleAdd(e: React.FormEvent) {
    e.preventDefault();
    const trimmed = title.trim();
    if (!trimmed) return;
    createTodo.mutate(
      { title: trimmed },
      {
        onSuccess: () => { setTitle(""); toast.success("Todo ditambahkan"); },
        onError: (err) => toast.error((err as Error).message),
      },
    );
  }

  return (
    <div className="card">
      <div className="card-head flex items-center justify-between">
        <div className="card-title">Todo hari ini</div>
        <Link to="/chat" className="text-sm text-primary hover:underline">Tanya Noah →</Link>
      </div>
      <div className="card-pad space-y-1" style={{ paddingTop: 14 }}>
        {rows.length === 0 && <p className="text-sm text-muted-foreground">Tidak ada todo terbuka.</p>}
        {rows.map((t) => (
          <div key={t.id} className="flex items-center gap-2 text-sm">
            <button
              type="button"
              aria-label={`Selesaikan ${t.title}`}
              className="pt-icon-btn shrink-0"
              disabled={completeTodo.isPending}
              onClick={() => handleComplete(t.id)}
            >
              <Check size={15} />
            </button>
            <span className="text-muted-foreground w-24 shrink-0">
              {t.due_at
                ? (wibDayKey(t.due_at) === todayWibKey() ? "Hari ini" : wibDayKey(t.due_at).slice(5)) + " · " + formatWibTime(t.due_at)
                : "—"}
            </span>
            <span className="flex-1 truncate">{t.title}</span>
          </div>
        ))}
        <form onSubmit={handleAdd} className="flex items-center gap-2" style={{ paddingTop: 8 }}>
          <input
            type="text"
            className="input flex-1"
            placeholder="Tambah todo…"
            aria-label="Tambah todo"
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
          <button type="submit" aria-label="Simpan todo baru" className="pt-icon-btn shrink-0" disabled={createTodo.isPending}>
            <Plus size={15} />
          </button>
        </form>
      </div>
    </div>
  );
}
