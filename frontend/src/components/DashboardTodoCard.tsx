import { Link } from "react-router-dom";
import { useTodos } from "../api/hooks";
import { todayWibKey, wibDayKey, formatWibTime } from "../lib/wib";

const MAX_ROWS = 5;

export function DashboardTodoCard() {
  const todos = useTodos();
  const rows = (todos.data ?? []).slice(0, MAX_ROWS);

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
            <span className="text-muted-foreground w-24 shrink-0">
              {t.due_at
                ? (wibDayKey(t.due_at) === todayWibKey() ? "Hari ini" : wibDayKey(t.due_at).slice(5)) + " · " + formatWibTime(t.due_at)
                : "—"}
            </span>
            <span className="flex-1 truncate">{t.title}</span>
          </div>
        ))}
      </div>
    </div>
  );
}
